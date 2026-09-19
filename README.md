# OmaTracker

OmaTracker is an Omarchy bar widget backed by a native Rust CLI. Quickshell
only presents JSON returned by the CLI and submits commands to it; the CLI owns
the ledger, atomic writes, invoice snapshots, Typst rendering, and Drive uploads.

## Agent-first invoicing

Install the CLI from this repository first:

```sh
make install
omatracker skill install --harness opencode
```

`make install` builds and installs for the current user, without sudo. The command
is linked at `~/.local/bin/omatracker`; the independent binary and templates live
under `${XDG_DATA_HOME:-~/.local/share}/omatracker`. Ensure `~/.local/bin` is on
your `PATH`. Prebuilt release users can run `make install-bin` without Rust.
Rerun the installation target after updating OmaTracker, and rerun `skill install`
to refresh global skill documentation. `BINDIR` and `DATADIR` can override the
installation directories.

If you use the Omarchy widget too, deploy both from this checkout:

```sh
make install-plugin
omarchy restart shell
```

This updates the installed widget's runtime files, backs up its previous files,
and links its plugin-local backend to the same installed executable used by the
CLI. Later `make install` updates that shared backend for both consumers. Rerun
`make install-plugin` for QML/widget changes. `make install-plugin-bin` uses the
prebuilt binary; `PLUGIN_DIR` and `PLUGIN_BACKUP_DIR` override deployment paths.
The defaults are `~/.config/omarchy/plugins/epgeroy.omatracker` and
`${XDG_STATE_HOME:-~/.local/state}/omatracker/plugin-backups`. Existing unrelated
plugin files and git metadata are retained. New widget installs can be enabled
with `omarchy plugin enable epgeroy.omatracker`.
Restart the shell after deploying QML changes; a plugin rescan can leave previously
loaded QML components cached in the running process.

Do not use an old widget backend against a newer ledger: version 0.4 cannot
preserve invoice metadata or project/client archive markers. The widget polls
the ledger every five seconds, but polling an outdated backend will still show
incorrect results. Inspect the actual widget connection with
`omarchy-shell omatracker status` (backend path, ledger path, cached entity IDs),
and request an immediate read with `omarchy-shell omatracker refresh`.

Use the local versioned JSON interface and the bundled skill; no MCP server is
needed:

```sh
bin/omatracker agent help
bin/omatracker agent doctor
bin/omatracker agent context
bin/omatracker agent migration.preview
```

See [Agent API](AGENT_API.md) for project/client setup, historical rates, dated
entries and reversible corrections, invoice issuance, and Drive operations.
Install [the OmaTracker skill](skills/omatracker/SKILL.md) globally for your harness:

```sh
omatracker skill install --harness opencode
omatracker skill install --harness claude,codex
omatracker skill targets
```

With no `--harness`, installation uses the shared `~/.agents/skills/omatracker`
directory. Supported harnesses are `opencode`, `claude`, `codex`, `gemini`, and
`cursor`; `shared` selects the cross-harness location. Installations include all
reference docs and the absolute executable path, so they work from other projects.
Use `--dry-run` to preview paths or `--json` for structured output. Repeating the
command updates an unmodified managed installation; customized/existing skills
require `--force`, which preserves a backup outside the skill discovery directory.
Quit and restart OpenCode after installing; restart/reload skills in other harnesses.

Remove a global skill with the same harness selection (`uninstall` is an alias):

```sh
omatracker skill remove --harness opencode
omatracker skill remove --harness claude,codex --dry-run
```

Removal supports `--json`, is a no-op if already absent, and leaves the CLI and
tracking data installed. Modified or unmanaged skill directories require `--force`,
which moves them to a backup outside the skill discovery directory. With no
`--harness`, it removes the shared copy; that affects every harness loading that
shared directory. Other installed copies can still be discovered by your harness.
Restart/reload the harness after removal (quit and restart OpenCode).

The [manual invoice walkthrough](tests/manual-invoices.md) covers an isolated
end-to-end setup including branding and Google Drive testing.

