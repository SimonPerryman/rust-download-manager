//! Tauri glue for the rdm desktop frontend.
//!
//! This layer is deliberately thin: it owns a single [`rdm::Manager`], exposes
//! each of its methods as a Tauri command (1–3 lines of pass-through each), and
//! re-broadcasts the manager's snapshot stream to the webview as a
//! `downloads-changed` event. All download logic lives in the engine — this
//! crate contains none.

mod commands_guard;
mod diagnostics;
mod logging;
mod updater;

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use rdm::{AddOptions, Checksum, DownloadSnapshot, JsonStore, LogLine, Manager, Settings, Status};
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

/// Add a download. `dir` is the user's chosen destination (folder picker or
/// typed in the Add dialog), checked by [`download_dir`]; blank defaults to
/// [`default_dir`]. `checksum`, if given, must parse as `"sha256:<hex>"`;
/// `limit` is a rate like `"500K"` (blank = unlimited); `tags` follow the
/// engine's tag rules. Returns the new download's id.
#[tauri::command]
fn add_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    url: String,
    dir: Option<String>,
    checksum: Option<String>,
    limit: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<u64, String> {
    let dir = match dir {
        Some(d) if !d.trim().is_empty() => download_dir(&d)?,
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
        tags: tags.unwrap_or_default(),
        ..AddOptions::default()
    };
    state
        .manager
        .add_with(url, dir, options)
        .map_err(|e| e.to_string())
}

