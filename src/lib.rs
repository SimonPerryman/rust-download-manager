//! rdm — a download manager in Rust.
//!
//! The crate is split into an engine (the core download logic, here) and thin
//! frontends (the `rdm` binary, and later a GUI) that only call the engine.
//! Keeping the engine in the library crate is also what lets the integration
//! tests drive it directly against a local server.

pub mod engine;
pub mod platform;

pub use engine::{
    check_file, download, find_existing, map_urls_in, parse_list, parse_rate, redact_url,
    redact_urls_in, run_limited, AddOptions, Algo, BatchAdded, BatchMatch, BatchSpec, Checksum,
    ChecksumState, ClaimTarget, Control, DownloadRecord, DownloadRequest, DownloadSnapshot,
    DownloadState, Error, Existing, FindEvent, FindResult, FolderMatch, FolderRelink, FoundFile,
    HistoryEvent, HistoryKind, JsonStore, ListError, ListLink, ListPattern, LogLevel, LogLine,
    Manager, MatchKind, MatchMode, MatchReason, MissingChange, ParsedList, Progress, Proxy,
    ProxyMode, RateLimiter, Result, RetryPolicy, SameFolderOffer, Segment, Settings, SkipReason,
    SkippedLine, Status, Store, StoreData, Summary, TargetClaims, LOG_LINES, MAX_BATCH_LABEL_LEN,
    MAX_LIST_LINKS, MAX_PATTERN_LINKS, PROBE_CONCURRENCY,
};
