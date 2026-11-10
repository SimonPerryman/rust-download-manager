//! Single-connection fetching: resuming a `.part` and streaming a body into it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reqwest::header;
use reqwest::StatusCode;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use super::http::{header_str, next_chunk, send, status_error};
use super::wording::{announce_resume, restarting};
use super::{
    progress, state, Control, DownloadRequest, DownloadState, Error, Flow, Progress, Result,
    Segment, SpeedMeter,
};

/// Resume an existing `.part`. Offset is the live `.part` size. A genuine 206
/// appends; a 200 means restart (changed resource or a range-ignoring/lying
/// server); a 416 means already-complete-or-restart.
#[allow(clippy::too_many_arguments)]
pub(super) async fn resume(
    client: &reqwest::Client,
    req: &DownloadRequest,
    control: &Control,
    target: &Path,
    part: &Path,
    sidecar: &Path,
    st: DownloadState,
    on_progress: &mut impl FnMut(Progress),
) -> Result<(u16, PathBuf, u64)> {
    // The offset comes from the handle the bytes will be appended through, not
    // a separate by-path lookup, so the requested range starts exactly where
    // the append lands; a short offset would re-request bytes already held.
    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .open(part)
        .await?;
    let offset = file.metadata().await?.len();

    let mut builder = client
        .get(&st.url)
        .header(header::RANGE, format!("bytes={offset}-"));
    if let Some(validator) = st.if_range() {
        builder = builder.header(header::IF_RANGE, validator);
    }
    let resp = send(control, builder, req.read_timeout).await?;
    let status = resp.status();
    tracing::debug!("Requested bytes {offset}– · HTTP {}", status.as_u16());

    match status {
        StatusCode::PARTIAL_CONTENT => {
            // Resource unchanged: append from the current offset.
            announce_resume(req, offset);
            let total = st.total;
            file.seek(std::io::SeekFrom::End(0)).await?;
            let bytes = stream_to_part(
                resp,
                file,
                control,
                req.read_timeout,
                part,
                sidecar,
                offset,
                total,
                on_progress,
            )
            .await?;
            Ok((status.as_u16(), part.to_path_buf(), bytes))
        }
        StatusCode::OK => {
            // 200 to a Range request: the resource changed, or the server
            // ignores/lies about ranges. Only a true 206 is safe to append, so
            // truncate and restart with fresh validators.
            restarting(req, offset, "the server sent the whole file again");
            drop(file);
            let total = resp.content_length();
            let etag = header_str(resp.headers(), header::ETAG);
            let last_modified = header_str(resp.headers(), header::LAST_MODIFIED);
            // Truncate before the new validators land, as on the fresh path.
            let file = tokio::fs::File::create(part).await?;
            let fresh = DownloadState::new(&st.url, target, total, etag, last_modified);
            fresh.save(sidecar)?;

            let bytes = stream_to_part(
                resp,
                file,
                control,
                req.read_timeout,
                part,
                sidecar,
                0,
                total,
                on_progress,
            )
            .await?;
            Ok((status.as_u16(), part.to_path_buf(), bytes))
        }
        StatusCode::RANGE_NOT_SATISFIABLE => {
            drop(file);
            // Offset past the end. If we already have the whole thing, we're
            // done; otherwise the `.part` is inconsistent — restart.
            if let Some(total) = st.total {
                if offset >= total {
                    announce_resume(req, offset);
                    on_progress(Progress {
                        downloaded: offset,
                        total: Some(total),
                        bytes_per_sec: 0.0,
                        eta: None,
                        connections: 0,
                        segments: single_segment(offset, Some(total)),
                        lanes: vec![None],
                    });
                    return Ok((status.as_u16(), part.to_path_buf(), offset));
                }
            }
            restarting(req, offset, "the saved part doesn't match the file");
            tokio::fs::File::create(part).await?; // truncate
            restart(client, req, control, target, part, sidecar, on_progress).await
        }
        _ => Err(status_error(&resp)),
    }
}

