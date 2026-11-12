# Using the GUI

The desktop app (Tauri + React) shows every download in one sortable table,
keeps the list across restarts, and has light and dark themes. To install it,
see [Installing rdm](install.md#desktop-app); to build it, see
[CONTRIBUTING.md](../CONTRIBUTING.md#building-the-desktop-gui).

## Contents

- [Adding downloads](#adding-downloads)
- [Finding and selecting](#finding-and-selecting)
- [The queue](#the-queue)
- [Lanes and details](#lanes-and-details)
- [Completed and failed downloads](#completed-and-failed-downloads)
- [Archive and tags](#archive-and-tags)
- [Settings](#settings)
- [Updates](#updates)
- [State](#state)
- [Logs and troubleshooting](#logs-and-troubleshooting)

## Adding downloads

Add downloads with the **Add** button, by dragging a link onto the window, or
by pressing Ctrl/Cmd+V (outside a text box) with one or more links on the
clipboard. The dialog takes one link per line and previews each file's name
and site, skipped lines and links already in your list or in your archive
before anything starts ("Add 3 downloads"); a link you've archived is still
added, so you can download it again. Ctrl+Enter adds. The folder starts at the
one you used last. **Tags** (comma-separated) go on every download you add.
Under **More options** you can give a SHA-256 checksum and a speed limit.

## Finding and selecting

- **Filter** with the sidebar (by status, destination folder or
  [tag](#archive-and-tags)) or by name from the toolbar. Click a column header to sort. The sidebar also shows the
  total speed with a graph of the last minute. In a window narrower than about
  760 px the sidebar shrinks to an icon rail (hover an icon for its name; the
  total speed puts its unit under the number) and less important columns hide.
- **Select** a row by clicking it; Ctrl+click adds or removes rows and
  Shift+click selects a range. With two or more selected, the toolbar offers
  the actions that apply to them all (Open, Pause, Resume, Stop, Retry,
  Tags…, Archive; in Archived, Unarchive and Remove).
- **Right-click** a row (or press Shift+F10 or the menu key) for every action
  that fits it, such as Show in folder, Copy link, Download again, Tags…,
  Delete partial file and Archive (in Archived, Unarchive and Remove).
- **Keyboard:** with the table focused, ↑/↓ move (Shift extends the
  selection), Home/End jump, Ctrl+A selects all, Space pauses or resumes,
  Enter opens a completed file, Delete archives (in Archived, removes) and
  Escape clears the selection.
- **Density:** the toolbar toggles compact 32 px rows (the default) and
  comfortable 48 px rows that also show each file's site.

## The queue

Downloads beyond the concurrency limit ([Settings](#settings); see
[how the queue works](how-it-works.md#the-queue-and-speed-limits)) wait, and
the first waiting one says **Next up**. To reorder, drag a waiting row by its
handle, press Alt+↑/Alt+↓, or use **Start next**, **Move up** and **Move
down** from its menu or the details pane.

You can pause, resume or stop any download, and choose whether a stop keeps
the partial file.

## Speed limits

The gauge button in a row sets or changes that download's own speed limit
(clicking its speed does the same); the status bar and Settings hold the
global limit for all downloads. While a download has its own limit, the gauge
turns orange and the Speed column reads the speed against the limit:

- **Downloading:** `49.8 / 50 KB/s`, the live speed with the limit in grey,
  or `– / 50 KB/s` until the first reading.
- **Paused or waiting:** `≤ 50 KB/s`.
- In a window narrower than about 760 px the limit shortens to the form you
  type in the editor: `49.8 / 50K`.

## Lanes and details

- **Lanes:** a multi-connection download draws one line per piece of the file
  in the Progress column, showing where the piece sits and how far it has got.
- **Details:** click a row to open the details pane under the table; it then
  follows the row you select (drag its top edge to resize).
  - **Overview** shows sizes, speed, time left, the lanes, the link, the
    folder and the checksum. A completed download gets a summary such as
    "5.8 GB in 2 min 14 s, average 44 MB/s, checksum verified" and **Verify
    checksum**, which checks the file against a SHA-256 you paste.
  - **The lanes** in Overview are one bar per open connection, top to
    bottom in lane order, headed with how many are open ("3 lanes"). A bar
    appears when rdm opens a lane and merges back up when it closes. Each
    shows only its own lane's work: the piece it is on now, strong, and the
    pieces it finished earlier, soft, so no part of the file is drawn twice.
    A closed lane's work moves to the top bar (Lane 1's); if it left a piece
    unfinished, the rest shows dashed until another lane takes it. Past four
    lanes the bars scroll in place, so the pane keeps its size. Hover a bar
    for which lane it is, its piece, where that sits and how far it has got
    ("Lane 3 · piece 5 · 19.1–23.8 MB · 40% downloaded"). Paused or stopped,
    the bars become one bar of the work so far; once the download completes,
    they give way to one finish bar and a line such as "Downloaded in 4
    pieces · up to 3 lanes".
  - **Speed** graphs the last 60 seconds, with the speed limit as a dashed
    line.
  - **Log** tells the download's story in short lines, updating while the
    tab is open: how it started (size, whether it can resume, how many
    pieces and lanes), lanes being added, closed or capped by the server, a
    lane losing its connection and reconnecting, pauses and where it resumed, rate-limit
    waits, the checksum result, where the file was saved, and a closing
    summary:

    ```
    Started · 6.25 GB · resumable · 4 pieces, up to 4 lanes
    Added Lane 2 — now 2 lanes
    Added Lane 3 — now 3 lanes
    Closing Lane 3 — more lanes didn't speed this up
    Lane 2 lost connection — retrying (attempt 2)
    Lane 2 reconnected
    Paused
    Resumed at 3.10 GB
    Checksum verified ✓
    Saved to /home/me/Downloads/big.iso
    Completed · 6.25 GB in 9m 59s (active 7m 14s) · avg 14.7 MB/s · peak 38.0 MB/s · 1 retry
    ```

    Switch from **Simple** to **Detailed** to also see each request (its
    byte range and the server's answer) and each lane picking up or taking
    over work, such as "Lane 2 finished piece 2, now on piece 5 of 8
    (3.10 GB – 3.90 GB)" or "Lane 3 took over the back half of Lane 1's
    piece 1 (400 MB – 780 MB) as piece 6". The choice is remembered. rdm
    keeps a download's last 200 lines, plus its last 300 detailed ones.

    **Copy** puts every line rdm keeps for the download on the clipboard,
    the detailed ones too whichever view is showing, one per line as
    `HH:MM:SS  text`; it says "Copied" for a moment (and tells a screen
    reader how many lines). **Save…** writes the same text to a file you
    pick, named after the download (`big.iso.log`), and says "Saved".
  - A finished download's lanes and log lines are kept only until the app
    closes; after a restart the pane shows the summary alone.

![The details pane's Speed tab for a running download: speed now, average and peak, and a graph of the last minute, under a table whose rows draw their lanes](screenshots/details.png)

## Completed and failed downloads

- **Completed** downloads stay where they are, with **Open** and
  **Show in folder**, until you [archive](#archive-and-tags) them.
- **Failed** downloads are pinned to the top with the reason in plain words and
  a **Retry** button.

## Archive and tags

- **Archive** moves a finished download (Completed, Failed or Stopped) out
  of the working views into **Archived** in the sidebar, which counts them;
  nothing is deleted, and it keeps its details and log. **Archive finished**
  in the toolbar archives every Completed and Stopped download at once, with
  **Undo** for a few seconds. Each finished row also has an Archive button,
  and Delete archives the selection.
- **Archived** lists only archived downloads. Select some and choose
  **Unarchive** to move them back, or **Remove** to drop them from rdm for
  good: the downloaded file, and a partial file kept by Stop, stay on disk,
  while a failed download's partial file is deleted. Remove asks first; tick
  **Don't ask again** to skip that, and turn **Ask before removing** back on
  in [Settings](#settings). If its downloads leave the archive while it asks
  (Undo on the archive toast, say), the dialog closes and nothing is removed.
  Retrying or resuming an archived download moves it back too, and **Retry
  all failed** leaves archived ones alone (it isn't offered in Archived).
- **Tags** label downloads (up to 16, each up to 32 characters; case doesn't
  make a different tag). Add them in the Add dialog, from **Tags…** in a row's
  menu or the selection toolbar, or in the details pane's Overview, where a
  tag's × takes it off. The sidebar's **Tags** section lists each tag with a
  count and filters by it. It follows the view: under Archived it lists the
  archived downloads' tags and narrows the archive ("Archived › work");
  elsewhere it lists only the working downloads' tags.

## Settings

**Settings** (the sliders button) holds the theme under **Display**, and
under **Downloads** the number of concurrent downloads, the global speed
limit, the max connections per download, the [proxy](#proxy) and whether
Remove asks first, and
under **Updates** this version, **Check for updates** and **Check for updates
automatically** (see [Updates](#updates)), and, last, **Troubleshooting**
(see [Logs and troubleshooting](#logs-and-troubleshooting)). The status bar
shows the total speed, and you can click the global limit there to change it.

### Proxy

**Proxy** sends downloads through an HTTP, HTTPS or SOCKS5 proxy: enter its
URL, e.g. `socks5h://127.0.0.1:1080` (`socks5h` has the proxy look up host
names too) or `http://user:pass@proxy.example:3128`. Leave it empty for no
proxy. The URL is checked when you press Enter or leave the field; an
invalid one shows an error under the field and isn't saved (closing Settings
first shows the error as a notice instead). A change applies
to downloads started or resumed after it, not to ones already running.

Once saved, the field shows the password as `••••••`. Leave the dots to keep
the saved password; it's kept only while the scheme, host, port and user
name stay the same, so the password never goes to a proxy you didn't enter
it for. To change any of those, type the password again. Logs and Copy
diagnostics never show the password (diagnostics name only the kind of
proxy, e.g. `proxy socks5h`), and only `state.json` keeps it.

Downloads use only this setting: the `HTTP_PROXY`, `HTTPS_PROXY` and
`ALL_PROXY` environment variables and the system's proxy settings (such as
Windows' Internet Options) don't apply to them. The in-app updater still
follows the system's proxy settings.

## Updates

The desktop app can update itself from the project's GitHub releases.

- **Asked once.** The first time it starts, a small card at the bottom left
  asks **Check for updates automatically?** Nothing is checked until you
  answer. With **Yes, check**, rdm checks right away, on every start, and
  once a day while it stays open (soon after waking, if the computer slept
  through it); with **No thanks**, it never checks on its own. Change it any
  time in Settings.
- **Check for updates** in Settings always checks, whatever that setting,
  and says how it went: you're up to date, a newer version is available, or
  why it couldn't check. A failed automatic check shows nothing; it's in the
  log.
- **The notice.** When a newer version is out, a bar under the toolbar names
  it, with **What's new** (its release page) and three choices:
  - **Restart now** downloads the update (with its progress), checks its
    signature and restarts into it. Downloads running or waiting pause, as on
    a normal quit, and the bar says how many first.
  - **Install when I quit** downloads and checks it now without interrupting
    anything, then installs it when you next quit; downloads keep going until
    then. **Restart now instead** is there if you change your mind (it, too,
    says which downloads will pause). If a newer version comes out before
    you quit, its bar says the staged one still installs at the quit unless
    you install the newer one.
  - **Later** hides the bar until the next check (the next start, or the next
    day).

  If the download fails or its signature doesn't check out, nothing changes:
  the bar says why, with **Try again** and **Dismiss**, and stays until you
  choose one (or a check finds a newer version). Installed from a
  `.deb`, rdm can't replace itself: the bar offers **Download**, which opens
  the release page, instead.
- **After the restart**, a bar says how many downloads the update paused
  ("Updated to rdm 0.3.1 · 2 downloads were paused for the update."). Nothing
  resumes on its own: **Resume all** resumes exactly those (not ones you had
  paused yourself), and **✕** leaves them paused. The bar also goes once
  you've resumed them all by hand.

Whether to check, and the version that last ran, are kept in
`~/.rdm/updater.json`.

## State

The download list and settings are stored in `~/.rdm/state.json`. A list saved
by an older rdm opens as is; one saved by a newer rdm is refused rather than
opened and saved back without what this version doesn't understand: a dialog
names its version, and rdm closes when you dismiss it, leaving the list
untouched. rdm 0.1.0 predates that check, so going back to it opens the list,
but its first save drops archive flags and tags. Tags edited into the file by
hand are brought within the tag rules when it opens. On Unix, `~/.rdm` is
made owner-only (0700), since the state file holds full download URLs.

`RDM_HOME` relocates `~/.rdm`, and `RDM_DOWNLOAD_DIR` changes the Add dialog's
default folder.

## Logs and troubleshooting

The GUI writes a daily log in `~/.rdm` (`rdm.YYYY-MM-DD.log`, the last 7
kept), with its level set by `RUST_LOG` (default `info`); the Log tab always
shows informational lines and up (and, with **Detailed**, rdm's debug lines
too), whatever `RUST_LOG` says. Each run's log starts with the version, OS,
WebView, memory use and the settings that matter (no URLs, paths or tags).
Every crash is recorded there with its details, and so is any error the
window itself hits; see
[When something goes wrong](how-it-works.md#when-something-goes-wrong).

Logged URLs and error messages hide a link's secrets but not where it
points: a login shows only as `<redacted>:<redacted>@` (or `<redacted>@`
for a user name alone), so you can still tell one was used, a query string
becomes `?<redacted>`, and a fragment is dropped. Site names and file paths
stay visible. Only `state.json` keeps download URLs, and the proxy's login,
in full.

**Settings → Troubleshooting** has two buttons:

- **Open logs folder** opens `~/.rdm` in your file manager (hover it to see
  the path).
- **Copy diagnostics** puts a report on the clipboard for a bug report: the
  version, OS, WebView, memory use and settings lines, the log folder and
  the last 300 lines of the log. It hides more than the log does, because
  it's the text you post:
  - a link keeps only its scheme and login marker, with its site and path
    replaced: `https://<redacted>:<redacted>@<site-1>/<path>?<redacted>`
    (the same site gets the same number throughout the report);
  - the file names of the downloads in your list show as `<file-1>.iso`,
    keeping the extension;
  - their folders and the default download folder show as `<folder-1>`,
    and your home folder as `~`.

  Other text in the log lines stays as it is, including the names of
  downloads you've since removed from the list, so still read the report
  before you post it. If a developer needs more, the full log is in the logs
  folder.

If an action hits a bug in rdm, it shows an error instead of closing the
app. If the window itself fails to draw, it shows **Something went wrong**
instead of going blank. Your downloads keep running. **Reload** redraws the
window, **Open logs folder** and **Copy diagnostics** work as above (the
copied report includes the error), and **Show details** shows the error's
text.

To report a bug, open a
[bug report](https://github.com/SimonPerryman/rust-download-manager/issues/new?template=bug_report.yml)
on GitHub (the window's "new issue on GitHub" link opens it) and paste the
diagnostics into it.
