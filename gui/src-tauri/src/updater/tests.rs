//! The updater against a local fixture release feed, signed with a throwaway
//! key: never GitHub.

use std::io::{BufRead, BufReader, Cursor, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use rdm::{JsonStore, Manager, Status};
use semver::Version;
use serde_json::{json, Value};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::utils::config::BundleType;

use super::*;

const PAYLOAD: &[u8] = b"the new rdm";

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

// ---- Pure rules ----

#[test]
fn only_newer_versions_are_offered() {
    assert!(offered(&v("0.3.0"), &v("0.3.1")));
    assert!(!offered(&v("0.3.0"), &v("0.3.0")));
    assert!(!offered(&v("0.3.0"), &v("0.2.9")));
}

#[test]
fn stable_builds_never_get_a_pre_release() {
    assert!(!offered(&v("0.3.0"), &v("0.4.0-rc.1")));
    // An rc build moves on to a newer rc and to the final release.
    assert!(offered(&v("0.3.0-rc.1"), &v("0.3.0-rc.2")));
    assert!(offered(&v("0.3.0-rc.2"), &v("0.3.0")));
    assert!(!offered(&v("0.3.0-rc.2"), &v("0.3.0-rc.1")));
}

#[test]
fn each_installer_kind_reads_its_own_entry() {
    let key = |b| manifest_target(Some(b));
    let arch = std::env::consts::ARCH;
    if cfg!(windows) {
        assert_eq!(
            key(BundleType::Nsis),
            (format!("windows-{arch}-nsis"), true)
        );
        assert_eq!(key(BundleType::Msi), (format!("windows-{arch}-msi"), true));
        assert_eq!(
            manifest_target(None),
            (format!("windows-{arch}-nsis"), false)
        );
    }
    if cfg!(target_os = "linux") {
        assert_eq!(
            key(BundleType::AppImage),
            (format!("linux-{arch}-appimage"), true)
        );
        // A .deb learns the version from the AppImage entry but can't install it.
        assert_eq!(
            key(BundleType::Deb),
            (format!("linux-{arch}-appimage"), false)
        );
    }
    if cfg!(target_os = "macos") {
        assert_eq!(key(BundleType::App), (format!("darwin-{arch}-app"), true));
    }
    assert!(!key(BundleType::Deb).1);
    assert!(!key(BundleType::Rpm).1);
}

#[test]
fn pre_release_builds_follow_the_newest_release_with_a_manifest() {
    let release = |tag: &str, draft: bool, manifest: bool| {
        let assets = if manifest {
            json!([{ "name": "latest.json", "browser_download_url": format!("https://example.invalid/{tag}/latest.json") }])
        } else {
            json!([{ "name": "rdm.zip", "browser_download_url": "https://example.invalid/x" }])
        };
        json!({ "tag_name": tag, "draft": draft, "prerelease": tag.contains('-'), "assets": assets })
    };
    let releases = json!([
        release("v0.4.0", true, true),
        release("v0.3.1-rc.1", false, false),
        release("v0.3.0-rc.2", false, true),
        release("v0.2.0", false, false),
        release("v0.3.0-rc.1", false, true),
        release("nightly", false, true),
    ]);
    let (version, url) = newest_manifest(&releases).unwrap();
    assert_eq!(version, v("0.3.0-rc.2"));
    assert_eq!(
        url.as_str(),
        "https://example.invalid/v0.3.0-rc.2/latest.json"
    );
    assert_eq!(newest_manifest(&json!([])), None);
}

// ---- Against a fixture feed ----

/// A throwaway update key: the public half as the app's config holds it, and
/// signatures as `tauri signer sign` writes them (base64 of a minisign
/// signature whose trusted comment binds the version).
struct Key(minisign::KeyPair);

impl Key {
    fn new() -> Self {
        Self(minisign::KeyPair::generate_unencrypted_keypair().unwrap())
    }

    fn public(&self) -> String {
        b64(&self.0.pk.to_box().unwrap().to_string())
    }

    fn sign(&self, data: &[u8], version: &str) -> String {
        let comment = format!("timestamp:0\tfile:rdm-update\tversion:{version}");
        let sig = minisign::sign(
            Some(&self.0.pk),
            &self.0.sk,
            Cursor::new(data),
            Some(&comment),
            None,
        )
        .unwrap();
        b64(&sig.to_string())
    }
}

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s)
}