/// Check a user-chosen destination: it must be an absolute path and, if it
/// exists, a directory. A missing one is fine; the engine creates it when the
/// download starts.
fn download_dir(dir: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(dir);
    if !path.is_absolute() {
        return Err(format!("download folder must be a full path: {dir}"));
    }
    if path.exists() && !path.is_dir() {
        return Err(format!("download folder is not a folder: {dir}"));
    }
    Ok(path)
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

/// Archive the listed finished downloads. Returns the ids archived, for Undo
/// to unarchive.
#[tauri::command]
fn archive(state: State<'_, AppState>, ids: Vec<u64>) -> Vec<u64> {
    state.manager.archive(&ids)
}

/// Return the listed archived downloads to the working views. Returns how
/// many changed.
#[tauri::command]
fn unarchive(state: State<'_, AppState>, ids: Vec<u64>) -> usize {
    state.manager.unarchive(&ids)
}

/// Archive the finished downloads in `statuses` (only those in `ids`, if
/// given). Returns the ids archived, for Undo to unarchive.
#[tauri::command]
fn archive_finished(
    state: State<'_, AppState>,
    statuses: Vec<Status>,
    ids: Option<Vec<u64>>,
) -> Vec<u64> {
    state.manager.archive_finished(&statuses, ids.as_deref())
}

/// Add `tags` to the listed downloads. Returns how many changed, or the
/// reason a tag was rejected.
#[tauri::command]
fn add_tags(state: State<'_, AppState>, ids: Vec<u64>, tags: Vec<String>) -> Result<usize, String> {
    state
        .manager
        .add_tags(&ids, &tags)
        .map_err(|e| e.to_string())
}

/// Remove `tags` from the listed downloads. Returns how many changed.
#[tauri::command]
fn remove_tags(state: State<'_, AppState>, ids: Vec<u64>, tags: Vec<String>) -> usize {
    state.manager.remove_tags(&ids, &tags)
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

/// Download `id`'s recent log lines, oldest first (empty for an unknown id).
#[tauri::command]
fn download_log(state: State<'_, AppState>, id: u64) -> Vec<LogLine> {
    state.manager.download_log(id)
}

/// Remove a download from the list entirely. A completed download's file and
/// a partial kept by Stop stay on disk; any other partial (e.g. a failed
/// download's `.part`) is deleted.
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

/// Resume the downloads an update restart paused (its notice's Resume all).
#[tauri::command]
fn resume_paused_for_update(state: State<'_, AppState>) {
    state.manager.resume_paused_for_update();
}

/// Dismiss the paused-for-the-update notice, leaving those downloads paused.
#[tauri::command]
fn forget_paused_for_update(state: State<'_, AppState>) {
    state.manager.forget_paused_for_update();
}

/// Resume every paused download (Resume all). Stopped ones stay stopped.
#[tauri::command]
fn resume_all(state: State<'_, AppState>) {
    for d in state.manager.snapshot() {
        if d.status == Status::Paused {
            state.manager.resume(d.id);
        }
    }
}

/// Retry every failed download (Retry all failed) that isn't archived.
#[tauri::command]
fn retry_failed(state: State<'_, AppState>) {
    for d in state.manager.snapshot() {
        if d.status == Status::Failed && !d.archived {
            state.manager.resume(d.id);
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
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    commands_guard::contained("pick_folder", async move {
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.dialog().file().pick_folder(move |path| {
            let _ = tx.send(path);
        });
        Ok(rx.await.ok().flatten().map(|p| p.to_string()))
    })
    .await
}

/// Open completed download `id`'s file with the OS default handler. Takes an
/// id, not a path, so the webview can only open files the engine downloaded.
#[tauri::command]
fn open_download(app: tauri::AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    let path = state
        .manager
        .completed_file(id)
        .map_err(|e| e.to_string())?;
    if let Some(log) = env_dir(OPEN_LOG_ENV) {
        return append_open_log(&log, "open", &path).map_err(|e| e.to_string());
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Show completed download `id`'s file in its folder in the OS file manager.
#[tauri::command]
fn reveal_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), String> {
    let path = state
        .manager
        .completed_file(id)
        .map_err(|e| e.to_string())?;
    if let Some(log) = env_dir(OPEN_LOG_ENV) {
        return append_open_log(&log, "reveal", &path).map_err(|e| e.to_string());
    }
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

/// When set, `open_download` / `reveal_download` / `open_update_notes` append `open <path>` /
/// `reveal <path>` / `url <url>` lines to this file instead of launching the OS handler. Lets
/// the webview E2E suite check what would open without starting another program.
const OPEN_LOG_ENV: &str = "RDM_OPEN_LOG";

/// One `RDM_OPEN_LOG` line (without its newline): `kind` is `open`, `reveal` or `url`.
fn open_log_line(kind: &str, path: &std::path::Path) -> String {
    format!("{kind} {}", path.display())
}

/// Append one [`open_log_line`] to `log`, creating the file if needed.
fn append_open_log(
    log: &std::path::Path,
    kind: &str,
    path: &std::path::Path,
) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    writeln!(file, "{}", open_log_line(kind, path))
}

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

/// Create `dir` (and its parents) if missing. On Unix it is made owner-only
/// (0700): its state file holds full download URLs, which may carry
/// credentials or signed query strings.
fn create_private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        // A dir made by an older rdm keeps its old mode otherwise.
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(dir)
}

/// Where rdm keeps its state and logs: `$RDM_HOME`, else `~/.rdm`.
fn rdm_home(app: &tauri::AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(match env_dir(HOME_ENV) {
        Some(dir) => dir,
        None => app.path().home_dir()?.join(".rdm"),
    })
}

/// Build the `Manager` over `dir/state.json` and spawn the snapshot bridge.
fn init_manager(
    app: &tauri::AppHandle,
    dir: &std::path::Path,
) -> Result<Arc<Manager>, Box<dyn std::error::Error>> {
    let dir = dir.to_path_buf();
    create_private_dir(&dir)?;
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
    // After the Manager, whose layer it installs; loading logs nothing. The
    // default level is `info`, overridable with `RUST_LOG`.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let level = filter.to_string();
    logging::install_logging(&dir, filter, manager.log_layer());
    logging::log_startup(&startup_lines(&manager, &level));

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

/// What a log needs to tie it to a build and a setup: the version, OS,
/// WebView, log level and memory use, then the settings that change
/// behaviour and how many downloads are in each state. No URLs, paths or tags.
fn startup_lines(manager: &Manager, level: &str) -> Vec<String> {
    let webview = tauri::webview_version().unwrap_or_else(|_| "unknown".to_owned());
    let memory = rdm::platform::resident_memory()
        .map(|bytes| format!("{} MiB", bytes / (1024 * 1024)))
        .unwrap_or_else(|| "unknown".to_owned());
    let settings = manager.settings();
    let limit = settings
        .global_limit
        .map(|rate| format!("{rate} B/s"))
        .unwrap_or_else(|| "none".to_owned());
    vec![
        format!(
            "rdm {} · {} · WebView {webview} · log level {level} · memory {memory}",
            logging::version(),
            rdm::platform::os_description(),
        ),
        format!(
            "Settings: {} connections · {} at a time · speed limit {limit} · data dir {} · downloads: {}",
            settings.max_connections,
            settings.max_concurrent,
            if env_dir(HOME_ENV).is_some() { "overridden" } else { "default" },
            status_counts(&manager.snapshot()),
        ),
    ]
}

/// How many downloads are in each status, e.g. `2 completed, 1 failed`.
fn status_counts(downloads: &[DownloadSnapshot]) -> String {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for d in downloads {
        let status = format!("{:?}", d.status).to_lowercase();
        match counts.iter_mut().find(|(s, _)| *s == status) {
            Some((_, n)) => *n += 1,
            None => counts.push((status, 1)),
        }
    }
    if counts.is_empty() {
        return "none".to_owned();
    }
    let parts: Vec<String> = counts.iter().map(|(s, n)| format!("{n} {s}")).collect();
    parts.join(", ")
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

/// What to tell the user when rdm can't start: for a download list saved by
/// a newer rdm, its version and that it was left alone.
fn startup_error_message(err: &(dyn std::error::Error + 'static)) -> String {
    match err.downcast_ref::<rdm::Error>() {
        Some(rdm::Error::UnsupportedStore(version)) => format!(
            "Your download list was saved by a newer rdm (list version {version}), \
             which this version can't open.\n\n\
             Install the newer rdm to open it. The list has been left untouched."
        ),
        _ => format!("rdm couldn't start.\n\n{err}"),
    }
}

/// Show why rdm can't start in a native error dialog (a release build has no
/// console), then exit with status 1 once it's dismissed.
fn report_startup_error(app: &tauri::AppHandle, err: &(dyn std::error::Error + 'static)) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    let message = startup_error_message(err);
    eprintln!("rdm: {message}");
    let handle = app.clone();
    app.dialog()
        .message(message)
        .title("rdm can't start")
        .kind(MessageDialogKind::Error)
        .show(move |_| handle.exit(1));
}

/// Look for a newer rdm; `None` when this is the newest. The update found is
/// kept for [`install_update`].
#[tauri::command]
async fn check_for_update(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    updater: State<'_, updater::UpdaterState>,
) -> Result<Option<updater::UpdateInfo>, String> {
    commands_guard::contained("check_for_update", async move {
        updater::check_command(&app, &updater, state.manager.clone()).await
    })
    .await
}

/// Download the update [`check_for_update`] found (progress as
/// `update-progress` events), then restart into it now or install it at quit.
/// `version` is the one the notice shows; any other update is refused.
#[tauri::command]
async fn install_update(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    updater: State<'_, updater::UpdaterState>,
    when: updater::InstallWhen,
    version: String,
) -> Result<(), String> {
    commands_guard::contained("install_update", async move {
        updater::install_command(&app, &updater, &state.manager, when, &version).await
    })
    .await
}

/// Open the release page of the update the last check found. It takes no
/// URL, so the webview can open only that page.
#[tauri::command]
fn open_update_notes(
    app: tauri::AppHandle,
    updater: State<'_, updater::UpdaterState>,
) -> Result<(), String> {
    let url = updater
        .notes_url()
        .ok_or("No update found. Check for updates first.")?;
    if let Some(log) = env_dir(OPEN_LOG_ENV) {
        return append_open_log(&log, "url", url.as_ref()).map_err(|e| e.to_string());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Whether to check for updates automatically (`null` until the first-run
/// question is answered), this app's version, and the version it was updated
/// from when this is the first run since.
#[tauri::command]
fn update_prefs(prefs: State<'_, updater::prefs::PrefsStore>) -> updater::prefs::UpdatePrefs {
    prefs.prefs()
}

/// Turn automatic update checks on or off.
#[tauri::command]
fn set_auto_check(prefs: State<'_, updater::prefs::PrefsStore>, on: bool) -> Result<(), String> {
    prefs.set_auto_check(on)
}

/// The version waiting to install at quit, if any.
#[tauri::command]
fn staged_update(updater: State<'_, updater::UpdaterState>) -> Option<String> {
    updater.staged_version()
}

/// Build and run the Tauri application.
///
/// If the engine `Manager` can't be initialised (e.g. the download list was
/// saved by a newer rdm), it says why in a dialog and exits with status 1,
/// leaving the list untouched.
pub fn run() {
    logging::install_panic_hook();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updater::UpdaterState::new(updater::Seam::current()))
        .setup(|app| {
            let started = rdm_home(app.handle())
                .and_then(|home| Ok((init_manager(app.handle(), &home)?, home)));
            let (manager, home) = match started {
                Ok(started) => started,
                Err(err) => {
                    // No window: the dialog's callback ends the app.
                    report_startup_error(app.handle(), &*err);
                    return Ok(());
                }
            };
            app.manage(AppState { manager });
            let version = app.package_info().version.to_string();
            app.manage(updater::prefs::PrefsStore::open(&home, &version));
            create_main_window(app.handle())?;
            Ok(())
        })
        .invoke_handler(commands_guard::contain_panics(tauri::generate_handler![
            list,
            add_download,
            get_settings,
            set_settings,
            set_limit,
            pause,
            resume,
            stop,
            move_queued,
            archive,
            unarchive,
            archive_finished,
            add_tags,
            remove_tags,
            verify_checksum,
            download_log,
            remove,
            pause_all,
            resume_all,
            resume_paused_for_update,
            forget_paused_for_update,
            retry_failed,
            default_download_dir,
            pick_folder,
            open_download,
            reveal_download,
            check_for_update,
            install_update,
            staged_update,
            open_update_notes,
            update_prefs,
            set_auto_check,
            diagnostics::log_frontend_error,
            diagnostics::logs_folder,
            diagnostics::open_logs_folder,
            diagnostics::open_bug_report,
            diagnostics::diagnostics,
            diagnostics::save_text_file,
            #[cfg(feature = "fault-injection")]
            commands_guard::test_panic,
            #[cfg(feature = "fault-injection")]
            commands_guard::test_panic_async,
        ]))
        .build(tauri::generate_context!())
        .expect("error while building rdm GUI")
        .run(|app, event| {
            // Record running downloads' time before the process goes, then
            // install an update waiting for the quit, then get every log line
            // to disk.
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.manager.shutdown();
                }
                if let Some(updater) = app.try_state::<updater::UpdaterState>() {
                    updater::install_at_exit(&updater);
                }
                logging::flush_logs();
            }
        });
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    /// A list saved by a newer rdm names its version and says it's untouched;
    /// any other startup error shows its own text.
    #[test]
    fn startup_errors_say_why() {
        let newer: Box<dyn std::error::Error> = Box::new(rdm::Error::UnsupportedStore(3));
        let message = super::startup_error_message(&*newer);
        assert!(message.contains("newer rdm (list version 3)"), "{message}");
        assert!(message.contains("left untouched"), "{message}");

        let other: Box<dyn std::error::Error> = "no home directory".into();
        assert_eq!(
            super::startup_error_message(&*other),
            "rdm couldn't start.\n\nno home directory"
        );
    }

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

    /// A destination must be absolute and not an existing file; a missing
    /// folder is accepted (the engine creates it).
    #[test]
    fn download_dir_is_absolute_and_not_a_file() {
        let root = std::env::temp_dir().join(format!("rdm-gui-dir-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("a-file");
        std::fs::write(&file, b"x").unwrap();
        let s = |p: &std::path::Path| p.to_string_lossy().into_owned();

        assert_eq!(super::download_dir(&s(&root)).unwrap(), root);
        let missing = root.join("new").join("sub");
        assert_eq!(super::download_dir(&s(&missing)).unwrap(), missing);
        assert!(super::download_dir(&s(&file))
            .unwrap_err()
            .contains("not a folder"));
        assert!(super::download_dir("relative/dir")
            .unwrap_err()
            .contains("full path"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The open log records one `<kind> <path>` line per call, appending.
    #[test]
    fn open_log_appends_one_line_per_call() {
        assert_eq!(
            super::open_log_line("open", r"C:\dl\a b.iso".as_ref()),
            r"open C:\dl\a b.iso"
        );
        let log = std::env::temp_dir().join(format!("rdm-gui-open-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&log);
        super::append_open_log(&log, "open", "/dl/a.iso".as_ref()).unwrap();
        super::append_open_log(&log, "reveal", "/dl/b.iso".as_ref()).unwrap();
        let text = std::fs::read_to_string(&log).unwrap();
        let _ = std::fs::remove_file(&log);
        assert_eq!(text, "open /dl/a.iso\nreveal /dl/b.iso\n");
    }

    /// The state/log dir is created owner-only, and an existing one tightened.
    #[cfg(unix)]
    #[test]
    fn state_dir_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("rdm-gui-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join(".rdm");
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;

        super::create_private_dir(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        super::create_private_dir(&dir).unwrap();
        let tightened = mode(&dir);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(tightened, 0o700);
    }

    /// `RUST_LOG` (the outputs' filter) doesn't reach the Log tab: with the
    /// outputs at `error`, a download's lower-level lines (here its retry
    /// warning) still land in its log.
    #[test]
    fn log_tab_keeps_its_lines_whatever_the_filter() {
        let dir = std::env::temp_dir().join(format!("rdm-gui-logtab-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let store = rdm::JsonStore::new(dir.join("state.json"));
        let mgr = rdm::Manager::load(Box::new(store), runtime.handle().clone()).unwrap();
        let subscriber = super::logging::logging_subscriber(
            tracing_subscriber::EnvFilter::new("error"),
            mgr.log_layer(),
            std::io::sink,
            None::<fn() -> std::io::Sink>,
        );

        // A link nothing serves: its first retry is logged at once; no need
        // to wait for it to fail.
        let lines = tracing::subscriber::with_default(subscriber, || {
            runtime.block_on(async {
                let id = mgr.add("http://127.0.0.1:9/f.bin", &dir, None, None);
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    let lines = mgr.download_log(id);
                    let retried = lines.iter().any(|l| l.message.contains("— retrying in"));
                    if retried || Instant::now() >= deadline {
                        mgr.remove(id);
                        break lines;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
        });
        drop(mgr);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            lines.iter().any(|l| l.message.contains("— retrying in")),
            "{lines:#?}"
        );
    }

    /// The log goes to a dated `rdm.YYYY-MM-DD.log` (daily rotation), not one
    /// ever-growing `rdm.log`.
    #[test]
    fn logs_to_a_daily_file() {
        let dir = std::env::temp_dir().join(format!("rdm-gui-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // An explicit filter: an ambient `RUST_LOG=error` would hide the probe.
        super::logging::install_logging(
            &dir,
            tracing_subscriber::EnvFilter::new("info"),
            tracing_subscriber::layer::Identity::new(),
        );
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
                break found.map(|e| e.path());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        let today = super::logging::today_log_path(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        let logged = logged.unwrap_or_else(|| panic!("no dated log with the probe in {names:?}"));
        // The startup lines and panic records are appended to the file by
        // this name, so it must be the appender's.
        assert_eq!(logged, today);
    }

    /// The startup lines name the build and the settings, and nothing that
    /// could identify what was downloaded: no URL, path or tag.
    #[test]
    fn startup_lines_leave_out_urls_paths_and_tags() {
        let dir = std::env::temp_dir().join(format!("rdm-gui-startup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let store = rdm::JsonStore::new(dir.join("state.json"));
        let mgr = rdm::Manager::load(Box::new(store), runtime.handle().clone()).unwrap();
        assert!(super::startup_lines(&mgr, "info")[1].ends_with("downloads: none"));

        let options = rdm::AddOptions {
            tags: vec!["secret-tag".to_owned()],
            ..rdm::AddOptions::default()
        };
        let id = runtime.block_on(async {
            mgr.add_with("http://127.0.0.1:9/private.iso?token=SECRET", &dir, options)
                .unwrap()
        });
        let lines = super::startup_lines(&mgr, "info").join("\n");
        mgr.remove(id);
        drop(mgr);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            lines.contains(&format!("rdm {}", env!("CARGO_PKG_VERSION"))),
            "{lines}"
        );
        assert!(lines.contains("log level info"), "{lines}");
        // Queued or already downloading: one download, counted by status.
        assert!(lines.contains("downloads: 1 "), "{lines}");
        for private in [
            "127.0.0.1",
            "private.iso",
            "SECRET",
            "secret-tag",
            "rdm-gui-startup",
        ] {
            assert!(!lines.contains(private), "{private} in {lines}");
        }
    }
}
