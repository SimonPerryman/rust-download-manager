//! rdm — a download manager in Rust.
//!
//! The crate is split into an engine (the core download logic, here) and thin
//! frontends (the `rdm` binary, and later a GUI) that only call the engine.
//! Keeping the engine in the library crate is also what lets the integration
//! tests drive it directly against a local server.

pub mod engine;
pub mod platform;

pub use engine::{
    download, map_urls_in, parse_rate, redact_url, redact_urls_in, AddOptions, Algo, Checksum,
    ChecksumState, ClaimTarget, Control, DownloadRecord, DownloadRequest, DownloadSnapshot,
    DownloadState, Error, JsonStore, LogLevel, LogLine, Manager, Progress, RateLimiter, Result,
    RetryPolicy, Segment, Settings, Status, Store, StoreData, Summary, LOG_LINES,
};
