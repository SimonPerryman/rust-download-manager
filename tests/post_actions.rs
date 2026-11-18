//! Post-download actions: the events the Manager announces, the command run
//! after each download (no shell, a failure only warns), and the power action
//! (held back by anything still in play, counted down, cancellable). The
//! power backend is a fake and its countdown moves by hand: no test ever
//! sleeps or shuts the machine down.

mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rdm::engine::actions::{
    ActionsStatus, CommandAction, LifecycleEvent, NoPower, PostActions, PowerAction, PowerBackend,
    PowerSetting, PowerStatus, WaitReason,
};
use rdm::engine::schedule::{
    Days, FakeClock, LocalDateTime, QueueWindow, Schedule, StartAt, TimeOfDay, Window,
};
use rdm::{AddOptions, DownloadSnapshot, JsonStore, Manager, Status, Store};

const T: Duration = Duration::from_secs(10);

/// Records what it was asked to do; never touches the machine.
#[derive(Default)]
struct FakePower(Mutex<Vec<PowerAction>>);

impl PowerBackend for FakePower {
    fn perform(&self, action: PowerAction) -> Result<(), String> {
        self.0.lock().unwrap().push(action);
        Ok(())
    }
}

impl FakePower {
    fn done(&self) -> Vec<PowerAction> {
        self.0.lock().unwrap().clone()
    }
}

// 2026-03-09 is a Monday.
fn on(h: u8, mi: u8) -> LocalDateTime {
    LocalDateTime::from_parts(2026, 3, 9, h, mi).unwrap()
}

struct Rig {
    mgr: Manager,
    power: Arc<FakePower>,
    clock: Arc<FakeClock>,
    dir: tempfile::TempDir,
    server: common::TestServer,
}

async fn rig() -> Rig {
    let server = common::spawn().await;
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(FakeClock::new(on(12, 0)));
    let store: Box<dyn Store> = Box::new(JsonStore::new(dir.path().join("state.json")));
    let mgr =
        Manager::load_with_clock(store, tokio::runtime::Handle::current(), clock.clone()).unwrap();
    let power = Arc::new(FakePower::default());
    mgr.set_power_backend(power.clone());
    mgr.hold_actions_clock();
    Rig {
        mgr,
        power,
        clock,
        dir,
        server,
    }
}

fn power_on(action: PowerAction) -> PostActions {
    PostActions {
        power: PowerSetting {
            enabled: true,
            action,
        },
        ..PostActions::default()
    }
}

fn snap(mgr: &Manager, id: u64) -> DownloadSnapshot {
    mgr.snapshot().into_iter().find(|s| s.id == id).unwrap()
}

async fn wait_for(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + T;
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_status(mgr: &Manager, id: u64, want: Status) {
    wait_for(&format!("{want:?}"), || snap(mgr, id).status == want).await;
}

async fn wait_power(mgr: &Manager, want: PowerStatus) {
    let want = ActionsStatus { power: want };
    wait_for(&format!("{want:?}"), || mgr.actions_status() == want).await;
}

fn countdown(left: u64, action: PowerAction) -> PowerStatus {
    PowerStatus::Countdown {
        seconds_left: left,
        action,
    }
}

fn waiting(reason: WaitReason) -> PowerStatus {
    PowerStatus::Waiting { reason }
}

/// The probe example, built if `cargo test` didn't already.
fn probe() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let name = format!("argv_probe{}", std::env::consts::EXE_SUFFIX);
    let path = exe
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples")
        .join(name);
    if !path.exists() {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let built = std::process::Command::new(cargo)
            .args(["build", "--example", "argv_probe"])
            .status()
            .unwrap();
        assert!(
            built.success() && path.exists(),
            "could not build argv_probe"
        );
    }
    path
}

/// A command that runs the probe: it records `{name}` and `{path}` into
/// `out`, waits for `gate` (if any) and exits with `code`.
fn probe_command(out: &Path, code: i32, gate: Option<&Path>) -> CommandAction {
    CommandAction {
        enabled: true,
        program: probe().to_string_lossy().into_owned(),
        args: vec![
            out.to_string_lossy().into_owned(),
            code.to_string(),
            gate.map_or("-".into(), |g| g.to_string_lossy().into_owned()),
            "{name}".into(),
            "{path}".into(),
        ],
    }
}

