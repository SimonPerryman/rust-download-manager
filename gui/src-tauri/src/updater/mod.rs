//! The in-app updater: finds a newer rdm on GitHub Releases, downloads and
//! verifies it, then installs it now (restarting) or when the app quits.
//!
//! Everything runs here in Rust through rdm's own commands; the webview gets
//! no `updater:*` permission. Stable builds read `releases/latest`, which
//! never points at a pre-release, and also refuse a pre-release version
//! outright; a pre-release build follows the newest release, rcs included.

pub mod prefs;
#[cfg(debug_assertions)]
mod seam;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rdm::Manager;
use semver::Version;
use serde::Serialize;
use tauri::utils::config::BundleType;
use tauri::{AppHandle, Emitter, Runtime, Url};
use tauri_plugin_updater::{Update, UpdaterExt};

const REPO: &str = "SimonPerryman/rust-download-manager";
/// Where stable builds look: GitHub's latest non-pre-release release.
const STABLE_MANIFEST: &str =
    "https://github.com/SimonPerryman/rust-download-manager/releases/latest/download/latest.json";
/// Where pre-release builds look for the newest release, rcs included.
const RELEASES_API: &str =
    "https://api.github.com/repos/SimonPerryman/rust-download-manager/releases?per_page=30";
const MANIFEST_ASSET: &str = "latest.json";
const TIMEOUT: Duration = Duration::from_secs(30);
/// The webview event carrying an update's download progress.
pub const PROGRESS_EVENT: &str = "update-progress";

/// Test-only overrides (debug builds read them from the environment; a release
/// build has none): another manifest URL and public key, the installer kind,
/// and stopping where the installer would run or making it fail. With either
/// of the last two, a restart is only recorded.
#[derive(Debug, Default, Clone)]
pub struct Seam {
    pub endpoint: Option<Url>,
    pub pubkey: Option<String>,
    pub bundle: Option<BundleType>,
    pub dry_install: bool,
    pub fail_install: bool,
}

impl Seam {
    /// The overrides in effect for this process.
    pub fn current() -> Self {
        #[cfg(debug_assertions)]
        return seam::from_env();
        #[cfg(not(debug_assertions))]
        Self::default()
    }
}

/// A newer version, as the frontend shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// The release page, for its notes (and the download on `.deb` installs).
    pub notes_url: String,
    /// False where rdm can't replace itself (a `.deb` install, or a build
    /// that wasn't installed from a bundle): the frontend offers the release
    /// page instead.
    pub can_install: bool,
}

/// Download progress, in bytes.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// When to install a downloaded update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallWhen {
    /// Pause downloads, install and restart into the new version.
    Now,
    /// Install at the next normal quit, interrupting nothing.
    OnQuit,
}

/// The updater's state across commands: the update the last check found and
/// one downloaded to install at quit.
#[derive(Default)]
pub struct UpdaterState {
    /// The update and whether this install can apply it in place.
    found: Mutex<Option<(Update, bool)>>,
    staged: Mutex<Option<(Update, Vec<u8>)>>,
    hook: Arc<RestartHook>,
    seam: Seam,
    restart_requested: AtomicBool,
}

/// What Windows' before-exit hook (run by the plugin just before it starts
/// the installer) does: ahead of Restart now, pause downloads and mark them as
/// paused for the update. An install at quit runs the same hook after the manager has shut
/// down, so then it does nothing.
#[derive(Default)]
pub struct RestartHook {
    restarting: AtomicBool,
    paused: AtomicBool,
}

impl RestartHook {
    pub fn run(&self, manager: &Manager) {
        if self.restarting.load(Ordering::SeqCst) {
            manager.shutdown_for_restart();
            self.paused.store(true, Ordering::SeqCst);
        }
    }
}

impl UpdaterState {
    pub fn new(seam: Seam) -> Self {
        Self {
            seam,
            ..Self::default()
        }
    }

