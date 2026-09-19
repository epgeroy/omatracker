# OmaTracker

OmaTracker is an Omarchy bar widget backed by a native Rust CLI. Quickshell
only presents JSON returned by the CLI and submits commands to it; the CLI owns
the ledger, atomic writes, report snapshots, Typst rendering, and Drive uploads.

## Plugin installation

```bash
omarchy plugin add https://github.com/epgeroy/omatracker --enable
```

## Breaking Rename

OmaTracker is a new plugin identity: `epgeroy.omatracker`. Existing
`sophie.time-tracker` installations must be removed and the new plugin enabled
in the preferred bar section:

```bash
omarchy plugin remove sophie.time-tracker
omarchy plugin enable epgeroy.omatracker --section right --after omarchy.tray
```

The new default ledger is `~/.config/omarchy/omatracker.json`. Copy the old
ledger to that path before enabling OmaTracker if you want to retain its data:

```bash
cp ~/.config/omarchy/time-tracker.json ~/.config/omarchy/omatracker.json
```

The plugin loader clones files but does not build Rust projects or run install
hooks. Releases must therefore include an executable
`bin/omatracker` for the intended Linux architecture. The widget resolves and
runs that plugin-local binary; it never relies on a similarly named program in
`PATH`.

The current development target is `x86_64-unknown-linux-gnu`. A release for a
different architecture needs a separately compiled `bin/omatracker`.

## Development build

Rust is only required when building the backend. Build the plugin-local binary
and run all checks with:

```bash
make backend
make check
```

`make backend` produces `bin/omatracker`, which is the executable that must
be included when publishing a plugin release.

## Focus-card interface

The main view keeps the active timer, task list, project total and rate estimate
in view. **Menu** (or `Ctrl+K`) opens searchable commands for project settings,
reports, PDF templates, running timers across projects, and preferences/Drive.

- `j/k` or arrows navigate; `Space`/`Enter` activate the visible selection.
- `n` creates a named task; `e` edits; `h/l` or left/right reveal task actions.
- `p` switches projects, `/` finds tasks, `,` opens preferences, `?` shows help.
- `Esc` backs out one level. In forms, Tab/Shift+Tab traverse controls; in the
  main browse view they retain Omarchy's next/previous-panel behavior.
- Reset/delete actions require an explicit confirmation. Recorded time remains
  available for reports.

The optional wooden click sounds once per **hour of tracked work across projects**.
Overlapping timers count once; pauses, manual additions and undated legacy time
do not count. Counter resets do not reset the hourly cadence. Partial hours
survive restarts, and gaps longer than 30 seconds in notification polling suppress
missed sounds. Milestones are checked every 10 seconds while the shell runs,
including with the popup closed. No sound plays while the shell is stopped.

