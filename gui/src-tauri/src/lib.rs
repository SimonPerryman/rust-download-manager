//! Tauri glue for the rdm desktop frontend.
//!
//! This layer is deliberately thin: it owns a single [`rdm::Manager`], exposes
//! each of its methods as a Tauri command (1–3 lines of pass-through each), and
//! re-broadcasts the manager's snapshot stream to the webview as a
//! `downloads-changed` event. All download logic lives in the engine — this
//! crate contains none.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use rdm::{AddOptions, Checksum, DownloadSnapshot, JsonStore, Manager, Settings, Status};
use tauri::{Emitter, Manager as _, State};
use tauri_plugin_opener::OpenerExt;

/// Shared application state: the one engine [`Manager`] every command drives.
struct AppState {
    manager: Arc<Manager>,
}

/// One-shot snapshot of every download, in display order.
#[tauri::command]
fn list(state: State<'_, AppState>) -> Vec<DownloadSnapshot> {
    state.manager.snapshot()
}

/// Add a download. `dir` defaults to [`default_dir`]; `checksum`, if
/// given, must parse as `"sha256:<hex>"`; `limit` is a rate like `"500K"`
/// (blank = unlimited). Returns the new download's id.
#[tauri::command]
fn add_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    url: String,
    dir: Option<String>,
    checksum: Option<String>,
    limit: Option<String>,
) -> Result<u64, String> {
    let dir = match dir {
        Some(d) if !d.trim().is_empty() => PathBuf::from(d),
        _ => default_dir(&app)?,
    };
    let checksum = match checksum {
        Some(c) if !c.trim().is_empty() => {
            Some(Checksum::from_str(c.trim()).map_err(|e| e.to_string())?)
        }
        _ => None,
    };
    let options = AddOptions {
        checksum,
        limit: parse_limit(limit)?,
        ..AddOptions::default()
    };
    Ok(state.manager.add_with(url, dir, options))
}

/// Parse an optional rate string (`"2M"`, `"500K"`); blank or absent is
/// unlimited.
fn parse_limit(limit: Option<String>) -> Result<Option<u64>, String> {
    match limit {
        Some(l) if !l.trim().is_empty() => rdm::parse_rate(l.trim()).map(Some),
        _ => Ok(None),
    }
}

/// The engine's current manager-wide settings.
#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.manager.settings()
}

/// Update some of the manager-wide settings: an absent field is left as is,
/// so concurrent edits of different fields can't clobber each other.
/// `global_limit` is a rate string (blank = unlimited), validated before
/// anything is applied. Returns the settings as the engine clamped them.
#[tauri::command]
fn set_settings(
    state: State<'_, AppState>,
    max_concurrent: Option<usize>,
    global_limit: Option<String>,
    max_connections: Option<u8>,
) -> Result<Settings, String> {
    let global_limit = global_limit.map(|l| parse_limit(Some(l))).transpose()?;
    if let Some(n) = max_concurrent {
        state.manager.set_max_concurrent(n);
    }
    if let Some(limit) = global_limit {
        state.manager.set_global_limit(limit);
    }
    if let Some(n) = max_connections {
        state.manager.set_max_connections(n);
    }
    Ok(state.manager.settings())
}

/// Cap one download's speed; `limit` is a rate string (blank or absent =
/// unlimited). Applies live and persists. Returns the limit as the engine
/// stored it.
#[tauri::command]
fn set_limit(
    state: State<'_, AppState>,
    id: u64,
    limit: Option<String>,
) -> Result<Option<u64>, String> {
    let limit = parse_limit(limit)?;
    state.manager.set_limit(id, limit);
    state
        .manager
        .snapshot()
        .into_iter()
        .find(|d| d.id == id)
        .map(|d| d.limit)
        .ok_or_else(|| format!("no download with id {id}"))
}

/// Pause a running download.
#[tauri::command]
fn pause(state: State<'_, AppState>, id: u64) {
    state.manager.pause(id);
}

/// Start, resume, retry, or restart a download.
#[tauri::command]
fn resume(state: State<'_, AppState>, id: u64) {
    state.manager.resume(id);
}

/// Stop a download, keeping its partial file to resume later or deleting it
/// (also a partial kept by an earlier stop).
#[tauri::command]
fn stop(state: State<'_, AppState>, id: u64, keep_partial: bool) {
    state.manager.stop(id, keep_partial);
}