/// Fresh GET + stream after a 416-triggered restart (target already resolved).
#[allow(clippy::too_many_arguments)]
async fn restart(
    client: &reqwest::Client,
    req: &DownloadRequest,
    control: &Control,
    target: &Path,
    part: &Path,
    sidecar: &Path,
    on_progress: &mut impl FnMut(Progress),
) -> Result<(u16, PathBuf, u64)> {
    let url = {
        // Re-read the URL from the sidecar (the request's url matches it).
        let st = DownloadState::load(sidecar)?;
        st.url
    };
    let resp = send(control, client.get(&url), req.read_timeout).await?;
    let status = resp.status();
    tracing::debug!("Requested the file again · HTTP {}", status.as_u16());
    if !status.is_success() {
        return Err(status_error(&resp));
    }
    let total = resp.content_length();
    let etag = header_str(resp.headers(), header::ETAG);
    let last_modified = header_str(resp.headers(), header::LAST_MODIFIED);
    let fresh = DownloadState::new(&url, target, total, etag, last_modified);
    fresh.save(sidecar)?;

    let file = tokio::fs::File::create(part).await?;
    let bytes = stream_to_part(
        resp,
        file,
        control,
        req.read_timeout,
        part,
        sidecar,
        0,
        total,
        on_progress,
    )
    .await?;
    Ok((status.as_u16(), part.to_path_buf(), bytes))
}

/// Stream a response body into an open `.part` file, honouring cancel/pause and
/// the throttle hook. `start_offset` is how many bytes are already on disk (for
/// progress accounting); the returned count is the total `.part` length.
///
/// On cancel the `.part` + `sidecar` are removed (download discarded); on pause
/// the `.part` is flushed and both are kept so it can be resumed.
#[allow(clippy::too_many_arguments)]
pub(super) async fn stream_to_part(
    resp: reqwest::Response,
    mut file: tokio::fs::File,
    control: &Control,
    read_timeout: Duration,
    part: &Path,
    sidecar: &Path,
    start_offset: u64,
    total: Option<u64>,
    on_progress: &mut impl FnMut(Progress),
) -> Result<u64> {
    let mut downloaded = start_offset;
    let mut meter = SpeedMeter::new();
    let start = Instant::now();

    on_progress(Progress {
        downloaded,
        total,
        bytes_per_sec: 0.0,
        eta: None,
        connections: 1,
        segments: single_segment(downloaded, total),
        lanes: single_lane(total),
    });

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = next_chunk(&mut stream, read_timeout).await {
        // Control point: honoured once per chunk, before the write.
        match control.poll() {
            Flow::Cancel => {
                drop(file);
                let _ = tokio::fs::remove_file(part).await;
                let _ = tokio::fs::remove_file(sidecar).await;
                state::remove_stale_temps(sidecar);
                return Err(Error::Cancelled);
            }
            Flow::Pause => {
                file.flush().await?;
                return Err(Error::Paused);
            }
            Flow::Continue => {}
        }

        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        // Throttle point: sleeps off rate-limit debt; pause/cancel cut it short.
        control.throttle(chunk.len(), None).await;

        let bytes_per_sec = meter.observe(start.elapsed(), downloaded);
        on_progress(Progress {
            downloaded,
            total,
            bytes_per_sec,
            eta: progress::eta(downloaded, total, bytes_per_sec),
            connections: 1,
            segments: single_segment(downloaded, total),
            lanes: single_lane(total),
        });
    }

    file.flush().await?;
    Ok(downloaded)
}

/// The one lane of a single-connection download, on its one segment (none
/// while the size is unknown, as in [`single_segment`]).
fn single_lane(total: Option<u64>) -> Vec<Option<u8>> {
    total.map(|_| Some(1)).into_iter().collect()
}

/// A single connection's progress as one segment over the whole file, or none
/// while the size is unknown.
pub(crate) fn single_segment(downloaded: u64, total: Option<u64>) -> Vec<Segment> {
    total
        .map(|total| Segment {
            start: 0,
            end: total,
            done: downloaded.min(total),
        })
        .into_iter()
        .collect()
}
