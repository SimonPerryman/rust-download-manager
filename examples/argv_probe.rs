//! Test helper for the run-a-command tests (`tests/post_actions.rs`): records
//! the arguments it was started with, so a test can check each arrived intact
//! with no shell in between. An example (not a bin) so it never ships.
//!
//! Usage: `argv_probe <out-file> <exit-code> <gate-file|-> [args...]`. It
//! waits until the gate file exists (`-` is no wait), writes the remaining
//! arguments to the out file, one per line, then exits with the given code.

use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [out, code, gate, rest @ ..] = args.as_slice() else {
        eprintln!("usage: argv_probe <out-file> <exit-code> <gate-file|-> [args...]");
        std::process::exit(64);
    };
    if gate != "-" {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !std::path::Path::new(gate).exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    std::fs::write(out, rest.join("\n")).expect("write the out file");
    std::process::exit(code.parse().expect("a numeric exit code"));
}