    /// The version staged to install at quit, if any.
    pub fn staged_version(&self) -> Option<String> {
        lock(&self.staged).as_ref().map(|(u, _)| u.version.clone())
    }

    /// The release page of the update the last check found (or the one
    /// staged), so the webview never names the URL it opens.
    pub fn notes_url(&self) -> Option<String> {
        let found = lock(&self.found).as_ref().map(|(u, _)| u.version.clone());
        found
            .or_else(|| self.staged_version())
            .map(|v| release_page(&v))
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether `remote` is offered to a build at `current`: it must be newer, and
/// a stable build never gets a pre-release.
pub fn offered(current: &Version, remote: &Version) -> bool {
    remote > current && (remote.pre.is_empty() || !current.pre.is_empty())
}

/// The manifest key this install updates from (`{os}-{arch}-{installer}`) and
/// whether it can install it in place. A `.deb` (or an unbundled build) still
/// reads a manifest entry to learn the version, but can't install it.
pub fn manifest_target(bundle: Option<BundleType>) -> (String, bool) {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };
    let arch = std::env::consts::ARCH;
    let (kind, can_install) = match bundle {
        Some(BundleType::Nsis) => ("nsis", true),
        Some(BundleType::Msi) => ("msi", true),
        Some(BundleType::App | BundleType::Dmg) => ("app", true),
        Some(BundleType::AppImage) => ("appimage", true),
        Some(BundleType::Deb | BundleType::Rpm) | None => match os {
            "windows" => ("nsis", false),
            "darwin" => ("app", false),
            _ => ("appimage", false),
        },
    };
    (format!("{os}-{arch}-{kind}"), can_install)
}

/// The newest release in a GitHub releases listing that carries a manifest:
/// its version and the manifest's URL. Drafts and unparsable tags are skipped.
pub fn newest_manifest(releases: &serde_json::Value) -> Option<(Version, Url)> {
    releases
        .as_array()?
        .iter()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false))
        .filter_map(|r| {
            let tag = r["tag_name"].as_str()?;
            let version = Version::parse(tag.trim_start_matches('v')).ok()?;
            let url = r["assets"]
                .as_array()?
                .iter()
                .find(|a| a["name"] == MANIFEST_ASSET)?["browser_download_url"]
                .as_str()?
                .parse()
                .ok()?;
            Some((version, url))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
}

/// The release page for `version`.
fn release_page(version: &str) -> String {
    format!("https://github.com/{REPO}/releases/tag/v{version}")
}

/// Which manifest a build at `current` reads: the stable one, or for a
/// pre-release build the newest release's (none yet: `None`).
async fn manifest_url(current: &Version, seam: &Seam) -> Result<Option<Url>, String> {
    if let Some(url) = &seam.endpoint {
        return Ok(Some(url.clone()));
    }
    if current.pre.is_empty() {
        return Ok(Some(STABLE_MANIFEST.parse().expect("valid URL")));
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .user_agent(concat!("rdm/", env!("CARGO_PKG_VERSION")))
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    let body = client
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    let releases: serde_json::Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    Ok(newest_manifest(&releases).map(|(_, url)| url))
}

/// Ask the release feed whether a newer version is out for a build at
/// `current`. `before_exit` runs on Windows just before the installer takes
/// over the process.
pub async fn check<R: Runtime>(
    app: &AppHandle<R>,
    current: Version,
    seam: &Seam,
    before_exit: impl Fn() + Send + Sync + 'static,
) -> Result<Option<(UpdateInfo, Update)>, String> {
    let Some(url) = manifest_url(&current, seam).await? else {
        return Ok(None);
    };
    let (target, can_install) = manifest_target(
        seam.bundle
            .clone()
            .or_else(tauri::utils::platform::bundle_type),
    );
    let current_for_check = current.clone();
    let mut builder = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(|e| e.to_string())?
        .target(target)
        .timeout(TIMEOUT)
        .version_comparator(move |_, release| offered(&current_for_check, &release.version))
        .on_before_exit(before_exit);
    if let Some(pubkey) = &seam.pubkey {
        builder = builder.pubkey(pubkey.clone());
    }
    let update = match builder.build().map_err(|e| e.to_string())?.check().await {
        Ok(update) => update,
        // A release without this installer kind's entry: nothing to offer.
        Err(
            e @ (tauri_plugin_updater::Error::TargetNotFound(_)
            | tauri_plugin_updater::Error::TargetsNotFound(_)),
        ) => {
            tracing::warn!("the newest release has no update for this install: {e}");
            None
        }
        Err(e) => return Err(e.to_string()),
    };
    Ok(update.map(|update| {
        let info = UpdateInfo {
            version: update.version.clone(),
            current_version: current.to_string(),
            notes_url: release_page(&update.version),
            can_install,
        };
        (info, update)
    }))
}

/// Download and verify `update`, reporting progress at most every 100 ms and
/// once more at the end.
pub async fn download(
    update: &Update,
    mut on_progress: impl FnMut(Progress),
) -> Result<Vec<u8>, String> {
    let mut downloaded = 0u64;
    let mut total = None;
    let mut last = Instant::now();
    let result = update
        .download(
            |chunk, length| {
                downloaded += chunk as u64;
                total = length;
                if last.elapsed() >= Duration::from_millis(100) {
                    last = Instant::now();
                    on_progress(Progress { downloaded, total });
                }
            },
            || {},
        )
        .await;
    if result.is_ok() {
        on_progress(Progress { downloaded, total });
    }
    result.map_err(|e| match e {
        tauri_plugin_updater::Error::Minisign(_)
        | tauri_plugin_updater::Error::SignedVersionMismatch { .. }
        | tauri_plugin_updater::Error::MissingSignedVersion => {
            format!("The update's signature didn't verify, so it wasn't installed ({e}).")
        }
        e => format!("Couldn't download the update: {e}"),
    })
}

/// The check command's work: look for an update and remember it for install.
pub async fn check_command<R: Runtime>(
    app: &AppHandle<R>,
    state: &UpdaterState,
    manager: Arc<Manager>,
) -> Result<Option<UpdateInfo>, String> {
    let current = app.package_info().version.clone();
    let hook = state.hook.clone();
    let found = check(app, current, &state.seam, move || hook.run(&manager))
        .await
        .inspect_err(|e| tracing::warn!("update check failed: {e}"))?;
    match &found {
        Some((info, _)) => tracing::info!(version = %info.version, "update available"),
        None => tracing::info!("no update available"),
    }
    *lock(&state.found) = found
        .as_ref()
        .map(|(info, update)| (update.clone(), info.can_install));
    Ok(found.map(|(info, _)| info))
}

/// The install command's work: download `version`, the update the notice
/// shows, then restart into it now, or keep it to install at quit. When that
/// version is staged for the quit, its bytes are reused; a version that's
/// neither found nor staged is refused.
///
/// Restart now pauses downloads (as a quit does) and marks them as paused for
/// the update: on Windows just before the installer takes over, elsewhere
/// right after the new version is in place, so a failed install leaves them
/// running. A failed install keeps the update (and any staged for the quit),
/// so it can be tried again, unless Windows' hook had already paused the
/// downloads: then rdm restarts into this version instead.
pub async fn install_command<R: Runtime>(
    app: &AppHandle<R>,
    state: &UpdaterState,
    manager: &Manager,
    when: InstallWhen,
    version: &str,
) -> Result<(), String> {
    let found = lock(&state.found)
        .clone()
        .filter(|(u, _)| u.version == version);
    let staged = lock(&state.staged)
        .clone()
        .filter(|(u, _)| u.version == version);
    // Install the version the notice shows. The update staged for the quit is
    // already downloaded and verified: reuse it when it's that version.
    let (update, can_install, staged_bytes) = match (found, staged) {
        (found, Some((staged, bytes))) => (
            staged,
            found.is_none_or(|(_, can_install)| can_install),
            Some(bytes),
        ),
        (Some((found, can_install)), None) => (found, can_install, None),
        (None, None) => {
            return Err(format!(
                "rdm {version} isn't the update rdm found. Check for updates again."
            ))
        }
    };
    if !can_install {
        return Err(
            "This copy of rdm can't update itself; download the new version from its release page."
                .into(),
        );
    }
    let bytes = match staged_bytes {
        Some(bytes) => bytes,
        None => download(&update, |p| {
            let _ = app.emit(PROGRESS_EVENT, p);
        })
        .await
        .inspect_err(|e| tracing::warn!("update download failed: {e}"))?,
    };
    match when {
        InstallWhen::OnQuit => {
            tracing::info!(version = %update.version, "update will install at quit");
            forget_found(state, &update);
            *lock(&state.staged) = Some((update, bytes));
            Ok(())
        }
        InstallWhen::Now => {
            tracing::info!(version = %update.version, "installing update and restarting");
            state.hook.restarting.store(true, Ordering::SeqCst);
            if state.seam.dry_install {
                state.hook.run(manager);
                forget_found(state, &update);
                lock(&state.staged).take();
                tracing::info!("update install stopped before the installer (test seam)");
                return Ok(());
            }
            if let Err(e) = run_installer(state, manager, &update, &bytes) {
                state.hook.restarting.store(false, Ordering::SeqCst);
                tracing::error!("couldn't install the update: {e}");
                if state.hook.paused.load(Ordering::SeqCst) {
                    // The hook already stopped the downloads and closed the
                    // manager: restart into this version, where they can be
                    // resumed. An update staged for the quit isn't installed
                    // on the way out.
                    lock(&state.staged).take();
                    restart(app, state);
                    return Err(format!(
                        "Couldn't start the update's installer ({e}). rdm is restarting; resume your downloads from there."
                    ));
                }
                return Err(format!("Couldn't install the update: {e}"));
            }
            // Windows doesn't get here: the installer has taken over. An
            // older update staged for the quit mustn't follow this one.
            forget_found(state, &update);
            lock(&state.staged).take();
            manager.shutdown_for_restart();
            restart(app, state);
            Ok(())
        }
    }
}

/// Forget the checked update once it's installed or staged, unless a check
/// that finished meanwhile found a newer one.
fn forget_found(state: &UpdaterState, update: &Update) {
    let mut found = lock(&state.found);
    if found
        .as_ref()
        .is_some_and(|(u, _)| u.version == update.version)
    {
        found.take();
    }
}

/// Run the installer; on Windows it doesn't return when it starts.
fn run_installer(
    state: &UpdaterState,
    manager: &Manager,
    update: &Update,
    bytes: &[u8],
) -> Result<(), String> {
    if state.seam.fail_install {
        if cfg!(windows) {
            state.hook.run(manager);
        }
        return Err("installer failed (test seam)".into());
    }
    update.install(bytes).map_err(|e| e.to_string())
}

/// Restart the app, or with a test seam only record that it would.
fn restart<R: Runtime>(app: &AppHandle<R>, state: &UpdaterState) {
    state.restart_requested.store(true, Ordering::SeqCst);
    if !(state.seam.dry_install || state.seam.fail_install) {
        app.request_restart();
    }
}

/// Install an update staged for quit; called from the exit handler after the
/// manager has shut down. Errors are logged: the app is going away anyway.
pub fn install_at_exit(state: &UpdaterState) {
    let Some((update, bytes)) = lock(&state.staged).take() else {
        return;
    };
    if state.seam.dry_install {
        tracing::info!(version = %update.version, "update install at quit stopped before the installer (test seam)");
        return;
    }
    tracing::info!(version = %update.version, "installing update at quit");
    if let Err(e) = update.restart_after_install(false).install(&bytes) {
        tracing::error!("couldn't install the update at quit: {e}");
    }
}

#[cfg(test)]
mod tests;