fn with_command(command: CommandAction, power: Option<PowerAction>) -> PostActions {
    PostActions {
        command,
        power: PowerSetting {
            enabled: power.is_some(),
            action: power.unwrap_or_default(),
        },
    }
}

#[tokio::test]
async fn the_manager_announces_a_finish_and_an_empty_queue() {
    let r = rig().await;
    let mut events = r.mgr.subscribe_events();
    let id = r.mgr.add(r.server.url("/file"), r.dir.path(), None, None);
    wait_status(&r.mgr, id, Status::Completed).await;

    let done = snap(&r.mgr, id);
    let path = done.final_path.clone().unwrap();
    let first = tokio::time::timeout(T, events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        first,
        LifecycleEvent::Finished {
            id,
            path,
            name: done.filename,
        }
    );
    let second = tokio::time::timeout(T, events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second, LifecycleEvent::QueueEmptied);
}

#[tokio::test]
async fn a_power_action_counts_down_then_fires_once() {
    let r = rig().await;
    r.mgr.set_post_actions(power_on(PowerAction::Sleep));
    assert_eq!(r.mgr.actions_status().power, PowerStatus::Ready);

    let id = r.mgr.add(r.server.url("/file"), r.dir.path(), None, None);
    wait_status(&r.mgr, id, Status::Completed).await;
    wait_power(&r.mgr, countdown(60, PowerAction::Sleep)).await;

    r.mgr.advance_actions_clock(59);
    assert_eq!(
        r.mgr.actions_status().power,
        countdown(1, PowerAction::Sleep)
    );
    assert!(r.power.done().is_empty(), "not before the countdown ends");

    r.mgr.advance_actions_clock(1);
    assert_eq!(r.power.done(), [PowerAction::Sleep]);
    assert_eq!(r.mgr.actions_status().power, PowerStatus::Ready);
    r.mgr.advance_actions_clock(120);
    assert_eq!(r.power.done().len(), 1, "once per time everything is done");
}

#[tokio::test]
async fn cancel_stops_it_during_the_countdown() {
    let r = rig().await;
    r.mgr.set_post_actions(power_on(PowerAction::Shutdown));
    let id = r.mgr.add(r.server.url("/file"), r.dir.path(), None, None);
    wait_status(&r.mgr, id, Status::Completed).await;
    wait_power(&r.mgr, countdown(60, PowerAction::Shutdown)).await;

    r.mgr.advance_actions_clock(30);
    r.mgr.cancel_power();
    assert_eq!(r.mgr.actions_status().power, PowerStatus::Ready);
    r.mgr.advance_actions_clock(300);
    assert!(r.power.done().is_empty());
}

#[tokio::test]
async fn switching_it_off_ends_the_countdown() {
    let r = rig().await;
    r.mgr.set_post_actions(power_on(PowerAction::Shutdown));
    let id = r.mgr.add(r.server.url("/file"), r.dir.path(), None, None);
    wait_status(&r.mgr, id, Status::Completed).await;
    wait_power(&r.mgr, countdown(60, PowerAction::Shutdown)).await;

    r.mgr.set_post_actions(PostActions::default());
    assert_eq!(r.mgr.actions_status().power, PowerStatus::Off);
    r.mgr.advance_actions_clock(300);
    assert!(r.power.done().is_empty());
}

