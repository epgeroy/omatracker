# TimeTracker

TimeTracker is an Omarchy bar widget backed by a native Rust CLI. Quickshell
only presents JSON returned by the CLI and submits commands to it; the CLI owns
the ledger, atomic writes, report snapshots, Typst rendering, and Drive uploads.

## Plugin installation

```bash
omarchy plugin add https://github.com/epgeroy/omatracker --enable
```

The plugin loader clones files but does not build Rust projects or run install
hooks. Releases must therefore include an executable
`bin/time-tracker` for the intended Linux architecture. The widget resolves and
runs that plugin-local binary; it never relies on a similarly named program in
`PATH`.

The current development target is `x86_64-unknown-linux-gnu`. A release for a
different architecture needs a separately compiled `bin/time-tracker`.

## Development build

Rust is only required when building the backend. Build the plugin-local binary
and run all checks with:

```bash
make backend
make check
```

`make backend` produces `bin/time-tracker`, which is the executable that must
be included when publishing a plugin release.

## CLI

The CLI keeps data in `~/.config/omarchy/time-tracker.json` by default. Pass
`--data-path <file>` to use another ledger, which is also how the panel honors
the widget's `dataPath` setting.

```bash
bin/time-tracker status --json
bin/time-tracker project create "Client A"
bin/time-tracker project select <project-id>
bin/time-tracker task add "Design"
bin/time-tracker task start <task-id>
bin/time-tracker task stop <task-id>
bin/time-tracker task edit <task-id> --add 1h30m
bin/time-tracker report export weekly
bin/time-tracker report check
bin/time-tracker report retry
bin/time-tracker sync
bin/time-tracker service install
```

All mutations acquire an advisory lock and replace the JSON ledger atomically.
The CLI migrates the version 1 task list into the `Unassigned` project on its
first write. Historical cumulative totals are retained as undated legacy time,
so they never appear in a date-based report.

`bin/time-tracker service install` creates and enables a persistent user-level
systemd timer, which runs `report check` every 15 minutes even when Quickshell
is closed. Use `bin/time-tracker service remove` to disable and delete it. The
panel exposes the same background-check setting and also requests `report
check` while it is running; the CLI is the sole authority for which reports are
due.

## Typst and Google Drive

Time tracking has no external runtime dependency. PDF exports and Drive uploads
are opt-in:

```bash
sudo pacman -S typst rclone
```

- **Typst** is invoked as `typst compile` against the bundled local templates.
- **rclone** is invoked only as `rclone copyto --checksum`; TimeTracker never
  runs destructive remote synchronization or deletes remote files.
- rclone owns Google OAuth tokens. TimeTracker stores neither OAuth credentials
  nor API secrets.

Configure a `drive` remote independently, ideally with rclone's `drive.file`
scope, then set the remote and folder through the panel or:

```bash
bin/time-tracker drive update --remote time-tracker --folder TimeTracker --sync-on-startup true
```

Report snapshots, generated Typst sources, and PDFs live in
`~/.cache/omarchy/time-tracker/` until uploaded. Reports are immutable snapshots
of the project metadata, selected template, and time entries at queue time.

## Quickshell IPC

```bash
omarchy-shell time-tracker open
omarchy-shell time-tracker add
omarchy-shell time-tracker total
omarchy-shell time-tracker sync
omarchy-shell time-tracker exportWeekly
omarchy-shell time-tracker exportMonthly
```

## License

MIT

## Legal

- [Privacy Policy](PRIVACY_POLICY.md)
- [Terms of Service](TERMS_OF_SERVICE.md)
