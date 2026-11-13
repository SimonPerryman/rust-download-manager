//! One download attempt: probe the server, resume what's on disk, or start
//! fresh on one lane or several.

use std::path::{Path, PathBuf};

use reqwest::header;
use reqwest::StatusCode;

use super::http::{header_str, send, status_error};
use super::ramp::Ramping;
use super::segment::{self, Failure};
use super::single::{resume, stream_to_part};
use super::wording::{announce_resume, announce_start, size_text};
use super::{
    filename, human, state, Control, DownloadRequest, DownloadState, Error, Progress, Result,
};

/// What earlier attempts of one download learned about its server.
#[derive(Debug, Default)]
pub(super) struct Learned {
    /// It doesn't really honour ranges: stay on a single connection.
    pub single: bool,
    /// It refused a request with 429 or a `Retry-After`: fetch the rest in
    /// one request, since each new one may cost another wait.
    pub rate_limited: bool,
}

/// One attempt, re-run single-connection (without consuming a retry) if a
/// segmented run finds the server doesn't really honour ranges.
pub(super) async fn attempt_download(
    client: &reqwest::Client,
    req: &DownloadRequest,
    control: &Control,
    known_target: &mut Option<PathBuf>,
    learned: &mut Learned,
    segmented: &mut bool,
    on_progress: &mut impl FnMut(Progress),
) -> Result<(u16, PathBuf, u64)> {
    loop {
        let result = attempt_once(
            client,
            req,
            control,
            known_target,
            learned,
            segmented,
            on_progress,
        )
        .await;
        match result {
            Ok(done) => return Ok(done),
            Err(Failure::Engine(e)) => return Err(e),
            // The segmented run already discarded its sidecar and `.part`.
            Err(Failure::RangeUnsupported) => {
                learned.single = true;
                *segmented = false;
            }
        }
    }
}

/// What a previous run left on disk for a target.
enum Existing {
    /// A v1 sidecar: resume from the `.part` size.
    Single(DownloadState),
    /// A v2 sidecar with a full-size `.part`: resume the missing ranges.
    Segmented(DownloadState),
    /// Nothing usable.
    Absent,
}

/// Inspect `.part` + sidecar. A v2 `.part` is always pre-allocated to the full
/// size, so any other length means the pair is inconsistent and is discarded,
/// as is a `.part` whose sidecar is invalid (its contents can't be trusted).
/// A sidecar from a newer rdm is [`Error::UnsupportedState`] and kept.
fn existing_state(part: &Path, sidecar: &Path) -> Result<Existing> {
    if !(part.exists() && sidecar.exists()) {
        return Ok(Existing::Absent);
    }
    let st = match DownloadState::load(sidecar) {
        Ok(st) => st,
        Err(e @ Error::UnsupportedState(_)) => return Err(e),
        Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::InvalidData => {
            let _ = std::fs::remove_file(part);
            let _ = std::fs::remove_file(sidecar);
            return Ok(Existing::Absent);
        }
        Err(_) => return Ok(Existing::Absent),
    };
    if st.segments.is_none() {
        return Ok(Existing::Single(st));
    }
    let part_len = std::fs::metadata(part).ok().map(|m| m.len());
    if part_len.is_some() && part_len == st.total {
        Ok(Existing::Segmented(st))
    } else {
        let _ = std::fs::remove_file(part);
        let _ = std::fs::remove_file(sidecar);
        Ok(Existing::Absent)
    }
}

/// The total size, if this fresh response can be fetched as parallel ranges:
/// a plain 200 of known size, ranges advertised, a validator to pin the bytes
/// with `If-Range`, and big enough for at least two segments.
fn segmentable(req: &DownloadRequest, resp: &reqwest::Response) -> Option<u64> {
    let headers = resp.headers();
    let accepts_ranges = header_str(headers, header::ACCEPT_RANGES)
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("bytes"));
    let strong_etag =
        header_str(headers, header::ETAG).is_some_and(|e| segment::is_strong_etag(&e));
    let validated = strong_etag || headers.contains_key(header::LAST_MODIFIED);
    let total = resp.content_length()?;
    let eligible = req.effective_connections() > 1
        && resp.status() == StatusCode::OK
        && !headers.contains_key(header::CONTENT_ENCODING)
        && accepts_ranges
        && validated
        && total >= req.min_segment_size.saturating_mul(2);
    eligible.then_some(total)
}