#[tokio::test]
async fn it_is_dropped_while_a_download_is_queued_or_running() {
    let r = rig().await;
    r.mgr.set_max_concurrent(1);
    r.mgr.set_post_actions(power_on(PowerAction::Shutdown));
    let first = r.mgr.add(r.server.url("/gated"), r.dir.path(), None, None);
    let second = r.mgr.add(r.server.url("/gated"), r.dir.path(), None, None);
    assert_eq!(snap(&r.mgr, second).status, Status::Queued);
    assert_eq!(r.mgr.actions_status().power, waiting(WaitReason::Running));

    // The first finishing hands its slot to the second: not everything is done.
    wait_for("the first to reach the gate", || r.server.gate().holding() >= 1).await;
    r.server.gate().release_next();
    wait_status(&r.mgr, first, Status::Completed).await;
    wait_status(&r.mgr, second, Status::Downloading).await;
    assert_eq!(r.mgr.actions_status().power, waiting(WaitReason::Running));
    r.mgr.advance_actions_clock(300);
    assert!(r.power.done().is_empty());

    r.server.gate().release_all();
    wait_status(&r.mgr, second, Status::Completed).await;
    wait_power(&r.mgr, countdown(60, PowerAction::Shutdown)).await;
}

#[tokio::test]
async fn it_is_dropped_while_the_queue_window_holds_a_download() {
    let r = rig().await;
    r.mgr.set_max_concurrent(1);
    r.mgr.set_post_actions(power_on(PowerAction::Shutdown));
    // Open until 18:00, but running downloads are let finish when it closes.
    let time = |h: &str| h.parse::<TimeOfDay>().unwrap();
    r.mgr.set_schedule(Schedule {
        queue_windows: vec![QueueWindow {
            name: None,
            enabled: true,
            window: Window::new(time("08:00"), time("18:00"), Days::DAILY).unwrap(),
            finish_running: true,
        }],
        ..Schedule::default()
    });
    let running = r.mgr.add(r.server.url("/gated"), r.dir.path(), None, None);
    let held = r.mgr.add(r.server.url("/file"), r.dir.path(), None, None);
    wait_status(&r.mgr, running, Status::Downloading).await;

    r.clock.set(on(18, 30));
    r.mgr.check_schedule();
    r.server.gate().release_all();
    wait_status(&r.mgr, running, Status::Completed).await;

    assert_eq!(snap(&r.mgr, held).status, Status::Queued);
    assert_eq!(r.mgr.actions_status().power, waiting(WaitReason::Window));
    r.mgr.advance_actions_clock(300);
    assert!(r.power.done().is_empty());
}

#[tokio::test]
async fn a_pending_start_time_blocks_it_until_that_download_is_done() {
    let r = rig().await;
    r.mgr.set_post_actions(power_on(PowerAction::Shutdown));
    let at = StartAt(on(23, 0));
    let later = r
        .mgr
        .add_with(
            r.server.url("/file"),
            r.dir.path(),
            AddOptions {
                start_at: Some(at),
                ..AddOptions::default()
            },
        )
        .unwrap();
    let now = r.mgr.add(r.server.url("/file"), r.dir.path(), None, None);
    wait_status(&r.mgr, now, Status::Completed).await;

    assert_eq!(
        r.mgr.actions_status().power,
        waiting(WaitReason::Scheduled { at })
    );
    r.mgr.advance_actions_clock(300);
    assert!(
        r.power.done().is_empty(),
        "a start at 23:00 is still to come"
    );

    r.clock.set(on(23, 0));
    r.mgr.check_schedule();
    wait_status(&r.mgr, later, Status::Completed).await;
    wait_power(&r.mgr, countdown(60, PowerAction::Shutdown)).await;
    r.mgr.advance_actions_clock(60);
    assert_eq!(r.power.done(), [PowerAction::Shutdown]);
}

#[tokio::test]
async fn it_waits_for_a_running_command_to_exit() {
    let r = rig().await;
    let gate = r.dir.path().join("release");
    let out = r.dir.path().join("out.txt");
    r.mgr.set_post_actions(with_command(
        probe_command(&out, 0, Some(&gate)),
        Some(PowerAction::Hibernate),
    ));
    let id = r.mgr.add(r.server.url("/file"), r.dir.path(), None, None);
    wait_status(&r.mgr, id, Status::Completed).await;

    wait_power(&r.mgr, waiting(WaitReason::Command)).await;
    r.mgr.advance_actions_clock(300);
    assert!(r.power.done().is_empty(), "the command has not exited");

    std::fs::write(&gate, "").unwrap();
    wait_power(&r.mgr, countdown(60, PowerAction::Hibernate)).await;
    assert!(out.exists(), "the command ran to its end first");
    r.mgr.advance_actions_clock(60);
    assert_eq!(r.power.done(), [PowerAction::Hibernate]);
}