Rename tasks, projects, and clients with `agent task.update`, `agent project.update`,
and `agent client.update` using a `name` field. Each also has `.remove` and `.delete`
operations. Task deletion preserves dated time; projects and clients are archived
to preserve billing history. See the [entity lifecycle reference](AGENT_API.md#renaming-and-deleting-entities)
for examples and client reassignment rules.

Tasks can also have their own hourly rate, including tasks created without one:

```sh
omatracker agent task.rate --input '{"id":"TASK_ID","rate":"50","currency":"USD"}' --key auto
# Add "applyExisting":true to explicitly price existing unrated, uninvoiced time.
```

The widget's task editor exposes the same rate settings and opt-in backfill.
Use `entityRevision` from task/project/client get/list responses for concurrency
checks; it isn't invalidated by unrelated ledger changes. Generate a fresh key with
`agent request.key` for each new operation, or use `--key auto` interactively.
Recreating deleted entities needs a new creation key and a new ID.

Billability follows the rate at the time of work: a configured rate (including
zero) is billable; no rate is non-billable. New projects default to monthly invoice
drafts. Drafts are issued and uploaded explicitly. Invoice numbering, exact totals,
duplicate-billing protection, payment status, and immutable PDFs are owned by Rust.

First write upgrades old ledgers to version 4. A version 3 ledger gets a sibling
`.pre-task-rates.bak`; older ledgers get `.pre-invoices.bak`. Existing dated time
from pre-invoice ledgers needs an explicit historical-rate or
non-billable decision before invoicing; old PDFs remain archived reports. Issued
artifacts live in `<ledger>.invoices/`. Back up the ledger, invoice directory, and
custom template library together.
Version 4 protects task-rate metadata: older 0.5 backends reject it instead of
rewriting data they do not understand. Update the shared CLI/widget backend together.

## Clear all user data

`Unassigned` is an internal fallback workspace (`project-unassigned`), not a
permission restriction. Normal deletion cannot remove that reserved workspace.
The clear-all command resets it to an empty default and reports it separately from
user projects. It permanently removes active **and archived** user records,
including tasks, dated time, corrections, draft/issued invoice records and reports.

Preview the exact scope:

```sh
omatracker data clear --dry-run
omatracker data clear --include-drive --dry-run --json
```

For a local reset:

```sh
omatracker data clear
```

To include Drive, run this **instead**, while the original ledger still identifies
its uploaded files:

```sh
omatracker data clear --include-drive
```

The command creates a backup under `<ledger>.backups/clear-…` first. It preserves
reusable templates, preferences, the issuer profile, Drive configuration and
invoice-number counters. Generated invoice documents (including orphaned local
bundles) and tracked report artifacts are removed from active storage. Previously
created backups and unrelated cache files remain. Drive cleanup deletes only
identified invoice/report files and a verified matching `state.json`; it never
purges the remote folder. See [clear-all details](AGENT_API.md#clear-all-and-the-protected-workspace).

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
invoices, PDF templates, running timers across projects, and preferences/Drive.

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
Preview playback displays the selected output and volume, then completion or an
error. The player follows the system default audio output when it changes.

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
bin/omatracker agent invoice.period --input '{"project":"PROJECT_ID","cadence":"monthly"}'
bin/omatracker report check
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
is the sole authority for which invoice drafts are due. `report check` now creates
drafts only, including when called by an existing timer; it never issues or uploads.
Monthly/weekly/manual cadence is exclusive per project. Late work in an issued
period can produce a supplemental draft; existing drafts refresh explicitly.

The panel uses `status --json --compact`, which calculates task totals in one
aggregation pass and omits historical entries and reports. Dependency and timer
diagnostics are checked separately at startup, every 15 minutes, and after timer
settings change. The original full `status --json` response remains available.

Invoices, uploads, and diagnostics run in a separate command queue so starting
and stopping timers stays responsive. Ledger uploads use an immutable temporary
snapshot, allowing tracking to continue during a transfer.

## Project hourly rates

**Invoice amounts use historical entry rates.** The counter estimate described
below is a legacy presentation estimate at the current rate, not an invoice total.
The old `report export`/`report retry` commands remain explicit archive operations;
`report archive-check` maintains legacy reports. Use `agent invoice.*` for billing.

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
- **rclone** handles uploads with `copyto --checksum`. Explicit
  `data clear --include-drive` additionally inventories, backs up, and deletes
  identified tracker files with `deletefile`; it never purges a remote directory.
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