/// One connect+stream attempt. Returns `(status, part_path, part_len)` on a
/// completed stream (body fully read), or an error to be classified by the
/// retry loop. `segmented` is set when the attempt ran multi-segment.
async fn attempt_once(
    client: &reqwest::Client,
    req: &DownloadRequest,
    control: &Control,
    known_target: &mut Option<PathBuf>,
    learned: &Learned,
    segmented: &mut bool,
    on_progress: &mut impl FnMut(Progress),
) -> std::result::Result<(u16, PathBuf, u64), Failure> {
    // If we already know the target and a prior `.part`+sidecar exist, resume.
    if let Some(target) = known_target.as_deref() {
        let part = state::part_path(target);
        let sidecar = state::sidecar_path(target);
        match existing_state(&part, &sidecar)? {
            Existing::Single(st) => {
                return Ok(resume(
                    client,
                    req,
                    control,
                    target,
                    &part,
                    &sidecar,
                    st,
                    on_progress,
                )
                .await?);
            }
            Existing::Segmented(st) => {
                *segmented = true;
                announce_resume(req, st.segments.iter().flatten().map(|s| s.done).sum());
                return resume_segmented(
                    client,
                    req,
                    control,
                    &part,
                    &sidecar,
                    st,
                    learned.rate_limited,
                    on_progress,
                )
                .await;
            }
            Existing::Absent => {}
        }
    }

    // FRESH: issue a plain GET so we can learn the (post-redirect) URL and the
    // derived filename before committing to a target path.
    let resp = send(control, client.get(&req.url), req.read_timeout).await?;
    let status = resp.status();
    tracing::debug!(
        "Requested the file · HTTP {} · {}",
        status.as_u16(),
        size_text(resp.content_length())
    );
    if !status.is_success() {
        return Err(status_error(&resp).into());
    }

    let final_url = resp.url().clone();
    let content_disposition = header_str(resp.headers(), header::CONTENT_DISPOSITION);
    let total = resp.content_length();
    let etag = header_str(resp.headers(), header::ETAG);
    let last_modified = header_str(resp.headers(), header::LAST_MODIFIED);

    let target = match known_target.as_ref() {
        Some(known) => known.clone(),
        None => {
            let name = filename::resolve(None, final_url.as_str(), content_disposition.as_deref());
            let derived = filename::join_in(&req.dir, &name);
            let chosen = match &req.claim_target {
                Some(claim) => claim.claim(&derived),
                None => derived,
            };
            state::remove_stale_temps(&state::sidecar_path(&chosen));
            *known_target = Some(chosen.clone());
            chosen
        }
    };
    ensure_parent(&target).await?;

    let part = state::part_path(&target);
    let sidecar = state::sidecar_path(&target);

    // A resumable sidecar may exist even when the target wasn't known up front
    // (derived filename). If so, and the resource is unchanged, resume rather
    // than restart. Otherwise write a fresh sidecar BEFORE any bytes stream.
    match existing_state(&part, &sidecar)? {
        Existing::Single(st) => {
            return Ok(resume(
                client,
                req,
                control,
                &target,
                &part,
                &sidecar,
                st,
                on_progress,
            )
            .await?);
        }
        Existing::Segmented(st) => {
            *segmented = true;
            announce_resume(req, st.segments.iter().flatten().map(|s| s.done).sum());
            return resume_segmented(
                client,
                req,
                control,
                &part,
                &sidecar,
                st,
                learned.rate_limited,
                on_progress,
            )
            .await;
        }
        Existing::Absent => {}
    }

    if !learned.single {
        if let Some(total) = segmentable(req, &resp) {
            *segmented = true;
            let segments = segment::plan(total, req.effective_connections(), req.min_segment_size);
            let workers = segments.len();
            let pieces = human::count(workers as u32, "piece", "pieces");
            let lanes = match req.ramp {
                _ if learned.rate_limited => format!("{pieces}, 1 lane"),
                Ramping::Window(_) => format!("{pieces}, up to {workers} lanes"),
                Ramping::Off => format!("{pieces}, {workers} lanes"),
            };
            announce_start(req, &resp, &lanes);
            let st = DownloadState::segmented(
                &req.url,
                &target,
                total,
                etag,
                last_modified,
                segments,
                state::new_owner_tag(),
            );
            st.save(&sidecar)?;
            let file = tokio::fs::File::create(&part).await?;
            file.set_len(total).await?;
            drop(file);

            let job = segment::Job {
                client,
                control,
                url: final_url.to_string(),
                redirected: reqwest::Url::parse(&req.url).ok().as_ref() != Some(&final_url),
                part: &part,
                sidecar: &sidecar,
                state: st,
                workers,
                min_segment_size: req.min_segment_size,
                read_timeout: req.read_timeout,
                ramp: req.ramp,
                shed_grace: req.shed_grace,
                reprobe_interval: req.reprobe_interval,
                first: Some(resp),
                one_request: learned.rate_limited,
                tally: &req.tally,
            };
            let bytes = segment::run(job, on_progress).await?;
            return Ok((StatusCode::PARTIAL_CONTENT.as_u16(), part, bytes));
        }
    }

    announce_start(req, &resp, "1 lane");
    // Truncate any stale `.part` before the v1 sidecar lands: a kill in between
    // must not leave a sidecar that resumes from leftover bytes.
    let file = tokio::fs::File::create(&part).await?;
    let st = DownloadState::new(&req.url, &target, total, etag, last_modified);
    st.save(&sidecar)?;

    let bytes = stream_to_part(
        resp,
        file,
        control,
        req.read_timeout,
        &part,
        &sidecar,
        0,
        total,
        on_progress,
    )
    .await?;
    Ok((status.as_u16(), part, bytes))
}

/// Resume a multi-segment download from its v2 sidecar: fetch only the
/// missing ranges, with up to `connections` workers (redirects re-followed
/// from the original URL), or in one request once the host has rate-limited.
#[allow(clippy::too_many_arguments)]
async fn resume_segmented(
    client: &reqwest::Client,
    req: &DownloadRequest,
    control: &Control,
    part: &Path,
    sidecar: &Path,
    st: DownloadState,
    one_request: bool,
    on_progress: &mut impl FnMut(Progress),
) -> std::result::Result<(u16, PathBuf, u64), Failure> {
    let job = segment::Job {
        client,
        control,
        url: st.url.clone(),
        redirected: false,
        part,
        sidecar,
        state: st,
        workers: usize::from(req.effective_connections()),
        min_segment_size: req.min_segment_size,
        read_timeout: req.read_timeout,
        ramp: req.ramp,
        shed_grace: req.shed_grace,
        reprobe_interval: req.reprobe_interval,
        first: None,
        one_request,
        tally: &req.tally,
    };
    let bytes = segment::run(job, on_progress).await?;
    Ok((
        StatusCode::PARTIAL_CONTENT.as_u16(),
        part.to_path_buf(),
        bytes,
    ))
}

/// Create the parent directory of `path` if it isn't the cwd.
async fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
    Ok(())
}
