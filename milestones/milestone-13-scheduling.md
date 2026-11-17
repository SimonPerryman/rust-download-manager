---
id: M13
title: Scheduling & automation
status: todo
depends-on: [M4, M5, M12]
tags: [engine, cli, gui, testing]
---

## Goal
Let downloads run when the user wants: start at a set time, stay inside a
daily time window, follow a bandwidth schedule, and do something when they
finish.

## Context / rationale
Today everything runs the moment it's added, at one global speed limit. The
usual reasons to want otherwise are an overnight window, a metered or shared
connection that is fast only at night, and a PC that should shut itself off
once a big queue is done. The queue and limiter already exist (M4), so this
is a time-aware layer on top, not a new scheduler. Roadmap index:
[[roadmap]].

## Decisions (workers follow these; escalate only if one can't hold)
- **Time rules live in the engine** (`engine::schedule`) as pure functions
  of a rule set and a point in time. The Manager asks them; the CLI and GUI
  only edit and display rules. The engine takes an injected clock, so every
  test runs on a fake clock and never sleeps for real.
- **Local wall-clock time.** Rules are "HH:MM" plus days of the week in the
  machine's local zone. A window may cross midnight (22:00–06:00). Across a
  clock change the window follows the wall clock, so it is an hour shorter
  or longer that night; no special casing.
- **Three rule kinds, one rule set** (global settings, saved with them):
  1. *Queue window*: downloads start only inside the window. Recurring,
     with an end. When it closes, a running download pauses (not an error)
     and resumes when the window opens, using the existing sidecar state.
     A download whose server can't resume is let finish instead, since
     pausing would throw its bytes away. A window can be told to only stop
     *starting* downloads and let running ones finish.
  2. *Speed schedule*: a list of (window, limit) entries. Inside an entry
     its limit replaces the global limit; outside, the global limit applies.
     It feeds the existing limiter, so a change takes effect mid-download.
  3. *Start at*: a one-off time on any selection of downloads or on a whole
     add or batch ("start all of these at 22:00"). No end: once started
     they run as long as they take. Until then each shows as Scheduled and
     doesn't use a queue slot. It is set where the downloads are (a "Start
     at" field under More options in the Add dialog, and a row's context
     menu), not in the schedule form.
- **Days.** A window or speed rule runs on every day, weekdays, weekends or
  a chosen set of days. The data is a set of days; "Daily", "Weekdays" and
  "Weekends" are only names for common sets.
- **Missed times.** Rdm only schedules while it is running; it never wakes
  the machine or installs a service (backlog). On launch a queue window or
  speed entry just applies. A *start at* that passed while rdm was closed
  does not fire silently. Its rows show "Was due Mon 22:00", and one
  non-blocking notice (also shown on the main list at launch, so it isn't
  hidden in a view nobody opens) leads with schedules and gives downloads
  in brackets: "3 scheduled starts were missed (50 downloads)". The
  sidebar entry counts schedules (3), never downloads. A batch with one
  start time is one missed schedule and one row. Buttons:
  - *Start all* and *Start now*: the downloads join the normal queue,
    oldest due first, under the usual cap. Nothing floods the machine.
  - *Reschedule*: a menu, time first, with the real time each choice
    lands on (`Tue 22:00 Tonight`, `Wed 22:00 Tomorrow`,
    `24 Nov 22:00 Next week`, then Pick a time…).
  - *Dismiss*: removes the schedule only. The downloads stay in the list
    as ordinary waiting downloads, so nothing in this flow loses one.
  It persists until dealt with (a toast would vanish unseen).
- **Events, not callbacks.** The engine publishes a download-finished and
  a queue-emptied event on a broadcast channel (the way missing-file and
  find changes already are). Post-download actions are subscribers; the
  engine doesn't know what they do.
