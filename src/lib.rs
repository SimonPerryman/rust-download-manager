//! rdm — a download manager in Rust.
//!
//! The crate is split into an engine (the core download logic, here) and thin
//! frontends (the `rdm` binary, and later a GUI) that only call the engine.
//! Keeping the engine in the library crate is also what lets the integration
//! tests drive it directly against a local server.

pub mod engine;

pub use engine::{
    download, parse_rate, AddOptions, Algo, Checksum, ChecksumState, ClaimTarget, Control,
    DownloadRecord, DownloadRequest, DownloadSnapshot, DownloadState, Error, JsonStore, Manager,
    Progress, RateLimiter, Result, RetryPolicy, Segment, Settings, Status, Store, StoreData,
    Summary,
};
