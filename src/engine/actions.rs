//! Post-download actions: a command run after each download, and a power
//! action (shut down, sleep, hibernate) once everything is done. The Manager
//! announces [`LifecycleEvent`]s; these are the settings, the command runner
//! and the power backends the subscribers use.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use super::schedule::StartAt;

/// How long a power action waits, with a chance to cancel, once everything
/// is done.
pub const POWER_COUNTDOWN_SECS: u64 = 60;

/// What the Manager announces on its event channel (see
/// [`Manager::subscribe_events`](super::Manager::subscribe_events)).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LifecycleEvent {
    /// A download completed.
    Finished {
        /// The download.
        id: u64,
        /// Where the finished file is.
        path: PathBuf,
        /// The file's name.
        name: String,
    },
    /// The last running or waiting download just ended (completed or
    /// failed), so nothing is left in the queue.
    QueueEmptied,
}

/// What to do to the machine once everything is done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PowerAction {
    /// Turn the machine off.
    #[default]
    Shutdown,
    /// Suspend to memory.
    Sleep,
    /// Suspend to disk.
    Hibernate,
}

impl PowerAction {
    /// The lowercase name, as saved and as the CLI spells it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Shutdown => "shutdown",
            Self::Sleep => "sleep",
            Self::Hibernate => "hibernate",
        }
    }
}

impl std::str::FromStr for PowerAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "shutdown" => Ok(Self::Shutdown),
            "sleep" => Ok(Self::Sleep),
            "hibernate" => Ok(Self::Hibernate),
            other => Err(format!("unknown action `{other}`")),
        }
    }
}

/// Run a command after each download. Saved with the settings.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandAction {
    /// Off unless switched on.
    pub enabled: bool,
    /// The executable, started directly (never through a shell).
    pub program: String,
    /// One argument per entry; `{download_path}` and `{download_name}` in each are filled in
    /// with the finished file's path and name.
    pub args: Vec<String>,
}

/// Power the machine down when everything is done. Saved with the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PowerSetting {
    /// Off unless switched on.
    pub enabled: bool,
    /// What to do.
    pub action: PowerAction,
}

/// Both post-download actions. Global and off by default.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PostActions {
    /// Run a command after each download.
    pub command: CommandAction,
    /// Shut down, sleep or hibernate when everything is done.
    pub power: PowerSetting,
}

impl PostActions {
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// What a pending power action is waiting for, most pressing first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WaitReason {
    /// A download is running.
    Running,
    /// Downloads are waiting for a slot.
    Queued,
    /// Downloads are waiting for the queue window to open.
    Window,
    /// A download is scheduled to start later.
    Scheduled {
        /// When the earliest one starts.
        at: StartAt,
    },
    /// A command is still running.
    Command,
}

/// Where the power action stands, for the row that switches it on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PowerStatus {
    /// Switched off.
    Off,
    /// On, with nothing in the way: it runs after the next downloads finish.
    Ready,
    /// On, and held back by `reason`.
    Waiting {
        /// What it is waiting for.
        reason: WaitReason,
    },
    /// Counting down; Cancel stops it.
    Countdown {
        /// Whole seconds left.
        seconds_left: u64,
        /// What will happen.
        action: PowerAction,
    },
    /// The machine would not do it.
    Failed {
        /// Why.
        message: String,
    },
}

/// What the Manager says about the post-download actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionsStatus {
    /// The power action.
    pub power: PowerStatus,
}

/// Carries out a power action. Everything goes through this trait so tests
/// use a fake and never touch the machine.
pub trait PowerBackend: Send + Sync {
    /// Do `action` now. An error says why the machine refused.
    fn perform(&self, action: PowerAction) -> Result<(), String>;
}

/// The default backend: refuses, so a Manager nobody gave a real backend
/// (every test) can never power anything down.
#[derive(Debug, Default)]
pub struct NoPower;

impl PowerBackend for NoPower {
    fn perform(&self, _: PowerAction) -> Result<(), String> {
        Err("power actions are not available here".into())
    }
}

/// Appends each action's name to a file instead of doing it. The backend the
/// test seams install.
#[derive(Debug)]
pub struct LogPower(pub PathBuf);

impl PowerBackend for LogPower {
    fn perform(&self, action: PowerAction) -> Result<(), String> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.0)
            .map_err(|e| e.to_string())?;
        writeln!(file, "{}", action.name()).map_err(|e| e.to_string())
    }
}

/// The real machine. Only a frontend installs it, and it refuses to run in
/// this crate's own unit tests.
#[derive(Debug, Default)]
pub struct SystemPower;

impl PowerBackend for SystemPower {
    fn perform(&self, action: PowerAction) -> Result<(), String> {
        if cfg!(test) {
            return Err("the real power backend never runs in tests".into());
        }
        os::perform(action)
    }
}

