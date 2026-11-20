# CLI reference

The `rdm` command-line tool downloads one URL per run, or many links as a batch. It needs no webview or
Node.js, and uses the same engine as the desktop app.

## Usage

```
rdm [OPTIONS] <URL>...
rdm [OPTIONS] -i <FILE>
```

| Argument | Description |
|---|---|
| `<url>` | The URL to download. Give several to download them as a [batch](#many-links-at-once). Required unless `-i` is used. |
| `-i, --input-file <file>` | Download every link in this file, or on standard input with `-i -`. See [Many links at once](#many-links-at-once). |
| `-j, --jobs <n>` | How many links of a batch download at once, 1-32 (default 3). |
| `-o, --output <path>` | Output file path (one link only). Defaults to a name derived from the URL or headers, saved in the current directory. |
| `--checksum <algo:hex>` | (One link only.) Verify the finished file against a checksum, e.g. `sha256:…`. |
| `-c, --connections <n>` | Max parallel connections, 1–16 (default 4; `-c 1` for a single connection). Starts on one and adds more, up to `n`, while measured speed keeps rising. Used only for files of 2 MiB or more on servers that support range requests. |
| `--limit-rate <rate>` | Cap the download speed in bytes/s, with an optional `K`/`M`/`G` suffix (×1024), e.g. `500K` or `2M`. Shared across all of the download's connections; in a batch it applies to each file. |
| `--at <time>` | Wait until this local time, then download: `HH:MM` is the next such time (tomorrow's if it has passed today), or give a full `YYYY-MM-DDTHH:MM`. rdm must stay running; Ctrl-C while waiting cancels. Without it the download starts at once. With a batch, the whole batch waits. See [Scheduling](#scheduling). |
| `--on-finish <sleep\|shutdown>` | After every download has finished, wait 60 seconds (Ctrl-C cancels), then put the machine to `sleep` or `shutdown`. Not after a failure or a Ctrl-C. See [Scheduling](#scheduling). |
| `--proxy <url>` | Download through this proxy instead of the system's: `http://`, `https://`, `socks5://` or `socks5h://` (host names resolved by the proxy), with an optional `user:pass@`, e.g. `socks5h://127.0.0.1:1080`. See [Proxies](#proxies). |
| `--no-proxy` | Connect directly, ignoring the system's proxy. Can't be combined with `--proxy`. |
| `-v, --verbose` | Also log informational engine events (how the download started, lanes added, resumes, the checksum result, where it was saved and a summary) to stderr. |
| `-h, --help` / `-V, --version` | Print help or the version. |

[How it works](how-it-works.md) explains connections, retries and checksums
in more detail.

## Many links at once

Give several links, or a file of them, and rdm downloads them into the
current directory, up to `-j` at a time (3 by default):

```sh
rdm -i urls.txt
rdm -i urls.txt https://example.com/extra.zip -j 5
rdm https://example.com/a.zip https://example.com/b.zip
cat urls.txt | rdm -i -
```

In a file, put links on separate lines or separate them with spaces. Blank
lines and lines starting with `#` are ignored, a link listed twice is
downloaded once, and anything that isn't an `http` or `https` link is
skipped. A list can hold up to 5000 different links.

A `[a-b]` range in a link's path or query stands for every value between,
so one line can name a whole numbered set. Quote it so the shell leaves the
brackets alone:

```sh
rdm 'https://example.com/lesson[01-12].mp4'
echo 'https://example.com/img[a-f].png' | rdm -i -
```

- Numbers keep their zero padding: `[01-12]` gives `01` to `12`. If either
  end has a leading zero, both must have the same number of digits
  (`[01-015]` is refused). Single letters of one case work too (`[a-f]`).
- One range per link, in the path or query and never the host, counting up,
  making at most 500 links. A refused range is reported as `Skipped <link>:
  <why>` (with how many links it would make, when it is too big) and the rest
  of the list still downloads.
- Ranges expand before repeats are removed, so `lesson[01-15]` with
  `lesson[13-17]` is 17 links.
- Only `[x-y]` with letters or digits is read as a range. Other brackets
  (an IPv6 host like `http://[::1]/`, `?filter[name]=a`) stay as written;
  write `%5B` for a literal bracket that would look like a range.
- rdm can't know which numbers exist: every link is tried, and a missing
  one fails on its own line.

rdm prints one line per file as it finishes, then a summary such as
`8 done, 1 failed, 2 skipped lines`. A file that fails never stops the
others; the exit code is `1` if any failed. Two links that would save under
the same name get `name (1).ext`. Named batches, group rows, Retry failed and a
batch-wide speed limit are [GUI features](gui.md#batches); the CLI applies
`--limit-rate` to each file.

`-o` and `--checksum` name one file, so using them with `-i` or several
links is a usage error. Ctrl-C pauses every running file; run the same
command again to resume each from where it stopped (files that had already
finished are downloaded again, under a new name).

## Scheduling

```sh
rdm --at 01:30 https://example.com/big.iso
rdm --at 2026-12-01T22:00 -i urls.txt --on-finish shutdown
```

`--at` waits in the foreground, so rdm has to stay running until the time
comes; Ctrl-C while waiting prints `Cancelled`, downloads nothing and exits `130`. A
time that isn't `HH:MM` or `YYYY-MM-DDTHH:MM`, or an action other than
`sleep` or `shutdown`, is a usage error (exit `2`).

`--on-finish` counts down 60 seconds after the last download finishes and
then asks the operating system to sleep or shut down; Ctrl-C during the
countdown cancels it (exit `130`). It does nothing if any download failed or
a Ctrl-C paused the run. If the machine refuses, rdm prints an error and
exits `1`.

A download window, a speed schedule, running a command after each download
and rules for a tag, host or batch are [GUI features](gui.md#schedules): the
CLI runs once and exits, so there is no rule set for them to attach to.

## Proxies

By default rdm uses the system's proxy:

- the `HTTP_PROXY` (for `http://` links), `HTTPS_PROXY` (for `https://`
  links) or `ALL_PROXY` environment variable, lowercase names too, skipping
  the hosts listed in `NO_PROXY` (e.g. `NO_PROXY=localhost,.internal.example`);
- if none of those is set, on Windows and macOS the proxy set in the
  operating system's settings (Windows: Settings → Network & internet →
  Proxy → Manual proxy setup), with its bypass list.

Proxy auto-config (a PAC script or WPAD, "Automatically detect settings")
isn't supported: with only that set up, rdm connects directly. Use
`--proxy` to name the proxy instead, or `--no-proxy` to connect directly
whatever the system says.

`--proxy` sends every request of the download through an HTTP, HTTPS or
SOCKS5 proxy, e.g. one behind a corporate firewall or the SOCKS5 endpoint a
VPN service offers. Use `socks5h://` rather than `socks5://` to have the
proxy look up the host name too, so no DNS lookup leaves your machine
directly. A SOCKS5 proxy without a port uses 1080.

```sh
rdm https://example.com/big-file.zip --proxy socks5h://user:pass@127.0.0.1:1080
```

`--proxy` applies to every host; `NO_PROXY` doesn't apply to it. An
invalid proxy URL is reported before
anything is downloaded (exit code 2). The proxy's login never appears in
rdm's output: `-v` shows the proxy as `Through proxy
socks5h://<redacted>:<redacted>@127.0.0.1:1080`.

## Resuming and Ctrl-C

Interrupted, or pressed Ctrl-C? Run the same command again to resume. Ctrl-C
saves progress, prints `Paused — run the same command again to resume`, and
exits; a second Ctrl-C quits at once.

To discard a partial download instead, delete its `<file>.part` and
`<file>.rdm`.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | The download finished (and its checksum matched, if given); for a batch, every file finished. |
| `1` | The download failed; for a batch, at least one file failed. Also when `--on-finish` is refused by the machine. |
| `2` | Invalid arguments (printed with the usage). |
| `101` | rdm hit a bug (a panic); see [Troubleshooting](#troubleshooting-and-bug-reports). |
| `130` | Paused by Ctrl-C; run the same command again to resume. Also Ctrl-C while `--at` waits (nothing was started) or during the `--on-finish` countdown. |

## Output and logging

Retries and warnings print to stderr, one short line each, e.g.
`Lane 2 lost connection — retrying (attempt 2)`. A failed download is reported once, as
`Error: failed to download …`. `-v` adds informational events, one short line
each, such as `Started · 4.00 MB · resumable · 4 pieces, up to 4 lanes` and
`Completed · 4.00 MB in 2.1s · avg 1.90 MB/s · peak 2.40 MB/s · no retries`.
`RUST_LOG` (e.g. `RUST_LOG=debug`) overrides the log level; `debug` adds each
request's byte range and the server's answer. With `-v`, the first line names
the version and the OS, e.g. `rdm 0.2.0 · Ubuntu 24.04 LTS (Linux 6.8.0) x86_64`.

## Troubleshooting and bug reports

If rdm itself hits a bug (a panic), it prints the panic's message, then
`rdm <version> hit a bug. Please report it at …/issues …`, and exits with
code 101. A bug in a background step (such as the checksum check) adds a
final `Error: failed to download …: internal error, please report it` line. To report it, open a
[bug report](https://github.com/SimonPerryman/rust-download-manager/issues/new?template=bug_report.yml)
on GitHub with:

- the output of the same command rerun with `-v` (or with `RUST_LOG=debug`
  for every request's detail);
- the version (`rdm --version`) and your OS.

Check the output for anything private first: the log redacts the URLs it
mentions (a login shows only as `<redacted>:<redacted>@`, or `<redacted>@`
for a user name alone, and a query string as `?<redacted>`, but the site
name and path stay), and the command line you typed and the file names are
as you gave them. `RUST_LOG=debug` also shows the panic's backtrace.