/// A tiny HTTP server: `/latest.json` is the manifest, `/update` the payload,
/// `/slow` a large file that trickles (a download that stays running).
struct Feed {
    base: String,
}

impl Feed {
    fn serve(manifest: Value) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let manifest = Arc::new(manifest.to_string());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let manifest = manifest.clone();
                std::thread::spawn(move || respond(stream, &manifest));
            }
        });
        Self { base }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

fn respond(mut stream: TcpStream, manifest: &str) {
    let mut line = String::new();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    if reader.read_line(&mut line).is_err() {
        return;
    }
    // Drain the headers.
    let mut header = String::new();
    while reader.read_line(&mut header).is_ok_and(|n| n > 2) {
        header.clear();
    }
    let target = line.split_whitespace().nth(1).unwrap_or("/");
    let path = target.split('?').next().unwrap_or(target);
    let ok = |len: usize| {
        format!("HTTP/1.1 200 OK\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n")
    };
    let _ = match path {
        "/latest.json" => stream.write_all(format!("{}{manifest}", ok(manifest.len())).as_bytes()),
        "/update" => stream
            .write_all(ok(PAYLOAD.len()).as_bytes())
            .and_then(|_| stream.write_all(PAYLOAD)),
        "/slow" => {
            let _ = stream.write_all(ok(64 * 1024 * 1024).as_bytes());
            let deadline = Instant::now() + Duration::from_secs(30);
            while Instant::now() < deadline && stream.write_all(&[0; 1024]).is_ok() {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(())
        }
        _ => stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
    };
}

/// A manifest announcing `version` with one entry per `targets` key, every
/// one pointing at `/update`, signed by `signer`.
fn manifest(feed_base: &str, version: &str, targets: &[&str], signer: &Key) -> Value {
    let signature = signer.sign(PAYLOAD, version);
    let platforms: serde_json::Map<String, Value> = targets
        .iter()
        .map(|t| {
            let entry = json!({ "url": format!("{feed_base}/update?{t}"), "signature": signature });
            (t.to_string(), entry)
        })
        .collect();
    json!({ "version": version, "notes": "Fixes.", "pub_date": "2026-11-10T00:00:00Z", "platforms": platforms })
}

/// The installer kind these tests act as on this OS, and its manifest key.
fn this_kind() -> (BundleType, String) {
    let bundle = if cfg!(windows) {
        BundleType::Nsis
    } else if cfg!(target_os = "macos") {
        BundleType::App
    } else {
        BundleType::AppImage
    };
    (bundle.clone(), manifest_target(Some(bundle)).0)
}

/// Serve `version` for this OS's installer kind, signed by `signer`, and a
/// seam pointing at it that trusts `trusted`.
fn feed_offering(version: &str, signer: &Key, trusted: &Key) -> (Feed, Seam) {
    let (bundle, key) = this_kind();
    // The manifest names the payload's URL, so it's served by a second feed.
    let payload = Feed::serve(json!({}));
    let feed = Feed::serve(manifest(&payload.base, version, &[&key], signer));
    let seam = Seam {
        endpoint: Some(feed.url("/latest.json").parse().unwrap()),
        pubkey: Some(trusted.public()),
        bundle: Some(bundle),
        dry_install: true,
        ..Seam::default()
    };
    (feed, seam)
}

fn app() -> tauri::App<MockRuntime> {
    let mut ctx = mock_context(noop_assets());
    ctx.config_mut().plugins.0.insert(
        "updater".into(),
        json!({ "pubkey": "", "endpoints": [], "requireSignedVersion": true }),
    );
    ctx.package_info_mut().version = v("0.3.0");
    mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(ctx)
        .unwrap()
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

fn check_at(current: &str, seam: &Seam) -> Result<Option<(UpdateInfo, Update)>, String> {
    let app = app();
    block_on(check(app.handle(), v(current), seam, || {}))
}

#[test]
fn a_newer_version_is_offered_with_its_details() {
    let key = Key::new();
    let (_feed, seam) = feed_offering("0.3.1", &key, &key);
    let (info, update) = check_at("0.3.0", &seam).unwrap().expect("offered");
    assert_eq!(
        info,
        UpdateInfo {
            version: "0.3.1".into(),
            current_version: "0.3.0".into(),
            notes_url: "https://github.com/SimonPerryman/rust-download-manager/releases/tag/v0.3.1"
                .into(),
            can_install: true,
        }
    );
    // The download verifies against the trusted key.
    assert_eq!(block_on(download(&update, |_| {})).unwrap(), PAYLOAD);
}

#[test]
fn the_same_or_an_older_version_is_not_offered() {
    let key = Key::new();
    let (_feed, seam) = feed_offering("0.3.0", &key, &key);
    assert!(check_at("0.3.0", &seam).unwrap().is_none());
    assert!(check_at("0.3.1", &seam).unwrap().is_none());
}

#[test]
fn a_pre_release_is_not_offered_to_a_stable_build() {
    let key = Key::new();
    let (_feed, seam) = feed_offering("0.4.0-rc.1", &key, &key);
    assert!(check_at("0.3.0", &seam).unwrap().is_none());
    assert!(check_at("0.3.0-rc.2", &seam).unwrap().is_some());
}

#[test]
fn a_bad_signature_is_rejected() {
    let (signer, trusted) = (Key::new(), Key::new());
    let (_feed, seam) = feed_offering("0.3.1", &signer, &trusted);
    let (_, update) = check_at("0.3.0", &seam).unwrap().expect("offered");
    let err = block_on(download(&update, |_| {})).unwrap_err();
    assert!(err.contains("signature didn't verify"), "{err}");
}

#[test]
fn a_signature_for_another_version_is_rejected() {
    let key = Key::new();
    let placeholder = Feed::serve(json!({}));
    let (bundle, target) = this_kind();
    let mut doc = manifest(&placeholder.base, "0.3.1", &[&target], &key);
    // A genuine 0.3.1 signature, announced as 0.9.0.
    doc["version"] = json!("0.9.0");
    let feed = Feed::serve(doc);
    let seam = Seam {
        endpoint: Some(feed.url("/latest.json").parse().unwrap()),
        pubkey: Some(key.public()),
        bundle: Some(bundle),
        dry_install: true,
        ..Seam::default()
    };
    let (_, update) = check_at("0.3.0", &seam).unwrap().expect("offered");
    let err = block_on(download(&update, |_| {})).unwrap_err();
    assert!(err.contains("signature didn't verify"), "{err}");
}

#[test]
fn the_entry_matches_the_installer_kind() {
    // Only Windows ships two installer kinds; the Windows CI job runs this.
    if !cfg!(windows) {
        return;
    }
    let key = Key::new();
    let placeholder = Feed::serve(json!({}));
    let nsis = manifest_target(Some(BundleType::Nsis)).0;
    let msi = manifest_target(Some(BundleType::Msi)).0;
    let feed = Feed::serve(manifest(&placeholder.base, "0.3.1", &[&nsis, &msi], &key));
    let seam_for = |bundle| Seam {
        endpoint: Some(feed.url("/latest.json").parse().unwrap()),
        pubkey: Some(key.public()),
        bundle: Some(bundle),
        dry_install: true,
        ..Seam::default()
    };
    let url_for = |bundle| {
        let (_, update) = check_at("0.3.0", &seam_for(bundle)).unwrap().unwrap();
        update.download_url.query().unwrap_or_default().to_string()
    };
    assert_eq!(url_for(BundleType::Msi), msi);
    assert_eq!(url_for(BundleType::Nsis), nsis);
}

#[test]
fn a_release_without_this_installer_kind_offers_nothing() {
    let key = Key::new();
    let placeholder = Feed::serve(json!({}));
    let feed = Feed::serve(manifest(
        &placeholder.base,
        "0.3.1",
        &["other-os-x86_64-nsis"],
        &key,
    ));
    let seam = Seam {
        endpoint: Some(feed.url("/latest.json").parse().unwrap()),
        pubkey: Some(key.public()),
        bundle: Some(this_kind().0),
        dry_install: true,
        ..Seam::default()
    };
    assert!(check_at("0.3.0", &seam).unwrap().is_none());
}

// ---- Installing, up to the test seam ----

struct Fixture {
    app: tauri::App<MockRuntime>,
    state: UpdaterState,
    manager: Arc<Manager>,
    dir: tempfile::TempDir,
    runtime: tokio::runtime::Runtime,
    /// Serves the files downloads fetch.
    files: Feed,
}

impl Fixture {
    fn new() -> Self {
        Self::with(|_| {})
    }

    /// The fixture with its test seam adjusted.
    fn with(tweak: impl FnOnce(&mut Seam)) -> Self {
        let key = Key::new();
        let (_updates, mut seam) = feed_offering("0.3.1", &key, &key);
        tweak(&mut seam);
        let dir = tempfile::tempdir().unwrap();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let store = JsonStore::new(dir.path().join("state.json"));
        let manager = Arc::new(Manager::load(Box::new(store), runtime.handle().clone()).unwrap());
        Self {
            app: app(),
            state: UpdaterState::new(seam),
            manager,
            dir,
            runtime,
            files: Feed::serve(json!({})),
        }
    }

    /// Start a download that keeps running, with its partial file on disk.
    fn start_download(&self) -> u64 {
        let id = self.manager.add(
            self.files.url("/slow"),
            self.dir.path().join("files"),
            None,
            None,
        );
        self.wait_status(id, Status::Downloading);
        // Downloading is reported before the partial file is created; a
        // pause in between has no partial file to keep.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.part_exists(id) {
            assert!(Instant::now() < deadline, "no partial file");
            std::thread::sleep(Duration::from_millis(10));
        }
        id
    }

    fn part_exists(&self, id: u64) -> bool {
        let snapshot = self.manager.snapshot();
        let target = &snapshot.iter().find(|s| s.id == id).unwrap().target;
        std::path::Path::new(&format!("{}.part", target.display())).exists()
    }

    fn wait_status(&self, id: u64, want: Status) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.status(id) != want {
            assert!(Instant::now() < deadline, "never reached {want:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn status(&self, id: u64) -> Status {
        self.manager
            .snapshot()
            .into_iter()
            .find(|s| s.id == id)
            .unwrap()
            .status
    }

    fn saved(&self) -> Value {
        serde_json::from_slice(&std::fs::read(self.dir.path().join("state.json")).unwrap()).unwrap()
    }

    fn check(&self) -> Option<UpdateInfo> {
        let manager = self.manager.clone();
        self.runtime
            .block_on(check_command(self.app.handle(), &self.state, manager))
            .unwrap()
    }

    fn install(&self, when: InstallWhen) -> Result<(), String> {
        self.runtime.block_on(install_command(
            self.app.handle(),
            &self.state,
            &self.manager,
            when,
        ))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.manager.shutdown();
    }
}

#[test]
fn restart_now_pauses_downloads_and_marks_them_as_paused_for_the_update() {
    let f = Fixture::new();
    let id = f.start_download();
    assert_eq!(f.check().unwrap().version, "0.3.1");

    f.install(InstallWhen::Now).unwrap();

    let saved = &f.saved()["downloads"][0];
    assert_eq!(saved["id"], id);
    assert_eq!(saved["status"], "paused");
    assert_eq!(saved["paused_for_update"], true);
    let part = format!("{}.part", saved["target"].as_str().unwrap());
    assert!(
        std::path::Path::new(&part).exists(),
        "partial file {part} is gone"
    );
}

#[test]
fn install_at_quit_leaves_downloads_running() {
    let f = Fixture::new();
    let id = f.start_download();
    f.check().unwrap();

    f.install(InstallWhen::OnQuit).unwrap();

    assert_eq!(f.state.staged_version().as_deref(), Some("0.3.1"));
    assert_eq!(f.status(id), Status::Downloading);
    // The quit: the manager shuts down as usual, then the staged update
    // reaches the installer step (stopped there by the seam).
    f.manager.shutdown();
    install_at_exit(&f.state);
    assert_eq!(f.state.staged_version(), None);
    assert_eq!(f.saved()["downloads"][0]["status"], "paused");
    assert!(f.saved()["downloads"][0].get("paused_for_update").is_none());
}

/// Restart now drops an update staged for the quit, which would otherwise be
/// installed over the newer one as the app exits to restart.
#[test]
fn restart_now_drops_an_update_staged_for_the_quit() {
    let f = Fixture::new();
    f.check().unwrap();
    f.install(InstallWhen::OnQuit).unwrap();
    assert!(f.state.staged_version().is_some());
    f.check().unwrap();
    f.install(InstallWhen::Now).unwrap();
    assert_eq!(f.state.staged_version(), None);
}

/// A Restart now whose installer fails keeps the update for another try. On
/// macOS and Linux the downloads were never touched; on Windows the hook had
/// already paused and marked them, so rdm restarts into this version.
#[test]
fn a_failed_restart_now_keeps_the_update() {
    let f = Fixture::with(|s| {
        s.dry_install = false;
        s.fail_install = true;
    });
    let id = f.start_download();
    f.check().unwrap();
    f.install(InstallWhen::OnQuit).unwrap();
    f.check().unwrap();

    let err = f.install(InstallWhen::Now).unwrap_err();
    if cfg!(windows) {
        assert!(err.contains("rdm is restarting"), "{err}");
        assert!(f.state.restart_requested.load(Ordering::SeqCst));
        assert_eq!(f.saved()["downloads"][0]["paused_for_update"], true);
        // Not installed on the way out of the restart.
        assert_eq!(f.state.staged_version(), None);
    } else {
        assert!(err.contains("Couldn't install the update"), "{err}");
        assert!(!f.state.restart_requested.load(Ordering::SeqCst));
        assert_eq!(f.status(id), Status::Downloading);
        assert_eq!(f.state.staged_version().as_deref(), Some("0.3.1"));
        // Trying again needs no new check.
        let again = f.install(InstallWhen::Now).unwrap_err();
        assert!(again.contains("Couldn't install the update"), "{again}");
    }
}

/// Windows' before-exit hook pauses and marks downloads only for Restart now.
#[test]
fn the_before_exit_hook_marks_downloads_only_for_a_restart() {
    let f = Fixture::new();
    f.start_download();
    let hook = RestartHook::default();
    hook.run(&f.manager);
    assert!(!hook.paused.load(Ordering::SeqCst));
    assert_eq!(f.status(1), Status::Downloading);

    hook.restarting.store(true, Ordering::SeqCst);
    hook.run(&f.manager);
    assert!(hook.paused.load(Ordering::SeqCst));
    assert_eq!(f.saved()["downloads"][0]["paused_for_update"], true);
}

/// An install that can't update in place (a `.deb`) is refused, whatever the
/// frontend shows.
#[test]
fn an_install_that_cant_update_in_place_is_refused() {
    let f = Fixture::new();
    let seam = Seam {
        bundle: Some(BundleType::Deb),
        ..f.state.seam.clone()
    };
    let state = UpdaterState::new(seam);
    let info = f
        .runtime
        .block_on(check_command(f.app.handle(), &state, f.manager.clone()))
        .unwrap()
        .expect("offered");
    assert!(!info.can_install);
    let err = f
        .runtime
        .block_on(install_command(
            f.app.handle(),
            &state,
            &f.manager,
            InstallWhen::Now,
        ))
        .unwrap_err();
    assert!(err.contains("can't update itself"), "{err}");
}

#[test]
fn install_needs_a_checked_update() {
    let f = Fixture::new();
    let err = f.install(InstallWhen::Now).unwrap_err();
    assert!(err.contains("Check for updates first"), "{err}");
}

// ---- The seam can't reach a release build ----

/// The environment overrides live only in `seam.rs`, which is compiled into
/// debug builds only.
#[test]
fn the_test_seam_is_compiled_out_of_release_builds() {
    let src = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let module = std::fs::read_to_string(format!("{src}/updater/mod.rs")).unwrap();
    assert!(
        module.contains("#[cfg(debug_assertions)]\nmod seam;"),
        "seam.rs must be declared under #[cfg(debug_assertions)]"
    );
    let mut stack = vec![std::path::PathBuf::from(src)];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if !path.ends_with("seam.rs") && !path.ends_with("tests.rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                assert!(
                    !text.contains("RDM_UPDATE_"),
                    "{} reads a test-seam variable outside seam.rs",
                    path.display()
                );
            }
        }
    }
}

/// The webview gets no updater permission, and the CSP still allows only IPC.
#[test]
fn the_webview_gets_no_updater_access() {
    let root = env!("CARGO_MANIFEST_DIR");
    let caps = std::fs::read_to_string(format!("{root}/capabilities/default.json")).unwrap();
    assert!(
        !caps.contains("updater:"),
        "capabilities grant an updater permission"
    );
    let conf: Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{root}/tauri.conf.json")).unwrap())
            .unwrap();
    let csp = conf["app"]["security"]["csp"].as_str().unwrap();
    let connect = csp
        .split(';')
        .map(str::trim)
        .find(|d| d.starts_with("connect-src"))
        .unwrap();
    assert_eq!(connect, "connect-src ipc: http://ipc.localhost");
}

/// Stable builds read the endpoint `tauri.conf.json` declares.
#[test]
fn the_stable_manifest_is_the_configured_endpoint() {
    let conf: Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        conf["plugins"]["updater"]["endpoints"],
        json!([STABLE_MANIFEST])
    );
}
