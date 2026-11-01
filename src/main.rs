//! `rdm` — the command-line frontend. It parses arguments, renders progress,
//! and delegates everything else to the engine. No download logic lives here.

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};

use rdm::{download, engine::state, Checksum, Control, DownloadRequest};

/// A download manager in Rust.
#[derive(Parser, Debug)]
#[command(name = "rdm", version, about)]
struct Cli {
    /// URL to download.
    url: String,

    /// Output file path (default: a name derived from the URL/headers, saved
    /// in the current directory).
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// Verify the finished file against a checksum, e.g.
    /// `--checksum sha256:<hex>`.
    #[arg(long = "checksum", value_parser = parse_checksum)]
    checksum: Option<Checksum>,
}

/// clap value parser: `sha256:<hex>` → [`Checksum`].
fn parse_checksum(s: &str) -> Result<Checksum, String> {
    s.parse()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut req = match cli.output {
        Some(out) => DownloadRequest::to_path(cli.url, out),
        None => DownloadRequest::new(cli.url, std::env::current_dir()?),
    };
    if let Some(checksum) = cli.checksum {
        req = req.with_checksum(checksum);
    }

    // When the target is known up front (explicit -o), note whether a prior
    // partial exists so we can tell the user we're resuming. The engine does
    // the actual resume off the `.part` + sidecar; this is only a message.
    if let Some(out) = &req.out {
        let part = state::part_path(out);
        let sidecar = state::sidecar_path(out);
        if part.exists() && sidecar.exists() {
            println!("Resuming {}", out.display());
        }
    }

    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template(
            "{bar:40.cyan/blue} {bytes}/{total_bytes} ({binary_bytes_per_sec}, eta {eta})",
        )
        .expect("valid template")
        .progress_chars("=>-"),
    );

    let control = Control::new();
    let summary = download(&req, &control, |p| {
        if let Some(total) = p.total {
            bar.set_length(total);
        }
        bar.set_position(p.downloaded);
    })
    .await
    .with_context(|| format!("failed to download {}", req.url))?;

    bar.finish_and_clear();
    println!(
        "Downloaded {} bytes to {}",
        summary.bytes,
        summary.path.display()
    );
    Ok(())
}