/// Move a waiting download to `index` among the waiting ones.
#[tauri::command]
fn move_queued(state: State<'_, AppState>, id: u64, index: usize) {
    state.manager.move_queued(id, index);
}

/// Remove the finished downloads in `statuses` (only those in `ids`, if
/// given). Returns how many were removed.
#[tauri::command]
fn clear(state: State<'_, AppState>, statuses: Vec<Status>, ids: Option<Vec<u64>>) -> usize {
    state.manager.clear(&statuses, ids.as_deref())
}

/// Check a completed download's file against `checksum`, given as
/// `"sha256:<hex>"` or a bare 64-digit hex SHA-256. The result arrives in the
/// snapshot's `checksum_state`.
#[tauri::command]
fn verify_checksum(state: State<'_, AppState>, id: u64, checksum: String) -> Result<(), String> {
    let checksum = parse_sha256(&checksum)?;
    state
        .manager
        .verify_checksum(id, checksum)
        .map_err(|e| e.to_string())
}

/// Parse `"<algo>:<hex>"`, or a bare 64-digit hex string as SHA-256.
fn parse_sha256(input: &str) -> Result<Checksum, String> {
    let input = input.trim();
    let bare = input.len() == 64 && input.bytes().all(|b| b.is_ascii_hexdigit());
    if bare {
        Checksum::from_str(&format!("sha256:{input}"))
    } else {
        Checksum::from_str(input)
    }
}

/// Remove a download from the list entirely.
#[tauri::command]
fn remove(state: State<'_, AppState>, id: u64) {
    state.manager.remove(id);
}

/// Pause every currently-running download.
#[tauri::command]
fn pause_all(state: State<'_, AppState>) {
    for d in state.manager.snapshot() {
        if matches!(d.status, Status::Downloading | Status::Queued) {
            state.manager.pause(d.id);
        }
    }
}

/// The default download directory, as a string (for the Add dialog's default).
#[tauri::command]
fn default_download_dir(app: tauri::AppHandle) -> Result<String, String> {
    Ok(default_dir(&app)?.to_string_lossy().into_owned())
}

/// Open a native folder picker; returns the chosen path, or `None` if cancelled.
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |path| {
        let _ = tx.send(path);
    });
    rx.await.ok().flatten().map(|p| p.to_string())
}

/// Open a file (or folder) with the OS default handler.
#[tauri::command]
fn open_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Reveal a file in its containing folder in the OS file manager.
#[tauri::command]
fn reveal_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| e.to_string())
}

/// Overrides the state/log directory (default `~/.rdm`), and puts the webview
/// profile (localStorage) under it too. Lets automated runs (the webview E2E
/// suite) stay off the developer's real download list and display settings.
const HOME_ENV: &str = "RDM_HOME";

/// Overrides the Add dialog's default download directory (default: the OS one).
const DOWNLOAD_DIR_ENV: &str = "RDM_DOWNLOAD_DIR";

/// A non-empty path from environment variable `key`, if set.
fn env_dir(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Resolve the default download directory: `RDM_DOWNLOAD_DIR`, else the OS
/// download directory, else home.
fn default_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Some(dir) = env_dir(DOWNLOAD_DIR_ENV) {
        return Ok(dir);
    }
    let paths = app.path();
    paths
        .download_dir()
        .or_else(|_| paths.home_dir())
        .map_err(|e| e.to_string())
}

/// Install the tracing subscriber: human-readable logs to stderr (visible when
/// run via `tauri dev`) and to `<dir>/rdm.<date>.log`, rotated daily (UTC) with
/// the last 7 files kept. The engine emits the events (download start, each
/// retry with its reason, failures), each tagged with its download's id; the
/// default level is `info`, overridable with `RUST_LOG`. Best-effort: logging
/// must never stop the app from starting.
fn init_logging(dir: &std::path::Path) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    install_logging(dir, filter);
}

/// [`init_logging`] with an explicit filter.
fn install_logging(dir: &std::path::Path, filter: tracing_subscriber::EnvFilter) {
    use tracing_appender::rolling::{Builder, Rotation};
    use tracing_subscriber::prelude::*;

    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("rdm")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir);
    let (file_writer, file_error) = match appender {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            // Keep the non-blocking writer's flush worker alive for the whole process.
            Box::leak(Box::new(guard));
            (Some(writer), None)
        }
        Err(e) => (None, Some(e)),
    };

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(file_writer.map(|writer| {
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer)
        }))
        .try_init();
    if let Some(e) = file_error {
        tracing::warn!(dir = %dir.display(), error = %e, "file logging disabled");
    }
}