/// The CLI's backend: a [`LogPower`] when `RDM_TEST_POWER_LOG` names a file
/// (so a test can check what would have run), else the real machine.
pub fn backend_from_env() -> Box<dyn PowerBackend> {
    match std::env::var_os("RDM_TEST_POWER_LOG") {
        Some(path) => Box::new(LogPower(PathBuf::from(path))),
        None => Box::new(SystemPower),
    }
}

#[cfg(windows)]
mod os {
    use super::PowerAction;
    use windows_sys::Win32::System::Power::SetSuspendState;

    pub(super) fn perform(action: PowerAction) -> Result<(), String> {
        match action {
            PowerAction::Shutdown => super::run_tool("shutdown", &["/s", "/t", "0"]),
            // SAFETY: a plain call that takes three flags.
            PowerAction::Sleep => suspend(unsafe { SetSuspendState(false, false, false) }),
            PowerAction::Hibernate => suspend(unsafe { SetSuspendState(true, false, false) }),
        }
    }

    fn suspend(ok: impl Into<i32>) -> Result<(), String> {
        if ok.into() == 0 {
            Err("the system refused to suspend".into())
        } else {
            Ok(())
        }
    }
}

#[cfg(target_os = "macos")]
mod os {
    use super::PowerAction;

    pub(super) fn perform(action: PowerAction) -> Result<(), String> {
        match action {
            PowerAction::Shutdown => super::run_tool(
                "osascript",
                &["-e", "tell application \"System Events\" to shut down"],
            ),
            PowerAction::Sleep => super::run_tool("pmset", &["sleepnow"]),
            PowerAction::Hibernate => Err("this system can't hibernate".into()),
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod os {
    use super::PowerAction;

    pub(super) fn perform(action: PowerAction) -> Result<(), String> {
        let verb = match action {
            PowerAction::Shutdown => "poweroff",
            PowerAction::Sleep => "suspend",
            PowerAction::Hibernate => "hibernate",
        };
        super::run_tool("systemctl", &[verb])
    }
}

/// Run a system tool and say why if it did not succeed.
#[cfg(any(unix, windows))]
fn run_tool(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| format!("couldn't run {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} {}", describe_exit(status)))
    }
}

/// Fill `{download_path}` and `{download_name}` in `template` in one pass, so a value that
/// itself contains `{download_name}` is left as it is.
pub fn expand(template: &str, path: &Path, name: &str) -> String {
    let path = path.to_string_lossy();
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        if let Some(tail) = rest.strip_prefix("{download_path}") {
            out.push_str(&path);
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("{download_name}") {
            out.push_str(name);
            rest = tail;
        } else {
            out.push('{');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

/// Run the command for one finished download and wait for it. Started
/// directly with one argument per entry, never through a shell. An error
/// says how it failed (it did not start, or exited non-zero).
pub fn run_command(command: &CommandAction, path: &Path, name: &str) -> Result<(), String> {
    let mut cmd = Command::new(&command.program);
    cmd.args(command.args.iter().map(|a| expand(a, path, name)))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up for each download.
        cmd.creation_flags(0x0800_0000);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("couldn't start {}: {e}", command.program))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} {}", command.program, describe_exit(status)))
    }
}

fn describe_exit(status: std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exited with code {code}"),
        None => "was stopped by a signal".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_fills_both_placeholders_once() {
        let path = Path::new("/dl/a;b {download_name}.zip");
        assert_eq!(
            expand(
                "--in={download_path} [{download_name}] {other} {",
                path,
                "a;b {download_name}.zip"
            ),
            "--in=/dl/a;b {download_name}.zip [a;b {download_name}.zip] {other} {"
        );
    }

    #[test]
    fn expand_leaves_the_old_short_names_as_typed() {
        assert_eq!(
            expand("{path} {name}", Path::new("/dl/a.zip"), "a.zip"),
            "{path} {name}"
        );
    }

    #[test]
    fn the_real_backend_refuses_in_tests() {
        assert!(SystemPower.perform(PowerAction::Shutdown).is_err());
        assert!(SystemPower.perform(PowerAction::Sleep).is_err());
        assert!(SystemPower.perform(PowerAction::Hibernate).is_err());
    }

    #[test]
    fn old_settings_without_actions_are_all_off() {
        let none: PostActions = serde_json::from_str("{}").unwrap();
        assert!(none.is_default());
        assert!(!none.command.enabled && !none.power.enabled);
        let some: PostActions =
            serde_json::from_str(r#"{"power":{"enabled":true,"action":"sleep"}}"#).unwrap();
        assert_eq!(some.power.action, PowerAction::Sleep);
    }
}