- **Post-download actions** are global, off by default, and are not
  schedules, so they have their own section ("When downloads finish") and
  not an entry in New schedule. Two rows:
  - *Run a command after each download*: an executable and argument list
    with `{path}` and `{name}` filled in. Never through a shell, and the
    form says it runs with the user's rights. A non-zero exit is shown as
    a warning on the row, not a failed download.
  - *Shut down when everything is done*: its row menu picks shut down,
    sleep or hibernate. A 60 s countdown with Cancel.
  - **Order.** A finish runs its command first. The power action fires
    only when nothing is running, queued, paused by a window or scheduled
    for later, and after every command has exited, so a scan is never cut
    off by a shutdown. A pending scheduled start blocks it (otherwise
    "done" at 08:00 would kill tomorrow's 03:00 start); the row says what
    it is waiting for.
  - Open-the-folder on finish is not in this milestone.
  - Both are global. A command or shutdown that belongs to one batch or
    schedule is a later extension (backlog).
- **One Schedules view** (sidebar, under Automation), mock-up kept locally
  in `.ai-cache/mockup-schedules.html`. Top to bottom, in the order a user
  acts on it:
  - a one-line status ("Downloading. The window closes at 08:00, then a
    5 MB/s cap applies"), with the missed notice above it when there is
    one;
  - *Coming up*: scheduled downloads, each "Starts today 22:00" or "Starts
    Thu 03:00" (weekday and clock, never "tomorrow"), with an Edit pencil
    (icon only, tooltip) and a *Start now* button (words only);
  - *Rules*: one row each with a switch and a ··· menu (Edit, Duplicate,
    Delete). A switched-off rule fades its text, not its switch, and keeps
    its settings. No "Active now" label: the switch and status line say so;
  - *When downloads finish*: the two rows above, on the same switch column.
  The sidebar entry shows a count only when something was missed. There is
  no day strip or timeline; it needed a legend and repeated the rule rows.
- **The New schedule form** is one modal, opened by one "New schedule"
  button, sentence case, a 20px heading like the Add dialog's and a close
  button. Fields, top to bottom:
  - *Type*: a select, Download window or Speed limit; the fields below
    follow it. Not a tab-style switch.
  - *Name*: optional; a greyed auto-name (such as "Overnight only") is used
    when left empty.
  - *Limit* (Speed limit only): a number and a unit, MB/s.
  - *Times*: typed `22:00` to `08:00`. A range that ends earlier than it
    starts runs overnight; there is no "(next day)" label.
  - *Days*: a select with Daily, Weekdays, Weekends and Choose days.
    The seven chips (Mon…Sun, three letters, a gap before Sat) show only
    under Choose days, starting from the days already picked. The select's
    text is derived from the set: all seven reads Daily, Mon–Fri
    Weekdays, Sat and Sun Weekends, anything else lists the days ("Mon,
    Tue and Thu"). There is no "Custom". With no day picked, Add is
    disabled and the footer says "Pick at least one day".
  - *Finish downloads in progress* (Download window only): a switch, off
    by default. Off means running downloads pause when the window ends.
  - The footer holds a plain-sentence summary beside Cancel and Add:
    "Downloads run from 22:00 to 08:00 every day, and pause at 08:00."
    "Downloads are limited to 5 MB/s from 08:00 to 18:00 on weekdays."
    With the switch on: "New downloads start from 22:00 to 08:00 every
    day. Running ones finish." Add is the only filled button.
  - *Applies to* (all downloads, a tag, a host or a batch) arrives with
    the scope slice and isn't in the first version.
- **Scheduled state is visible.** A row waiting for its time shows
  "Scheduled for 01:00", and a closed queue window shows once in the status
  bar ("Queue paused until 22:00"), not per row.
- **CLI form:** `rdm --at 01:30 <URL>` waits in the foreground, then
  downloads; `--on-finish <sleep|shutdown>` runs after the run. A window or
  speed schedule is GUI-only for now: the CLI runs once and exits, so a
  persistent rule set has nothing to attach to.
- **Auto-organise by type** is not built here; it overlaps the
  archive-and-tag automation in the roadmap backlog and belongs with it.

## Scope
**In:** `engine::schedule` and the injected clock; queue window; speed
schedule; per-download start time; post-download actions; settings UI,
status-bar and row states; CLI `--at` and `--on-finish`; docs.

**Out (deferred):**
- Waking the machine, running as a service or starting with the OS (→ "GUI
  desktop integration" in the roadmap backlog).
- Auto-organise by file type (→ "Archive and tag automation").
- Calendar rules beyond days of the week ("last Friday of the month").
- Time zones other than the machine's.
- Free-text entry ("weekdays 8 to 6, 5 MB/s"): a small rule-based parser
  over the same form (no AI), always showing what it understood and never
  saving by itself. A form-first design leaves room for it.
- A week grid where hours are painted per day, for different times on
  different days within one rule.
- A command or shutdown attached to one batch or schedule ("when this
  batch finishes").

## Tasks
Each is its own PR, in this order.
- [ ] `schedule-engine`: `engine::schedule` (rule types, window and
      speed-entry evaluation, midnight crossing, DST), injected clock,
      settings schema, unit tests on the fake clock
- [ ] `schedule-window`: Manager honours the queue window (pause at close,
      resume at open, launch-time evaluation), status-bar state, the
      Schedules view with its rule list, and the New schedule form for
      Download window
- [ ] `schedule-speed`: speed schedule feeding the limiter, mid-download
      change, Speed limit in the New schedule form, status line
- [ ] `schedule-start-at`: per-download and per-add start time, Scheduled
      row state, Coming up section, Missed notice with Reschedule menu,
      CLI `--at`
- [ ] `schedule-actions`: finish and queue-empty actions, power countdown
      with Cancel, run-a-command, CLI `--on-finish`
- [ ] `schedule-scope` (needs the owner's go-ahead): a window or speed
      entry can apply to all downloads, a tag, a host or a batch, so
      "this tag only overnight" works; settings UI, tests
- [ ] `schedule-docs`: `docs/gui.md`, `docs/cli.md`,
      `docs/how-it-works.md`, README highlight, screenshots

## Acceptance criteria
Binary pass/fail, each mapping to something an agent can run.
- [ ] `cargo test` passes, with offline fixture tests on a fake clock for:
      a window that crosses midnight; a window on selected weekdays; a
      download running at close pausing (not failing) and resuming at open,
      with bytes kept; a speed entry replacing the global limit inside it
      and restoring it outside, mid-download; a *start at* holding a
      download out of the queue slots until its time (asserted on Manager
      status, or a `/gated` fixture, not progress bytes); a past *start at*
      becoming Missed at launch and not starting by itself, with Start all
      feeding the queue under its cap; a 23- and a 25-hour DST day.
- [ ] The days set names itself (all seven Daily, Mon–Fri Weekdays, Sat
      and Sun Weekends, else the listed days) and an empty set is refused
      (GUI unit test).
- [ ] A power action is dropped when anything is queued, paused by a window
      or scheduled for later, waits for a running command to exit, and
      Cancel stops it during the countdown (tests use a fake power backend; no
      test ever sleeps or shuts the machine down).
- [ ] *Run a command* fills `{path}` and `{name}`, passes arguments without
      a shell (a name containing `;` or a space arrives intact), and a
      non-zero exit warns without failing the download.
- [ ] `rdm --at <time> <URL>` against the fixture server waits, downloads
      and exits 0; `--on-finish` with an unknown action is a usage error
      (`tests/cli.rs`).
- [ ] A webview e2e spec covers setting a queue window, a *Scheduled* row,
      the status-bar state, and the Schedules view listing the rule, the
      scheduled download and a missed one, with a rule switched off and on,
      and creating a rule through the New schedule form.
- [ ] `docs/cli.md`, `docs/gui.md` and `docs/how-it-works.md` describe the
      behaviour, the README highlights mention it, and the screenshots are
      regenerated.
