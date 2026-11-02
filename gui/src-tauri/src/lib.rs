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

use rdm::{AddOptions, Checksum, DownloadSnapshot, JsonStore, Manager, Settings};
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

/// Cancel a running or paused download, discarding its partial data.
#[tauri::command]
fn cancel(state: State<'_, AppState>, id: u64) {
    state.manager.cancel(id);
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
        if matches!(d.status, rdm::Status::Downloading | rdm::Status::Queued) {
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
/// run via `tauri dev`) and appended to `<dir>/rdm.log`. The engine emits the
/// events (download start, each retry with its reason, failures); the default
/// level is `info`, overridable with `RUST_LOG`. Best-effort: logging must
/// never stop the app from starting.
fn init_logging(dir: &std::path::Path) {
    use tracing_subscriber::prelude::*;

    let file_appender = tracing_appender::rolling::never(dir, "rdm.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    // Keep the non-blocking writer's flush worker alive for the whole process.
    Box::leak(Box::new(guard));

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(file_writer),
        )
        .try_init();
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
            pause,
            resume,
            cancel,
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