Preferences include click volume, a Preview button and Reduced motion (for the
widget's added transitions; Omarchy owns the outer popup fade). Preferences and
the atomic cross-panel notification checkpoint live in `<dataPath>.feedback.json`,
separate from the uploaded ledger. Audio uses Qt Multimedia and the bundled
original `sounds/wood-click.wav`; regenerate it with `python tests/generate-click.py`.

Try the UI using disposable data, including a click in about 20 seconds:

```bash
make backend
python tests/ui-check.py --preview --hour-demo
```

See [manual focus-card testing](tests/manual-focus-card.md) for the full checklist.

## CLI

The CLI keeps data in `~/.config/omarchy/omatracker.json` by default. Pass
`--data-path <file>` to use another ledger, which is also how the panel honors
the widget's `dataPath` setting.

```bash
bin/omatracker status --json
bin/omatracker status --json --compact
bin/omatracker diagnostics
bin/omatracker project create "Client A"
bin/omatracker project select <project-id>
bin/omatracker task add "Design"
bin/omatracker task start <task-id>
bin/omatracker task stop <task-id>
bin/omatracker task edit <task-id> --add 1h30m
bin/omatracker report export weekly
bin/omatracker report check
bin/omatracker report retry
bin/omatracker sync
bin/omatracker service install
```

All mutations acquire an advisory lock and replace the JSON ledger atomically
when state actually changes. No-op commands and idle report checks do not
rewrite the ledger.
The CLI migrates the version 1 task list into the `Unassigned` project on its
first write. Historical cumulative totals are retained as undated legacy time,
so they never appear in a date-based report.

`bin/omatracker service install` creates and enables a persistent user-level
systemd timer, which runs `report check` every 15 minutes even when Quickshell
is closed. Use `bin/omatracker service remove` to disable and delete it. The
panel exposes the same background-check setting and requests `report check`
while it is running if there is no active systemd timer for its ledger. The CLI
is the sole authority for which reports are due. Checks derive completed,
occupied periods from entries, including histories longer than three years and
late entries in previously empty periods.

The panel uses `status --json --compact`, which calculates task totals in one
aggregation pass and omits historical entries and reports. Dependency and timer
diagnostics are checked separately at startup, every 15 minutes, and after timer
settings change. The original full `status --json` response remains available.

Reports, uploads, and diagnostics run in a separate command queue so starting
and stopping timers stays responsive. Ledger uploads use an immutable temporary
snapshot, allowing tracking to continue during a transfer.

## Project hourly rates

In project settings, enter an **Hourly rate** and **Currency**, then save.
Leave the rate empty and save to remove it; `0` is a valid rate. The panel shows
the selected project's rate and estimated amount, including live timer time.

```bash
bin/omatracker project update <project-id> --hourly-rate 80.00 --currency USD
bin/omatracker project update <project-id> --hourly-rate 100.00
bin/omatracker project update <project-id> --clear-rate
```

The currency is required when first setting a rate. Subsequent rate updates
can reuse it. To change currencies, supply both the rate and currency; there
is no exchange-rate conversion. Currency codes are case-insensitive.

- **2 decimal places:** USD, EUR, GBP, CAD, AUD, NZD, CHF, CNY, INR, BRL, MXN,
  ARS, COP, PEN, ZAR, NGN, EGP, KES, SEK, NOK, DKK, PLN, CZK, HUF, RON, TRY,
  UAH, RUB, ILS, AED, SAR, QAR, SGD, HKD, TWD, THB, MYR, IDR, PHP, PKR, BDT.
- **0 decimal places:** JPY, KRW, CLP, VND.
- **3 decimal places:** BHD, KWD, OMR, TND.

Use a dot for decimals and no thousands separators. Negative rates, unsupported
currencies, excess decimal places, and rates over 1,000,000,000 minor units
(USD 10,000,000.00/hour, for example) are rejected without changing the ledger.

Amounts are `rate × seconds / 3600`, rounded half-up once at the total to the
currency's minor unit. A rate of USD 80/hour and 1h30m gives USD 120.00. Amounts
are estimates, without taxes or invoicing. Different projects' currencies are
never summed together.

The panel follows its visible time counters, including undated legacy time.
Resetting a counter or deleting a task reduces the panel estimate but retains
dated entries for reports. Weekly/monthly reports use only entries in that
period, excluding undated legacy time. Both PDF templates show the rate and
estimated amount when configured; older snapshots remain time-only.

Changing a rate recalculates the panel estimate and affects reports queued
after the change, including reports for past periods. Already queued reports
keep their original rate, currency, and amount, including on retry. Re-exporting
an already queued period does not replace its snapshot. Existing ledgers load
without rates until configured.

See [manual rate testing](tests/manual-rates.md) for panel, CLI, and PDF checks.

## Typst and Google Drive

Time tracking has no external runtime dependency. PDF exports and Drive uploads
are opt-in:

```bash
sudo pacman -S typst rclone
```

- **Typst** is invoked as `typst compile` against captured local templates.
  Use the bundled layouts or create your own through **Menu → PDF templates and
  appearance**. See [Custom PDF templates](TEMPLATES.md) for editing, CLI commands,
  the data contract, and a manual testing walkthrough.
- **rclone** is invoked only as `rclone copyto --checksum`; OmaTracker never
  runs destructive remote synchronization or deletes remote files.
- rclone owns Google OAuth tokens. OmaTracker stores neither OAuth credentials
  nor API secrets.

Configure a `drive` remote independently, ideally with rclone's `drive.file`
scope, then set the remote and folder through the panel or:

```bash
bin/omatracker drive update --remote omatracker --folder OmaTracker --sync-on-startup true
```

Report snapshots, generated Typst sources, and PDFs live in
`~/.cache/omarchy/omatracker/` until uploaded. Reports are immutable snapshots
of the project metadata, selected template source, local assets, logo, and time
entries at queue time. Template edits affect newly queued reports; retries reuse
the captured bundle. Existing rendered reports keep their PDFs. Older unrendered
reports capture the available template on their next render.
Upload retries reuse a successfully rendered PDF; a missing PDF is rendered
again. A per-ledger worker lock prevents a retry from resetting an export that
another process is still handling.

`make check` includes backend regression tests, an isolated QML concurrency
check using a fake backend, and rate-service checks against the rebuilt CLI in
a disposable home. It does not run the panel service against your ledger.

## Quickshell IPC

```bash
omarchy-shell omatracker open
omarchy-shell omatracker add
omarchy-shell omatracker total
omarchy-shell omatracker sync
omarchy-shell omatracker exportWeekly
omarchy-shell omatracker exportMonthly
```

## License

MIT

## Legal

- [Privacy Policy](PRIVACY_POLICY.md)
- [Terms of Service](TERMS_OF_SERVICE.md)
