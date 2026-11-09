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
- [Finished and failed downloads](#finished-and-failed-downloads)
- [Settings](#settings)
- [State and logs](#state-and-logs)

## Adding downloads

Add downloads with the **Add** button, by dragging a link onto the window, or
by pressing Ctrl/Cmd+V (outside a text box) with one or more links on the
clipboard. The dialog takes one link per line and previews each file's name
and site, skipped lines and links already in your list before anything starts
("Add 3 downloads"); Ctrl+Enter adds. The folder starts at the one you used
last. Under **More options** you can give a SHA-256 checksum and a speed
limit.

## Finding and selecting

- **Filter** with the sidebar (by status, or by destination folder) or by name
  from the toolbar. Click a column header to sort. The sidebar also shows the
  total speed with a graph of the last minute. In a window narrower than about
  760 px the sidebar shrinks to an icon rail (hover an icon for its name) and
  less important columns hide.
- **Select** a row by clicking it; Ctrl+click adds or removes rows and
  Shift+click selects a range. With two or more selected, the toolbar offers
  the actions that apply to them all (Open, Pause, Resume, Stop, Retry, Remove
  from list).
- **Right-click** a row (or press Shift+F10 or the menu key) for every action
  that fits it, such as Show in folder, Copy link, Download again, Delete
  partial file and Remove from list.
- **Keyboard:** with the table focused, ↑/↓ move (Shift extends the
  selection), Home/End jump, Ctrl+A selects all, Space pauses or resumes,
  Enter opens a finished file, Delete removes from the list and Escape clears
  the selection.
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

## Lanes and details

- **Lanes:** a multi-connection download draws one line per piece of the file
  in the Progress column, showing where the piece sits and how far it has got.
- **Details:** click a row to open the details pane under the table; it then
  follows the row you select (drag its top edge to resize).
  - **Overview** shows sizes, speed, time left, the lanes full size (hover one
    for details), the link, the folder and the checksum. A finished download
    gets a summary such as "5.8 GB in 2 min 14 s, average 44 MB/s, checksum
    verified" and **Verify checksum**, which checks the file against a SHA-256
    you paste.
  - **Speed** graphs the last 60 seconds, with the speed limit as a dashed
    line.
  - **Log** tells the download's story in short lines, updating while the
    tab is open: how it started (size, whether it can resume, how many
    lanes), lanes being added or capped by the server, a lane losing its
    connection and reconnecting, pauses and where it resumed, rate-limit
    waits, the checksum result, where the file was saved, and a closing
    summary:

    ```
    Started · 6.25 GB · resumable · up to 4 lanes
    Ramped to 2 lanes
    Lane 2 lost connection — retrying (attempt 2)
    Lane 2 reconnected
    Paused
    Resumed at 3.10 GB
    Checksum verified ✓
    Saved to /home/me/Downloads/big.iso
    Done · 6.25 GB in 9m 59s (active 7m 14s) · avg 14.7 MB/s · peak 38.0 MB/s · 1 retry
    ```

    Tick **Detailed** to also see each request (its byte range and the
    server's answer) and each lane picking up or taking over work. The
    choice is remembered. rdm keeps a download's last 200 lines, plus its
    last 300 detailed ones.
  - A finished download's lanes and log lines are kept only until the app
    closes; after a restart the pane shows the summary alone.

![The details pane's Speed tab for a running download: speed now, average and peak, and a graph of the last minute, under a table whose rows draw their lanes](screenshots/details.png)

## Finished and failed downloads

- **Finished** downloads stay where they are, marked Done, with **Open** and
  **Show in folder**. **Clear finished** removes Done and Stopped rows from the
  list (with Undo); downloaded files stay on disk.
- **Failed** downloads are pinned to the top with the reason in plain words and
  a **Retry** button.

## Settings

**Settings** (the sliders button) holds the theme, the max number of
concurrent downloads, the global speed limit and the max connections per
download. The status bar shows the total speed, and you can click the global
limit there to change it.

## State and logs

The download list and settings are stored in `~/.rdm/state.json`. The GUI also
writes a daily log in `~/.rdm` (`rdm.YYYY-MM-DD.log`, the last 7 kept), with
its level set by `RUST_LOG` (default `info`); the Log tab always shows
informational lines and up (and, with **Detailed**, rdm's debug lines too),
whatever `RUST_LOG` says. On
Unix, `~/.rdm` is made owner-only (0700), since the state file and logs hold
full download URLs.

Logged URLs and error messages drop any `user:password@` and fragment, and
replace a query string with `?<redacted>`, so they're safer to paste into a
bug report.

`RDM_HOME` relocates `~/.rdm`, and `RDM_DOWNLOAD_DIR` changes the Add dialog's
default folder.