#[tokio::test]
async fn a_backend_that_refuses_is_reported() {
    let r = rig().await;
    // What every Manager has until a frontend installs a real backend.
    r.mgr.set_power_backend(Arc::new(NoPower));
    r.mgr.set_post_actions(power_on(PowerAction::Shutdown));
    let id = r.mgr.add(r.server.url("/file"), r.dir.path(), None, None);
    wait_status(&r.mgr, id, Status::Completed).await;
    wait_power(&r.mgr, countdown(60, PowerAction::Shutdown)).await;
    r.mgr.advance_actions_clock(60);
    assert!(matches!(
        r.mgr.actions_status().power,
        PowerStatus::Failed { .. }
    ));
}

#[tokio::test]
async fn the_command_gets_the_path_and_name_as_whole_arguments() {
    let r = rig().await;
    let out = r.dir.path().join("out.txt");
    r.mgr
        .set_post_actions(with_command(probe_command(&out, 0, None), None));
    // `;` and a space would split a shell command line in two.
    let target = r.dir.path().join("a;b c.bin");
    let id = r
        .mgr
        .add_with(
            r.server.url("/file"),
            r.dir.path(),
            AddOptions {
                out: Some(target),
                ..AddOptions::default()
            },
        )
        .unwrap();
    wait_status(&r.mgr, id, Status::Completed).await;
    wait_for("the command's output", || out.exists()).await;

    let path = snap(&r.mgr, id).final_path.unwrap();
    let seen = std::fs::read_to_string(&out).unwrap();
    assert_eq!(seen, format!("a;b c.bin\n{}", path.display()));
    assert_eq!(snap(&r.mgr, id).command_warning, None);
}

#[tokio::test]
async fn a_failing_command_warns_without_failing_the_download() {
    let r = rig().await;
    let out = r.dir.path().join("out.txt");
    r.mgr
        .set_post_actions(with_command(probe_command(&out, 3, None), None));
    let id = r.mgr.add(r.server.url("/file"), r.dir.path(), None, None);
    wait_for("the warning", || snap(&r.mgr, id).command_warning.is_some()).await;
    let done = snap(&r.mgr, id);
    assert_eq!(done.status, Status::Completed);
    assert!(done.error.is_none());
    assert!(done.command_warning.unwrap().contains("exited with code 3"));
}

#[tokio::test]
async fn a_command_that_cannot_start_warns_too() {
    let r = rig().await;
    let missing = CommandAction {
        enabled: true,
        program: r
            .dir
            .path()
            .join("no-such-program")
            .to_string_lossy()
            .into_owned(),
        args: vec!["{path}".into()],
    };
    r.mgr.set_post_actions(with_command(missing, None));
    let id = r.mgr.add(r.server.url("/file"), r.dir.path(), None, None);
    wait_for("the warning", || snap(&r.mgr, id).command_warning.is_some()).await;
    assert_eq!(snap(&r.mgr, id).status, Status::Completed);
    assert!(snap(&r.mgr, id)
        .command_warning
        .unwrap()
        .contains("couldn't start"));
}

#[tokio::test]
async fn actions_are_saved_with_settings_and_old_files_load_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.json");
    std::fs::write(&state, r#"{"version":1,"settings":{"max_concurrent":2}}"#).unwrap();
    let load = || {
        let store: Box<dyn Store> = Box::new(JsonStore::new(state.clone()));
        Manager::load(store, tokio::runtime::Handle::current()).unwrap()
    };
    let mgr = load();
    assert_eq!(mgr.post_actions(), PostActions::default());
    assert_eq!(mgr.settings().max_concurrent, 2);

    let actions = with_command(
        CommandAction {
            enabled: true,
            program: "scan".into(),
            args: vec!["--file".into(), "{path}".into()],
        },
        Some(PowerAction::Sleep),
    );
    mgr.set_post_actions(actions.clone());
    mgr.shutdown();
    assert_eq!(load().post_actions(), actions);
}
