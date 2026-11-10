//! Test-only fault injection (the `fault-injection` cargo feature): lets the
//! integration tests make a download's task, or hashing on the blocking pool,
//! panic, to prove a panic is contained and reported. Never part of a release
//! build.

#[cfg(not(debug_assertions))]
compile_error!("the `fault-injection` feature is for tests only; never build a release with it");

/// The file-name prefix that makes hashing a file panic.
const PANIC_FILE: &str = "__panic";

/// The URL path that makes a download panic.
const PANIC_PATH: &str = "/__panic";

/// Panic when `url`'s path is [`PANIC_PATH`], as a bug inside the download
/// task would.
pub(crate) fn panic_on_request(url: &str) {
    let path = url::Url::parse(url).map(|u| u.path().to_owned());
    if path.as_deref() == Ok(PANIC_PATH) {
        panic!("injected fault: download task panicked");
    }
}

/// Panic when hashing a file named `__panic…` (a checksum check on the
/// blocking pool), as a bug in that step would.
pub(crate) fn panic_on_file(path: &std::path::Path) {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    if name.starts_with(PANIC_FILE) {
        panic!("injected fault: hashing panicked");
    }
}
