//! rdm — a download manager in Rust.
//!
//! The crate is split into an engine (the core download logic, here) and thin
//! frontends (the `rdm` binary, and later a GUI) that only call the engine.
//! Keeping the engine in the library crate is also what lets the integration
//! tests drive it directly against a local server.

pub mod engine;
pub mod platform;

pub use engine::{
    download, map_urls_in, parse_list, parse_rate, redact_url, redact_urls_in, run_limited,
    AddOptions, Algo, Checksum, ChecksumState, ClaimTarget, Control, DownloadRecord,
    DownloadRequest, DownloadSnapshot, DownloadState, Error, FindEvent, FindResult, FolderMatch,
    FolderRelink, FoundFile, JsonStore, ListError, ListLink, LogLevel, LogLine, Manager,
    MatchReason, MissingChange, ParsedList, Progress, Proxy, ProxyMode, RateLimiter, Result,
    RetryPolicy, SameFolderOffer, Segment, Settings, SkipReason, SkippedLine, Status, Store,
    StoreData, Summary, TargetClaims, LOG_LINES, MAX_LIST_LINKS,
};
