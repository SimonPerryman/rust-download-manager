# Using the GUI

The desktop app (Tauri + React) shows every download in one sortable table,
keeps the list across restarts, and has light and dark themes. To install it,
see [Installing rdm](install.md#desktop-app); to build it, see
[CONTRIBUTING.md](../CONTRIBUTING.md#building-the-desktop-gui).

## Contents

- [Adding downloads](#adding-downloads)
- [Batches](#batches)
- [Finding and selecting](#finding-and-selecting)
- [The queue](#the-queue)
- [Speed limits](#speed-limits)
- [Schedules](#schedules)
- [Lanes and details](#lanes-and-details)
- [Completed and failed downloads](#completed-and-failed-downloads)
- [Missing files](#missing-files)
- [Archive and tags](#archive-and-tags)
- [Settings](#settings)
- [Updates](#updates)
- [State](#state)
- [Logs and troubleshooting](#logs-and-troubleshooting)

## Adding downloads

Add downloads with the **Add** button, by dragging a link onto the window, or
by pressing Ctrl/Cmd+V (outside a text box) with one or more links on the
clipboard. The dialog takes one link per line and previews each file's name
and site, and links already in your list or in your archive, before anything
starts ("Add 3 downloads"); a link you've archived is still
added, so you can download it again. Ctrl+Enter adds. The folder starts at the
one you used last. **Tags** (comma-separated) go on every download you add.
Under **More options** you can give a SHA-256 checksum (for a single link) and
a speed limit (per file, when there are several links), and **Start at**: type
a time such as `22:00` (or `930`, `9.30`, `9:30 pm`) and the links wait until
then (the field says which day it means). Everything added together with one start time is one schedule.

A `[a-b]` range in a link, such as `lesson[01-12].mp4` or `img[a-f].png`,
adds every file in it. The preview shows one row ("lesson01.mp4 to
lesson12.mp4", 12 links) that opens to list its files. A range that isn't
valid, or makes more than 500 links, is explained under the box and left
out. **N more** lists all the rest of a range's files (the preview scrolls),
and becomes **Show less**; the range's chevron closes it too. The
[CLI](cli.md#many-links-at-once) takes the same ranges and `-i` list files. A link already in your list shows an info
mark instead of a tick; it is still added.

Beside "N links found", a quiet **N skipped** counts lines that aren't links
and repeats; click it for the reasons. The lines themselves aren't listed.

## Batches

Two or more links become a **batch** when you give them a **Batch name**. A
range fills the name in from the text before it (`lesson[01-12].mp4` gives
"lesson"); clearing it makes them ordinary downloads again (the name is
limited to 64 characters). With a name, a ticked **Create folder** box puts
the files in a folder of that name inside the destination; unticked, they go
straight into it. A folder name that is taken gets `(1)` after it, so a second
"Course files" goes in `Course files (1)`, and files that clash inside a
folder become `name (1).ext`.

![The Add dialog for a batch: 16 links found with a range row, the Batch name, the Create folder tick, and under More options the speed limit per file and for the batch](screenshots/batch-add-limits.png)

Under More options, **Speed limit per file** applies to each file, and
**Speed limit for the batch** (shown only with a name) caps all the batch's
files combined. Both can be set; the slower one wins, and the
[global limit](#speed-limits) still caps everything. The batch limit is
kept, so Retry failed and a restart keep it. Leave a field empty for no limit.

![The list grouped by batch: a Course files row with a progress bar, 14 of 16 done, 2 failed and Retry failed, above its files, then the Photos batch](screenshots/batch-grouped-list.png)

- **Group row:** in the list, a batch's files sit under one row with a
  progress bar and, right after it, a count ("9 of 14 done"). Its chevron opens and closes the
  files, and its **Show in folder** button opens the folder they went into. While any file has failed the row shows how many, and **Retry
  failed** retries those files of that batch only.
- **Ungrouped:** with grouping on, downloads in no batch close the list under
  an **Ungrouped** header with a plain count ("6 downloads") and the same
  chevron, so they don't read as part of the last batch. It shows only while a
  batch is listed and something is outside one.
- **Group batches:** a toolbar toggle beside the density button, shown once a
  batch exists, turns grouping off for a flat list. It is remembered.
- **Batches** in the sidebar lists each batch with its file count and filters
  to it; the toolbar then shows the batch's progress and Retry failed. A batch
  is fixed when it is added, unlike [tags](#archive-and-tags), which you edit.

## Finding and selecting

- **Filter** with the sidebar (by status, destination folder,
  [batch](#batches) or [tag](#archive-and-tags)) or by name from the toolbar. Click a column header to sort. While anything transfers,
  the sidebar also shows the total speed with a graph of the last minute. In a window narrower than about
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
the first queued one says **Next up**. A download held by a closed
[queue window](#schedules) says what it waits for instead: **Queued until
22:00**, with the weekday when that isn't today. A window scoped to a tag,
host or batch marks only the downloads it matches. To reorder, drag a queued row by its
handle, press Alt+↑/Alt+↓, or use **Start next**, **Move up** and **Move
down** from its menu or the details pane.

A queued row shows its size (and the file's real name) before it starts, when
the server tells. If it doesn't, the size stays blank until the download runs.

You can pause, resume or stop any download, and choose whether a stop keeps
the partial file.

## Speed limits

The gauge button in a row sets or changes that download's own speed limit
(clicking its speed does the same); the status bar and Settings hold the
global limit for all downloads. While a download has its own limit, the gauge
turns orange and the Speed column reads the speed against the limit:

- **Downloading:** `49.8 / 50 KB/s`, the live speed with the limit in grey,
  or `– / 50 KB/s` until the first reading.
- **Paused or queued:** `≤ 50 KB/s`.
- In a window narrower than about 760 px the limit shortens to the form you
  type in the editor: `49.8 / 50K`.

## Schedules

**Schedules** in the sidebar (under Automation) holds the rules for when
downloads run. One line at the top says where the queue stands ("Queue paused.
The window opens at 22:00."), and the list below has one row per rule.

![The Schedules view: a download window, a tag-scoped window and a weekday speed limit under Rules, two scheduled downloads under Coming up, and the When downloads finish section](screenshots/schedules.png)

A **download window** is the span of the day when downloads run, on every day,
weekdays, weekends or days you choose. It may cross midnight (22:00 to 08:00).
Outside it the queue is paused: nothing new starts, and the status bar says
**Queue paused until 22:00** once (a weekday comes first when it isn't today),
not on every row. When the window closes, running downloads pause rather than
fail, keeping what they have, and they continue when it opens. A download
whose server can't resume is left to finish, since pausing would throw its
bytes away. With **Finish downloads in progress** on, the window only stops
new downloads starting. The window follows the wall clock, so it is an hour
shorter or longer on the night the clocks change. Rules apply only while rdm
is running; it never wakes the computer.

**New schedule** opens a form: an optional name, the times typed as `22:00`
to `08:00`, the days, and the switch above. A time may be typed loosely, such
as `930`, `0930`, `9:30`, `9.30`, `9` (9:00) or `9:30 pm`; it tidies to
`HH:MM` when you leave the box. Times in sentences, rows and the status line
follow the computer's 12 or 24-hour setting ("10:00 PM" or "22:00"). The footer says in a sentence what
the rule does. **Days** offers Daily, Weekdays, Weekends and Choose days; the
day buttons appear under Choose days, and with none picked **Add** stays off.
Each rule has a switch (a switched-off rule keeps its settings) and a ···
menu with Edit, Duplicate and Delete. With several rules on, the queue is open
whenever any of them is.

A **speed limit** is a rule of the same shape with a cap: pick **Speed limit**
as the Type, type the limit in KB/s or MB/s (the unit is beside the box), and
set times and days. Inside its times
the cap replaces the global speed limit; outside them the global limit
applies. It takes effect on downloads already running, with no restart, and it
doesn't touch a limit set on one download or batch. Where entries overlap, the
first one switched on in the list wins. Rows read like "5 MB/s, 08:00–18:00,
weekdays", and the line at the top adds the cap in force or the next one
("A 5 MB/s cap applies until 18:00."). The type of an existing rule can't be
changed; make a new one.

**Applies to** points a rule at all downloads (the default), a tag, a host or a
batch. For a tag, type it or pick one from the list; a host is the name in the
download's address, such as `cdn.example.com`, matched exactly and ignoring
case; a batch is picked from the batches you have. A download window for a tag
holds and pauses only downloads carrying it, so "this tag only overnight"
works. A speed limit for a tag, host or batch caps those downloads together,
on top of the global limit, and the slower cap wins. A rule whose tag or batch
nothing carries any more is kept and applies to nothing. The status bar's
paused note and the top line speak for rules that apply to all downloads;
with only scoped rules the line says so.

### Start at

A **start time** holds downloads out of the queue until a time you pick,
without using a slot. Set it under **More options** in the Add dialog, or with
**Start at…** in a row's menu (for waiting, paused or failed downloads). Such a
row reads "Scheduled for 01:00" (with the weekday when it isn't today). When
the time comes the downloads join the queue like any other, under the usual
limit on how many run at once. **Coming up** in Schedules lists each schedule
as "Starts today 22:00" or "Starts Thu 03:00", with an **Edit** pencil to
change the time and **Start now** to send it into the queue at once.

rdm only schedules while it is running. A start time that passed while rdm
was closed does not fire by itself: its rows read "Was due Mon 22:00" and a
notice such as "3 scheduled starts were missed (50 downloads)" shows on the
main list and in Schedules, until you deal with it, and the sidebar entry
counts the missed schedules. **Start all** (or **Start now** on one) sends the
downloads into the queue, oldest due first, under the usual limit. **Reschedule**
offers real times (Tue 22:00 Tonight, Wed 22:00 Tomorrow, then a week on) and
**Pick a time…**. **Dismiss** drops the schedules and leaves their downloads
in the list, paused, to resume when you want.

### When downloads finish

The last section of Schedules, **When downloads finish**, has two actions.
Both are global, off by default, and not schedules, so they sit apart from the
rules. Each is one line: a title, a chip that shows and changes the action, and
a switch on the same column as the rules' switches.

**After each download** starts a program of your choosing with the finished
file. Its chip shows the command line (cut with an ellipsis if long; hover for
the whole line), or "No command"; each variable shows as a tinted token. Press the chip, or switch the row on with no
command yet, to open **Edit command**: the **Program** and its **Arguments**,
one per line. `{download_path}` (the finished file, with its folder) and
`{download_name}` (just its name) are filled in, and nothing else is. The
**Insert** buttons under Arguments add either one at the cursor: there is no `%VAR%` expansion, because the program is
started directly and never through a shell, so a name with a space or a `;`
arrives as one argument. It inherits rdm's own environment and runs with your
rights. The Program and Arguments are saved as typed in the settings file, so
don't type secrets there; put them in the script, or in an environment
variable the program reads itself. If it can't start or exits with an error,
the download stays completed and its Progress cell shows an amber warning icon
beside **Done** (hover it for "Command failed" and the reason).

**After all downloads** puts the computer to sleep, hibernates it or shuts it
down. Its chip shows which; press it to pick. **Sleep** is low power and wakes
in seconds; **Hibernate** saves to disk and powers off completely. When
nothing is left, a notice on every page counts down 60 seconds ("Shutting down
in 42 s") with **Cancel**; cancelling stops that one countdown and leaves the
action on. Nothing counts as done while a download is running, waiting for a
slot, held by the download window, or scheduled to start later (a pending
**Start at** blocks it), nor while a command is still running, so a scan is
never cut off. The row says which of these it is waiting for. A download that
fails counts as finished; one you pause doesn't hold it back.

## Lanes and details

- **Lanes:** a multi-connection download draws one line per piece of the file
  in the Progress column, showing where the piece sits and how far it has got.
- **Details:** click a row to open the details pane under the table; it then
  follows the row you select, including a selection that moves there from\n  elsewhere, and stays on its row while that row is still selected or has just\n  left the view (drag its top edge to resize).
  - **Overview** shows sizes, speed, time left, the lanes, the link, the
    folder and the checksum. A completed download gets a summary such as
    "5.8 GB in 2 min 14 s, average 44 MB/s"; the checksum fact beside it offers
    **Verify checksum**, which checks the file against a SHA-256 you paste.
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
  - A finished download's lanes are kept only until the app closes, and
    so are its detailed log lines; after a restart the pane shows the
    summary and the Log tab's informational lines, which rdm keeps on disk
    (see [how it works](how-it-works.md#a-downloads-log-on-disk)).

![The details pane's Speed tab for a running download: speed now, average and peak, and a graph of the last minute, under a table whose rows draw their lanes](screenshots/details.png)

## Completed and failed downloads

- **Completed** downloads stay where they are, with **Open** and
  **Show in folder**, until you [archive](#archive-and-tags) them.
- **Failed** downloads are pinned to the top with the reason in plain words and
  a **Retry** button.

## Missing files

When a finished download's file has been moved or deleted, its row dims and
shows **File missing** with a warning icon in place of its size and speed. The
download stays in the list as a record, and it counts under both **Completed**
and the sidebar's **Missing** view, which shows how many there are. Open and
Show in folder give way to **Locate…** on the row.

Select a missing row and the details pane shows a banner with the old path
and three actions:

- **Locate…** opens a file picker. Pick the file and the row relinks to it. A
  file of a different size is refused ("That file is a different size.").
- **Find file** searches nearby folders for it (below). Also in the row's
  right-click menu.
- **Remove from list** drops the record; the file is already gone.

With two or more missing rows selected, right-click offers **Locate folder…**
to pick the folder they all moved to.

### Find file

Find file shows its progress and the folder being searched, with **Cancel**.
It then reports one of:

- **Match found:** one file, with its folder, size and date. **Relink** accepts
  it; **Locate…** picks a different file instead.
- **Possible matches:** several files, each marked with why it may be the one
  (**Same name and size**, **Possible match: renamed** or **Checksum match**).
  Choose one and **Relink**, or **Skip**.
- **No match found:** the folders searched, with **Locate…**,
  **Search again** and **Remove from list**.

Nothing is relinked until you click Relink. When a search was cut short, a
quiet "Not every folder was fully searched" appears under the result. Each
search is recorded in the download's [Log](#logs-and-troubleshooting), with
the folders searched and the outcome. For what is searched and how files are
matched, see [how it works](how-it-works.md#missing-and-moved-files).

### Find files

In the Missing view the toolbar has **Find files**, which searches for every
missing file in one pass. A bar shows its progress, with **Cancel**. The result
opens as a review: each file with where it was, where it was found and why it
matches, plus a **Locate…** link in case the suggestion is wrong. Same-name,
same-size files start ticked; renamed ones start unticked. **Relink selected**
relinks the ticked rows.

### Same-folder offer

After you relink a file with Locate…, a bar offers to search the folder it
moved to for the other missing files that were in the same old folder.
**Search** runs Find files on just those; the X dismisses the bar.

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

**Proxy** chooses how downloads connect:

- **Use system proxy** (the default): the operating system's proxy settings
  on Windows and macOS (Windows: Settings → Network & internet → Proxy →
  Manual proxy setup), with their bypass list, and on every OS the
  `HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY` environment variables, which
  take precedence, with the hosts in `NO_PROXY` going direct. Proxy
  auto-config (a PAC script or WPAD, "Automatically detect settings") isn't
  supported: with only that set up, downloads connect directly.
- **No proxy:** always connect directly.
- **Custom proxy:** an HTTP, HTTPS or SOCKS5 proxy you enter in the field
  that appears, e.g. `socks5h://127.0.0.1:1080` (`socks5h` has the proxy
  look up host names too) or `http://user:pass@proxy.example:3128`, for
  every host. The URL is checked when you press Enter or leave the field;
  an invalid one shows an error under the field and isn't saved (closing
  Settings first shows the error as a notice instead).

A change applies to downloads started or resumed after it, not to ones
already running. The in-app updater always uses the system proxy.

Once saved, the custom proxy's field shows the password as `••••••`. Leave
the dots to keep the saved password; it's kept only while the scheme, host,
port and user name stay the same, so the password never goes to a proxy you
didn't enter it for. To change any of those, type the password again. Logs
and Copy diagnostics never show the password (diagnostics name only the
kind of proxy: `proxy system`, `proxy none`, or the scheme, e.g. `proxy
socks5h`), and only `state.json` keeps it.

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
by an older rdm opens as is (its downloads simply belong to no batch); one saved by a newer rdm is refused rather than
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
