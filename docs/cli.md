# CLI reference

The `rdm` command-line tool downloads one URL per run. It needs no webview or
Node.js, and uses the same engine as the desktop app.

## Usage

```
rdm [OPTIONS] <URL>
```

| Argument | Description |
|---|---|
| `<url>` | The URL to download (required). |
| `-o, --output <path>` | Output file path. Defaults to a name derived from the URL or headers, saved in the current directory. |
| `--checksum <algo:hex>` | Verify the finished file against a checksum, e.g. `sha256:…`. |
| `-c, --connections <n>` | Max parallel connections, 1–16 (default 4; `-c 1` for a single connection). Starts on one and adds more, up to `n`, while measured speed keeps rising. Used only for files of 2 MiB or more on servers that support range requests. |
| `--limit-rate <rate>` | Cap the download speed in bytes/s, with an optional `K`/`M`/`G` suffix (×1024), e.g. `500K` or `2M`. Shared across all of the download's connections. |
| `-v, --verbose` | Also log informational engine events (how the download started, lanes added, resumes, the checksum result, where it was saved and a summary) to stderr. |
| `-h, --help` / `-V, --version` | Print help or the version. |

[How it works](how-it-works.md) explains connections, retries and checksums
in more detail.

## Resuming and Ctrl-C

Interrupted, or pressed Ctrl-C? Run the same command again to resume. Ctrl-C
saves progress, prints `Paused — run the same command again to resume`, and
exits; a second Ctrl-C quits at once.

To discard a partial download instead, delete its `<file>.part` and
`<file>.rdm`.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | The download finished (and its checksum matched, if given). |
| `1` | The download failed. |
| `2` | Invalid arguments (printed with the usage). |
| `130` | Paused by Ctrl-C; run the same command again to resume. |

## Output and logging

Retries and warnings print to stderr, one short line each, e.g.
`Lane 2 lost connection — retrying (attempt 2)`. A failed download is reported once, as
`Error: failed to download …`. `-v` adds informational events, one short line
each, such as `Started · 4.00 MB · resumable · 4 pieces, up to 4 lanes` and
`Done · 4.00 MB in 2.1s · avg 1.90 MB/s · peak 2.40 MB/s · no retries`.
`RUST_LOG` (e.g. `RUST_LOG=debug`) overrides the log level; `debug` adds each
request's byte range and the server's answer.
