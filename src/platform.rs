//! What a bug report needs to know about the machine: the OS and its version,
//! and how much memory this process is using. Read locally, only for the log
//! and diagnostics a user chooses to share.

/// The OS, its version and the CPU architecture, e.g.
/// `Windows 11 (10.0.26200) x86_64` or `Ubuntu 24.04 LTS (Linux 6.8.0) x86_64`.
pub fn os_description() -> String {
    format!("{} {}", os_version(), std::env::consts::ARCH)
}

/// This process's resident memory (working set) in bytes, when the OS says.
/// On macOS it's the peak so far, the only figure available without extra
/// dependencies.
pub fn resident_memory() -> Option<u64> {
    imp::resident_memory()
}

fn os_version() -> String {
    imp::os_version().unwrap_or_else(|| std::env::consts::OS.to_owned())
}

#[cfg(windows)]
mod imp {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::SystemInformation::{OSVERSIONINFOEXW, OSVERSIONINFOW};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    /// Windows 11 still reports major version 10; its builds start here.
    const FIRST_WINDOWS_11_BUILD: u32 = 22_000;

    /// `wProductType` of a desktop (not server) Windows.
    const VER_NT_WORKSTATION: u8 = 1;

    pub(super) fn os_version() -> Option<String> {
        // RtlGetVersion, unlike GetVersionEx, isn't capped by the app manifest.
        // SAFETY: an all-zero OSVERSIONINFOEXW is valid (plain integers).
        let mut info: OSVERSIONINFOEXW = unsafe { std::mem::zeroed() };
        info.dwOSVersionInfoSize = size_of::<OSVERSIONINFOEXW>() as u32;
        // SAFETY: the size field tells the call it may fill the EX layout,
        // which starts with the OSVERSIONINFOW fields.
        let status = unsafe { RtlGetVersion((&raw mut info).cast::<OSVERSIONINFOW>()) };
        if status != 0 {
            return None;
        }
        let (major, minor, build) = (info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber);
        let name = if info.wProductType != VER_NT_WORKSTATION {
            "Server".to_owned()
        } else if major == 10 && build >= FIRST_WINDOWS_11_BUILD {
            "11".to_owned()
        } else {
            format!("{major}")
        };
        Some(format!("Windows {name} ({major}.{minor}.{build})"))
    }

    pub(super) fn resident_memory() -> Option<u64> {
        let size = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        // SAFETY: zeroed counters are valid; the call fills at most `size` bytes.
        let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
        counters.cb = size;
        // SAFETY: the current-process pseudo handle needs no closing.
        let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) };
        (ok != 0).then_some(counters.WorkingSetSize as u64)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    pub(super) fn os_version() -> Option<String> {
        let release = std::fs::read_to_string("/etc/os-release").ok();
        let distro = release
            .as_deref()
            .and_then(|text| text.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")))
            .map(|name| name.trim_matches('"').to_owned());
        let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .ok()
            .map(|k| format!("Linux {}", k.trim()));
        match (distro, kernel) {
            (Some(distro), Some(kernel)) => Some(format!("{distro} ({kernel})")),
            (distro, kernel) => distro.or(kernel),
        }
    }

    pub(super) fn resident_memory() -> Option<u64> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let kib = status
            .lines()
            .find_map(|l| l.strip_prefix("VmRSS:"))?
            .trim()
            .strip_suffix("kB")?
            .trim()
            .parse::<u64>()
            .ok()?;
        Some(kib * 1024)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    pub(super) fn os_version() -> Option<String> {
        let plist =
            std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist").ok()?;
        let after = plist.split("<key>ProductVersion</key>").nth(1)?;
        let version = after.split("<string>").nth(1)?.split("</string>").next()?;
        Some(format!("macOS {}", version.trim()))
    }

    pub(super) fn resident_memory() -> Option<u64> {
        // SAFETY: getrusage fills the zeroed struct it's given.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
            return None;
        }
        // Bytes on macOS (kilobytes on Linux, which reads /proc instead).
        u64::try_from(usage.ru_maxrss).ok()
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod imp {
    pub(super) fn os_version() -> Option<String> {
        None
    }

    pub(super) fn resident_memory() -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_the_os_and_architecture() {
        let os = os_description();
        assert!(os.ends_with(std::env::consts::ARCH), "{os}");
        #[cfg(windows)]
        assert!(os.starts_with("Windows "), "{os}");
        #[cfg(target_os = "linux")]
        assert!(os.contains("Linux"), "{os}");
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn reads_resident_memory() {
        let bytes = resident_memory().expect("the OS reports it");
        // More than a megabyte and less than a terabyte: plausible for a test.
        assert!((1 << 20..1 << 40).contains(&bytes), "{bytes}");
    }
}
