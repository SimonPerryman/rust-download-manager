//! Standalone copy of the offline test fixture, for the GUI's webview E2E
//! suite (`gui/e2e`). Serves the same routes as `tests/common` so they stay
//! single-sourced; an example (not a bin) so it never ships in the CLI.
//!
//! Usage: `cargo run --example fixture_server -- [--port N] [--slow-delay-ms N]
//! [--seg-delay-ms N]` (port 0, the default, picks a free one; the delays are
//! per chunk for `/slow`-style routes and `/seg` respectively). Prints
//! `listening on <base-url>` once bound.

#[path = "../tests/common/mod.rs"]
mod common;

use std::io::Write;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut port: u16 = 0;
    let mut cfg = common::FixtureConfig::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().ok_or_else(|| format!("{arg} needs a value"))?;
        match arg.as_str() {
            "--port" => port = value.parse()?,
            "--slow-delay-ms" => cfg.slow_chunk_delay = Duration::from_millis(value.parse()?),
            "--seg-delay-ms" => cfg.seg_chunk_delay = Duration::from_millis(value.parse()?),
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = listener.local_addr()?;
    println!("listening on http://{addr}");
    std::io::stdout().flush()?;
    common::serve(listener, cfg).await;
    Ok(())
}