/// Build the `Manager` over `~/.rdm/state.json` (or `$RDM_HOME/state.json`) and
/// spawn the snapshot bridge.
fn init_manager(app: &tauri::AppHandle) -> Result<Arc<Manager>, Box<dyn std::error::Error>> {
    let dir = match env_dir(HOME_ENV) {
        Some(dir) => dir,
        None => app.path().home_dir()?.join(".rdm"),
    };
    std::fs::create_dir_all(&dir)?;
    init_logging(&dir);
    let store = JsonStore::new(dir.join("state.json"));

    // The engine spawns each download on a Tokio runtime. Tauri's synchronous
    // command handlers run on the main thread with no ambient runtime, so give
    // the Manager a dedicated multi-thread runtime to spawn onto. It is leaked
    // so it lives for the whole process (a desktop app's lifetime) — which also
    // avoids dropping a runtime from within another runtime's context.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let handle = runtime.handle().clone();
    Box::leak(Box::new(runtime));

    let manager = Arc::new(Manager::load(Box::new(store), handle)?);

    // Bridge the manager's watch channel to a webview event. The receiver
    // always holds the latest coalesced snapshot, so a slow webview never
    // falls behind. An initial send primes the UI on startup.
    let app = app.clone();
    let mut rx = manager.subscribe();
    tauri::async_runtime::spawn(async move {
        let _ = app.emit("downloads-changed", rx.borrow().clone());
        while rx.changed().await.is_ok() {
            let snapshot = rx.borrow().clone();
            let _ = app.emit("downloads-changed", snapshot);
        }
    });

    Ok(manager)
}

/// Create the main window from its `tauri.conf.json` entry (marked
/// `create: false` so it's built here), with the webview profile under
/// `$RDM_HOME/webview` when that override is set (ignored on macOS).
fn create_main_window(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .ok_or("tauri.conf.json defines no \"main\" window")?;
    let mut builder = tauri::WebviewWindowBuilder::from_config(app, config)?;
    if let Some(home) = env_dir(HOME_ENV) {
        builder = builder.data_directory(home.join("webview"));
    }
    builder.build()?;
    Ok(())
}

/// Build and run the Tauri application.
///
/// # Panics
/// Panics if the engine `Manager` cannot be initialised (e.g. the home
/// directory is unavailable) — there is no usable app without it.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let manager = init_manager(app.handle())?;
            app.manage(AppState { manager });
            create_main_window(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list,
            add_download,
            get_settings,
            set_settings,
            set_limit,
            pause,
            resume,
            stop,
            move_queued,
            clear,
            verify_checksum,
            remove,
            pause_all,
            default_download_dir,
            pick_folder,
            open_path,
            reveal_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running rdm GUI");
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    #[test]
    fn checksums_parse_prefixed_or_bare() {
        let hex = "ab".repeat(32);
        let prefixed = super::parse_sha256(&format!("sha256:{hex}")).unwrap();
        let bare = super::parse_sha256(&format!("  {}  ", hex.to_uppercase())).unwrap();
        assert_eq!(prefixed, bare);
        assert_eq!(bare.to_string(), format!("sha256:{hex}"));
        assert!(super::parse_sha256("abc").is_err());
        assert!(super::parse_sha256("md5:abc").is_err());
    }

    /// The log goes to a dated `rdm.YYYY-MM-DD.log` (daily rotation), not one
    /// ever-growing `rdm.log`.
    #[test]
    fn logs_to_a_daily_file() {
        let dir = std::env::temp_dir().join(format!("rdm-gui-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // An explicit filter: an ambient `RUST_LOG=error` would hide the probe.
        super::install_logging(&dir, tracing_subscriber::EnvFilter::new("info"));
        tracing::warn!("daily rotation probe");

        let deadline = Instant::now() + Duration::from_secs(5);
        let logged = loop {
            let found = std::fs::read_dir(&dir).unwrap().flatten().find(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let date = name
                    .strip_prefix("rdm.")
                    .and_then(|n| n.strip_suffix(".log"));
                date.is_some_and(|d| d.len() == 10 && d.as_bytes()[4] == b'-')
                    && std::fs::read_to_string(e.path())
                        .is_ok_and(|s| s.contains("daily rotation probe"))
            });
            if found.is_some() || Instant::now() >= deadline {
                break found.is_some();
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(logged, "no dated log with the probe line in {names:?}");
    }
}
