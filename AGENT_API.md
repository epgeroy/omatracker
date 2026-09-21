# Agent API and invoicing

OmaTracker's agent interface is a local, noninteractive CLI. It needs no MCP server.

## Quick index

| Intent | Read |
| --- | --- |
| Create/start a task | [Quick start and targeting](#quick-start-task-and-time), [task fields](#time-and-corrections) |
| Add historical time | [Dated work and billing checks](#add-dated-work), [entry fields](#time-and-corrections) |
| Discover/edit entities | [Discovery rules](#discovery-rules), [projects](#discovery-setup-and-projects), [rename/delete](#renaming-and-deleting-entities) |
| Prepare, issue, render, upload | [Invoices](#invoices) |
| Customize/show a template | [Templates and Drive](#templates-and-drive), [invoice template contract](TEMPLATES.md#invoice-contract-version-1) |
| Install/remove | [Skill installation](#global-skill-installation), [skill removal](#global-skill-removal) |
| Reset/migrate | [Clear-all](#clear-all-and-the-protected-workspace), [migration and storage](#migration-and-storage) |

## Quick start: task and time

For an installed skill, read its `references/installation.md` once per
session/installation context for the exact executable. Use that absolute path
in place of `bin/omatracker` below; resolve documentation relative to the document,
not the working directory. Recheck the installation reference after a reinstall
or executable move. No `agent help` call is required for these documented requests.

### Discovery rules

- Reuse an explicit project ID already returned by an authoritative command.
- Otherwise choose **one** initial read: `repository.resolve` with an existing
  repository path for a binding, `project.list` for enumeration, or `context` if
  running timers/draft summaries are needed. Do not automatically fetch both
  `context` and `project.list`.
- Lists return `data.items`, `data.total`, and `data.nextOffset`. Follow `nextOffset`
  with the same filters; a partial page does not establish absence. Project/client
  lists hide archives unless `includeArchived: true`. Resolve ambiguous names with
  the user. `context` includes active projects, not archived projects or entity tokens.
- Fetch additional fields only when needed: `project.get` returns `data.billing`
  (timezone, client ID, rate history); get/list responses provide `entityRevision`
  for entity edits. Refresh metadata after a conflict or meaningful intervening
  change, rather than before every operation.
- Prepare independent reads and retry keys together. Order writes that depend on
  returned IDs. Use explicit project/task IDs; panel selection is never a target.

### Create a task and start tracking

Generate and retain two distinct keys with `agent request.key` (each returns
`data.key`). These reads and project discovery are independent and may run together:

```sh
bin/omatracker agent project.list --input '{"limit":50}'
bin/omatracker agent request.key
bin/omatracker agent request.key
```

If the target project ID is already known, skip discovery. Replace `PROJECT_ID`,
`CREATE_KEY`, and `START_KEY` with the returned values. Run the writes in order:

```sh
bin/omatracker agent task.create --input '{"project":"PROJECT_ID","title":"demo"}' --key CREATE_KEY
# Use data.id from the successful creation response as TASK_ID:
bin/omatracker agent task.start --input '{"id":"TASK_ID"}' --key START_KEY
```

Check `ok` on each response and `data.running: true` on start. No `task.list` or
revision lookup is needed for a new task. Retain the exact inputs and resolved
keys; retry a lost response with the same key, never another creation.
Full [task fields](#time-and-corrections) and [retry conventions](#agent-requests)
are below. This scenario needs only the installation lookup and this section.

### Add dated work

Resolve “yesterday” in the project's timezone. If it is not already known, read
`project.get` for `data.billing.timezone`; identify the task with a scoped
`task.list` only if its ID is unknown. Ask for a missing/ambiguous task or start
time rather than inventing a date, timezone offset, or interval.

For an agreed three-hour interval beginning at 09:00 UTC on 2026-09-18:

```sh
bin/omatracker agent request.key
# Replace ENTRY_KEY with data.key and TASK_ID with the authoritative task ID:
bin/omatracker agent entry.add --input '{"id":"TASK_ID","start":"2026-09-18T09:00:00+00:00","seconds":10800,"note":"Yesterday’s work"}' --key ENTRY_KEY
```

Replace the example date/offset with the user's actual interval. The end must be
no later than now. `entry.add` records completed work; it does not start a timer.
Use [entry fields](#time-and-corrections) for end timestamps and corrections.

Check every item in `data.entries`: `entry` holds the recorded segment and
`billing` its captured historical pricing. A rate boundary can split one request
into several entries. Missing rate is non-billable; zero is a billable rate.
Today's rate does not prove yesterday was priced. Report recorded duration and
actual billing status; investigate unexpected exclusions before invoicing.
If an amount is needed, use `summary` for the project/date range (exclusive `to`)
and report its per-currency totals and exclusions rather than computing money.
Pricing existing unrated work requires explicit intent; see
[task-rate adjustments](#assign-a-rate-to-an-existing-task).

## Conventions

```sh
bin/omatracker --data-path /absolute/path/ledger.json agent help
bin/omatracker agent ACTION --input '{"field":"value"}' --key unique-request-id
bin/omatracker agent ACTION --input-file request.json
# --input-file - reads stdin, useful for avoiding shell quoting.
```

Every agent response is JSON. Success uses `schemaVersion: 1`, `ok: true`, and
`data`. Ledger mutations also return `changed` and the ledger `revision`. Failures
use `ok: false`, `error.code`, and `error.message`, with a nonzero exit status.
Unknown request fields are rejected. Normal clap usage errors use stderr/exit 2.
The existing panel CLI retains its output formats for compatibility; agents should
use the versioned `agent` interface.

### Bulk dated work: `work.record-batch`

Create new tasks and dated entries in one execution call, with a retained workflow
key from `request.key`. Supply the same input and resolved `--key` to resume.

```json
{
  "project": "PROJECT_ID",
  "items": [{
    "ref": "webhook-mapping",
    "newTask": {"title": "Webhook event mapping"},
    "entries": [{"start": "2026-09-01T09:00:00Z", "end": "2026-09-01T13:00:00Z"}]
  }],
  "pricing": {"mode": "explicit", "rate": "50", "currency": "USD"},
  "summary": {"from": "2026-09-01", "to": "2026-09-16"}
}
```

- `project`, `items`, `pricing`, and `summary` are required. Only new tasks are
  supported. Project selection is explicit and independent of the UI/repository.
- Bounds: 1–50 items, 1–200 entries per item and at most 200 entries total. Unique
  `ref` values contain 1–64 ASCII letters, digits, `-` or `_`. Titles must be
  nonblank, single-line text of at most 160 characters; optional entry `note` is
  single-line text of at most 240 characters. Unknown fields are rejected.
- Entries require `start` and `end` RFC3339 timestamps at whole-second precision,
  after the epoch, with positive duration of at most 31 days, ending no later than
  now. Summary dates use the project's timezone and an exclusive `to`.
- Pricing must be either `{"mode":"explicit","rate":"50","currency":"USD"}`
  or `{"mode":"historical-inheritance"}`. Extra rate fields in inheritance mode
  are rejected. Explicit zero is a billable zero rate, not missing pricing.
- Explicit pricing is **entry-scoped**, captured atomically when each entry is
  created. It neither backfills other work nor creates a task-rate override nor
  changes project history/future policy. This is also available on `entry.add`
  through the same optional `pricing` object. Omitted `entry.add.pricing` retains
  the existing historical task/project inheritance behavior.
- `dryRun: true` validates every item and simulates the existing Rust mutation and
  summary paths in memory. It requires no key and writes no ledger or journal
  (a ledger lock file may be created). It returns planned actions and per-interval
  billing segments, including unresolved/non-billable historical time. Dry-run
  estimates are not reservations: inheritance uses history at each actual write.

Execution is sequential and resumable, **not a transaction across the batch**.
Before the first task is created, a versioned journal reserves the workflow key in
the ledger's existing receipt store, recording the normalized request/fingerprint,
canonical ledger path/incarnation, every step key, arguments, and task-ID dependencies. Resolved
arguments are saved before a dependent write. Each mutation uses the existing
atomic receipt path; an interrupted uncheckpointed mutation replays its original
key. Journal checkpoints retain returned IDs, revisions, and completion. A separate
workflow-worker lock serializes batch coordinators without holding the ledger lock
across nested calls; unrelated ledger edits need no global revision match.

Keys are ledger-local. Resume must use the original ledger and retained request:
copied/moved journals reject a different canonical path with `WORKFLOW_LEDGER_MISMATCH`.
Changing normalized input under a retained key gives `IDEMPOTENCY_CONFLICT`. An
unrelated ledger with no such journal treats a key as new; always retain the ledger
path with the key. The first workflow assigns `billing.ledgerId`, preserved by
ordinary writes and invalidated by clear-all. A content-free binding under
`<ledger>.workflows/<SHA-256-of-key>.json` retains only its format version and opaque
ledger token. It survives clear-all, rejecting old keys after same-path reset or
replacement. Task names, input and results live solely in ledger journals/receipts
and their explicit backups. Removing bindings discards this reset protection.
Restores must keep the ledger and receipt/journal state consistent.

Success returns `data.status: "completed"`, `resume: {key, ledger}`, per-item
`ref`, `taskId`, `entries` (IDs, seconds and captured billing), completed/pending
step counts, and `skippedAdjustments: []` (pricing is applied at creation). It also
returns one authoritative project-range `summary` with `summaryRevision`, including
other work in that range. Exact completed retries return the captured result with
`replayed: true`, after checking created targets still exist. It is a historical
completion snapshot; use `summary` again if later edits need to be reflected.

On a step/verification failure the CLI exits nonzero with `ok: false`, the original
`error`, and `data.status: "partial"`, item progress, `failedStep`, and the resume
identity. Receipt-confirmed but uncheckpointed writes appear as `recorded` steps;
they still need replay/checkpointing. Successfully recorded work is retained. Retry
the original request/key after resolving the cause; never generate replacement
keys to work around partial failure. Preflight errors create no tasks or journal.
Removed targets produce `REQUEST_TARGET_REMOVED`; they are never recreated by a
retry. If storage itself is unavailable, partial reporting may only include the
last known checkpoint; retry against the original ledger to reconcile receipts.

### Global skill installation

Install the CLI from its repository with `make install` first (or `make install-bin`
for a prebuilt release). This puts the `omatracker` command in `~/.local/bin` and
an independent binary/template bundle in `${XDG_DATA_HOME:-~/.local/share}/omatracker`.
Ensure `~/.local/bin` is on `PATH`. Then the following commands work from any directory:

```sh
omatracker skill install --harness opencode
omatracker skill install --harness claude,codex
omatracker skill install --harness gemini,cursor --dry-run
omatracker skill targets --json
```

When the widget is installed, use `make install-plugin` from the same checkout and
`omarchy restart shell` to load matching runtime files (a rescan can retain cached
QML components). This pins the
widget's backend to the standalone installation, so subsequent CLI updates also
update its backend. `omarchy-shell omatracker status` reports the widget's actual
ledger/backend paths and cached entity IDs; `omarchy-shell omatracker refresh`
requests an immediate refresh. Normal polling picks up external CLI edits within
five seconds. An old 0.4 backend must not write a version 3 or newer ledger: it drops billing
and archive metadata it doesn't understand.

The installer is user-global and needs no sudo. It does not open or migrate a ledger.
Harness names may be comma-separated or passed in repeated `--harness` options.

| Harness | Global skill directory |
| --- | --- |
| `shared` (default), `codex` | `~/.agents/skills/omatracker` |
| `opencode` | `${XDG_CONFIG_HOME:-~/.config}/opencode/skills/omatracker` |
| `claude` (alias `claude-code`) | `${CLAUDE_CONFIG_DIR:-~/.claude}/skills/omatracker` |
| `gemini` (alias `gemini-cli`) | `~/.gemini/skills/omatracker` |
| `cursor` | `~/.cursor/skills/omatracker` |

Codex uses the shared directory, so selecting both deduplicates the destination.
OpenCode, Gemini CLI and Cursor also discover shared skills; prefer the shared
installation for those harnesses together rather than creating duplicate copies.
Claude Code has its own personal skill directory. Environment overrides must be
absolute paths. These are local-machine installations, not cloud provisioning.

The binary embeds the matching skill and reference docs and records its absolute
path in `references/installation.md`. Reinstall after moving the executable or
upgrading OmaTracker. Unmodified managed installations update automatically;
identical installations are a no-op. Existing/modified content is preserved unless
`--force` is supplied. Replacements and updates keep the old directory under
`<harness-config>/omatracker-skill-backups/`, outside the scanned `skills/` tree.
Symlink destinations must be moved aside explicitly. Multi-harness requests
preflight all conflicts; publication is per destination, not an all-target transaction.

`--dry-run` creates no files or directories. `--json` returns destinations, planned
or performed actions, and any backup paths. Quit and restart OpenCode after
installation. Restart/reload skills in other harnesses to discover the skill.

### Global skill removal

```sh
omatracker skill remove --harness opencode
omatracker skill remove --harness claude,codex --dry-run
omatracker skill uninstall --harness gemini --json
```

`remove` and its alias `uninstall` accept the same harness names, repeated or
comma-separated, and default to `shared`. Codex and shared select the same directory;
removing it affects all harnesses loading that shared skill. Removal is scoped to
the selected directories; other copies may remain discoverable through a harness's
compatibility paths.

Unmodified managed installations are deleted. Their contents are checked against
their own installation manifest, so a CLI update does not make an older skill
look modified. Modified/unmanaged installations require `--force`; their directory
is then moved to `omatracker-skill-backups/` outside skill discovery rather than
discarding customizations. Symlink destinations must be moved aside explicitly.
Previous backups, unrelated skills, the tracker binary, templates, and ledger remain.

Absent installations are successful no-ops. `--dry-run` writes nothing, including
when the harness directories don't exist. `--json` returns a `removals` array with
the harnesses, path, action, and optional backup path. Actions are `remove` or
`remove-with-backup` for planned removal, `removed` after removal, or `absent`.
All targets are preflighted before changes; each target shares the installation lock
and is rechecked before removal. As with install, this is not an all-target transaction.
Quit and restart OpenCode after removal; restart/reload skills in other harnesses.

### Agent requests

- IDs are opaque; use the IDs returned by commands, never invent them.
- Supply `project` explicitly, or supply a bound `repository`. The agent API never
  uses the panel's active project and project creation does not switch it.
- `from` and `to` are `YYYY-MM-DD`, **to exclusive**, interpreted in the project's
  IANA timezone. Default timezone is UTC. Weeks start Monday.
- `start`, `end`, and `effectiveAt` are RFC3339 timestamps with explicit offsets.
- Durations, `seconds`, and signed correction `delta` are integer seconds.
- Monetary totals are exact integer minor-unit strings. Never sum different
  currencies. Lines are grouped by task title and historical rate; each line is
  rounded half-up once, then the invoice total sums the displayed line amounts.
- Lists accept `offset` and `limit` (default 50, maximum 200), returning `items`,
  `total`, and `nextOffset`. Follow pagination rather than assuming the first page
  contains everything.
- Use `--key` for retry-safe ledger writes, especially creation, manual entries,
  corrections, and issuance. Identical retries replay the original response;
  different arguments with the same key return `IDEMPOTENCY_CONFLICT`.
- Prepare fresh keys together with `agent request.keys` (or `request.key` for one)
  **before new logical operations**. Store each key with its logical step, then pass
  it with `--key`; reuse it only for an exact retry. Keys identify
  requests, not clients/projects or names. Deleting then recreating an entity requires
  a new key and produces a new ID. Replaying a creation whose entity was removed
  returns `REQUEST_TARGET_REMOVED` instead of handing back an obsolete ID.
- For interactive convenience, `--key auto` generates a fresh key per invocation,
  prints it to stderr before executing, and includes `requestKey` in the successful
  JSON response. Retry with that resolved key, **not** `auto`, to avoid creating a
  second operation. Agents should prefer `request.keys` so they know the keys before
  starting a write. Successful keyed responses include `requestKey`.
- Render/upload also support durable retry keys. After interruption, an upload
  retries the same pinned destination with `rclone copyto --checksum`.
- Other external operations (template/filesystem management, diagnostics, previews,
  and explicit Drive tests) do not accept retry keys. Template creation refuses
  to overwrite an existing name; inspect it after an interrupted create.
- Task/project/client get/list responses expose an opaque **`entityRevision`**.
  Pass that token for updates, removals and rate changes. Unrelated ledger activity
  does not invalidate it; changes to that entity do. This is the preferred guard.
  Legacy numeric `revision` remains a whole-ledger check for entity edits, so it
  can conflict after any unrelated write. Do not supply both kinds of token.
- Entry corrections and invoice edits still require their own numeric `revision`.
  Issuer and migration updates optionally accept a ledger revision.
  `REVISION_CONFLICT` and `STALE_DRAFT` mean inspect/refresh the specific target.
- Ledger changes and retry receipts are committed under one advisory lock. External
  rendering/uploads use separate worker locks so timers remain responsive.

### Grouped retry-key preparation

```sh
bin/omatracker agent request.keys --input '{"labels":["invoice:create","invoice:issue","invoice:render","invoice:upload"]}'
```

Response (key values shown as placeholders):

```json
{"schemaVersion":1,"ok":true,"changed":false,"data":{"keys":{"invoice:create":"request-<uuid-1>","invoice:issue":"request-<uuid-2>","invoice:render":"request-<uuid-3>","invoice:upload":"request-<uuid-4>"}}}
```

`request.keys` accepts only an object containing `labels`; `--input-file` and stdin
work as for other actions. Labels are case-sensitive and returned exactly, without
trimming or Unicode normalization. Empty batches, duplicates, invalid labels/types,
extra fields, more than 64 labels, or a `--key` argument return `INVALID_INPUT`.
The mapping is compact: one string per label, with no per-item request metadata.
Do not rely on object ordering. Every invocation generates fresh keys, including
when labels repeat across calls. Labels associate keys with steps; they are not
idempotency keys themselves. `request.key` retains its existing `data.key` response.

Both key actions avoid reading, creating, migrating, or locking the selected ledger.
They do not save keys or receipts. If generation is interrupted **before any write
uses the keys**, it can be repeated. Once a write starts, retain and reuse its original
resolved key, action, and exact input (including IDs and revisions), even if the
response is lost. Do not reconstruct the input with different defaults or substitute
fresh keys to resolve uncertainty. `--key auto` is only a new-operation convenience.

Prepare labels for later dependent steps as soon as their logical identity is known;
an invoice ID need not exist to allocate its issue/render/upload keys. Independent
discovery reads and key preparation can share a tool round. Validate each mutation's
response before resolving arguments and revisions for the next dependent mutation.
An invoice sequence needs at most one dedicated key-preparation round, rather than
four. Six tasks with create/add-entry/price-entry steps can prepare all 18 keys in
one call (e.g. `task-1:create`, `task-1:add-entry`, `task-1:price-entry`, through task 6).
This removes key-generation model turns; it does not batch or eliminate the writes.

For multiple new dated tasks, use `work.record-batch` with one retained workflow
key; it owns durable planning, dependent IDs, receipts and the final summary.
`request.keys` alone does not execute or journal mutations. The source repository's
`plans/workflow-journal.md` documents the implementation; the installed contract
and recipe are in this document and `references/workflows.md`.

## Operations

Fields below are inside the `--input` object. Optional fields are in parentheses.
`details` replaces a complete party profile; read it before updating it.

### Discovery, setup, and projects

| Action | Input |
| --- | --- |
| `help` | `{}` — operation names and basic conventions |
| `data.clear` | (`dryRun: true`, `includeDrive: true`); full reset, only on explicit user request |
| `request.key` | `{}` — generate a fresh retry key without opening a ledger |
| `request.keys` | `labels`: 1–64 unique strings, each 1–80 UTF-8 bytes, no surrounding whitespace/control characters; returns `data.keys` without opening a ledger |
| `context` | `{}` — projects, running timers, draft summaries, ledger revision |
| `doctor` | `{}` — tool availability/versions, scheduler diagnostics, setup instructions |
| `issuer.get` | `{}` |
| `issuer.set` | `details` (`revision`) |
| `client.list` | (`includeArchived: true`, pagination) |
| `client.get` | `id`; returns details and archive status |
| `client.set` | `details` (`id` to replace, `entityRevision` for an existing client) |
| `client.update` | `id`, `name` (`entityRevision`); preserves address/contact/payment fields |
| `client.remove` / `client.delete` | `id` (`entityRevision`); archive an unassigned client |
| `project.list` | (`includeArchived: true`, pagination) |
| `project.get` | `project` or `repository` |
| `project.create` | `name` (configuration fields below, `copyFrom` project ID) |
| `project.configure` | `project`, any configuration fields (`entityRevision`) |
| `project.update` | Alias of `project.configure`; use `name` to rename |
| `project.remove` / `project.delete` | `project` (`entityRevision`); stop its timers and archive |
| `project.rate` | `project`, either `rate` + `currency` or `noRate: true` (`effectiveAt`, `entityRevision`) |
| `repository.bind` | `project`, `repository` existing directory |
| `repository.resolve` | `repository` |

Party `details`: `name`, `address`, `email`, `registrationId`,
`paymentInstructions` (issuer). Address/payment text may contain newlines.

Project configuration fields: `name`, `client` (client ID; empty string unlinks), `template`, `logo` (image path;
empty removes), `accentColor`, `paper` (`a4`/`letter`), `cadence`
(`monthly`/`weekly`/`manual`), `timezone`, `dueDays` (default 30), `driveFolder`.
Creation/configuration can also include `rate`, `currency`, `effectiveAt` or
`noRate`. Supplied logos are copied into managed storage. `copyFrom` copies
settings and the current rate, not tasks, entries, invoices, or rate history.

### Time and corrections

| Action | Input |
| --- | --- |
| `task.list` | `project` (`running: true`, `includeArchived: true`, pagination) |
| `task.get` | `id` |
| `task.create` | `project`, `title` or `name` |
| `task.update` | `id`, any of `name`/`title`, `add` duration, task-rate fields (`entityRevision`); one atomic edit |
| `task.rate` | `id`, exactly one of `rate` + `currency`, `noRate: true`, or `inheritRate: true` (`effectiveAt`, `applyExisting`, `reason`, `entityRevision`) |
| `task.remove` / `task.delete` | `id` (`entityRevision`); stop its timer, remove the task, retain dated entries |
| `task.archive` | `id` (`entityRevision`); stop its timer and hide the task while retaining its identity and history |
| `task.restore` | `id` (`entityRevision`); restore an archived task to active lists |
| `task.start` / `task.stop` | `id` (task ID) |
| `entry.list` | `project` (`id` entry ID, `from` + `to`, pagination) |
| `entry.add` | `id` task ID, `start`, either `end` or `seconds` (`note`, `pricing` for entry-scoped explicit pricing or historical inheritance) |
| `entry.correct` | `id` entry ID, `revision`, signed `delta`, `reason` |
| `entry.undo` | `id` correction ID, current **entry** `revision`, `reason` |
| `summary` | `project`, `from`, `to` |

An entry's rate is captured when it is recorded using effective-dated rate history.
Sessions spanning rate changes are split. A missing rate is non-billable; zero is
a real billable rate. Later rate changes do not reprice recorded entries. Backdated
entries use the history applicable to their timestamps.

For “yesterday, 20:00–23:00”, read `project.get` → `billing.timezone` and resolve
both the date and endpoint offsets there. With local today fixed at September 19,
2026 in `Europe/London`, the request is:

```sh
omatracker agent entry.add --input '{"id":"TASK_ID","start":"2026-09-18T20:00:00+01:00","end":"2026-09-18T23:00:00+01:00"}' --key auto
```

Inspect **every** returned `data.entries[]` segment's `billing.rate` and
`billing.resolved`, not just the successful envelope. Unresolved time is excluded;
resolved time with `rate: null` is non-billable; an explicit zero is billable.
If the project rate first took effect September 19, report the three hours as
non-billable immediately. If billing was expected and historical pricing is unclear,
ask one focused rate/scope question; a current project/task rate is not authorization
to retroactively price it.

### Assign a rate to an existing task

Tasks inherit the project rate until explicitly overridden. A task can have its
own rate even when the project has none:

```sh
# New work only (effective now unless effectiveAt is supplied):
omatracker agent task.rate --input '{"id":"TASK_ID","rate":"50","currency":"USD"}' --key auto

# Explicitly price its existing unrated, uninvoiced entries too:
omatracker agent task.rate --input '{"id":"TASK_ID","rate":"50","currency":"USD","applyExisting":true}' --key auto

# Return to project inheritance, or make future work non-billable:
omatracker agent task.rate --input '{"id":"TASK_ID","inheritRate":true}'
omatracker agent task.rate --input '{"id":"TASK_ID","noRate":true}'
```

`task.get`, `task.list`, and widget task snapshots return the current `rate`,
`hourlyRate`, `rateSource` (`task`/`project`), and `entityRevision`. Agent task
responses also include the override `rateHistory`. Both project and task rate
boundaries split new time entries; explicitly returning to project inheritance
uses that project's historical rate at each point in time.

`applyExisting: true` requires an explicit rate and affects only dated entries for
that task whose rate is absent, including unresolved billing if explicitly selected
this way. It skips already-priced entries (including zero-rate entries), externally
billed entries, and any entry allocated to an issued/paid invoice. It applies to all
eligible recorded dates, independently of the future policy's `effectiveAt`.
Undated legacy counters are not converted into billable entries.

For a running timer, backfilling first records elapsed time with its old pricing,
then rates eligible unrated segments and continues the timer. Without backfill,
the old running portion retains its old rate and new work uses the new policy.
The result includes `rateChange.appliedEntryIds`, skip counts, and an audit ID.
`entry.list` includes entry-specific `rateAdjustments` with the previous billing
metadata. Existing drafts must be refreshed; issued invoices never change.

For “make that existing work billable at USD 50/hour; edit or recreate it”, prefer
the history-preserving rate edit. Ask only if the conversation has not already
established the rate and historical scope. Read `task.get` for `entityRevision`,
then paginate `entry.list` across **all dates** for its project and select by
`entry.taskId`; `entry.list.id` is an entry ID. Check eligibility and any issued/paid
allocations. If other eligible entries or elapsed running time fall outside the
authorized slot, explain the task-wide mismatch before writing. `effectiveAt` does
not restrict backfill dates, and task deletion retains dated entries. The call also
sets the future task rate, so resolve any explicit future-policy constraint first.

Once that scope is established (placeholders below are returned IDs/tokens):

```sh
omatracker agent task.rate --input '{"id":"TASK_ID","rate":"50","currency":"USD","applyExisting":true,"entityRevision":"TASK_TOKEN","reason":"User authorized pricing existing three hours"}' --key auto
omatracker agent entry.list --input '{"project":"PROJECT_ID","from":"2026-09-18","to":"2026-09-19"}'
omatracker agent summary --input '{"project":"PROJECT_ID","from":"2026-09-18","to":"2026-09-19"}'
# If a draft already exists, get its current numeric revision, refresh, then preview:
omatracker agent invoice.get --input '{"id":"DRAFT_ID"}'
omatracker agent invoice.refresh --input '{"id":"DRAFT_ID","revision":DRAFT_REVISION}' --key auto
omatracker agent invoice.preview --input '{"id":"DRAFT_ID"}'
```

Check `rateChange.appliedEntryIds` and `adjustmentId`, plus the **counts** in
`rateChange.skipped` (`alreadyRated`, `invoiced`, `externallyBilled`). Identify
specific skipped entries from entry/allocated-invoice inspection when needed; the
response does not supply skipped IDs. Confirm preserved intervals/durations and
updated billing/audit metadata with `entry.list`. For exactly those three hours,
`summary.uninvoiced` should report `amountMinor: "15000"`, `amountText: "USD 150.00"`.
`task.get` alone cannot establish a correct historical invoice. See
`skills/omatracker/references/workflows.md` (installed: `references/workflows.md`)
for the full decision table and correction recipe.

The widget's task editor has **Use project rate**, task rate/currency fields, and
an opt-in checkbox to price existing unrated time. Saving name, manual added time,
and rate settings is atomic. Added manual time is recorded under its historical
pricing before the rate change; the opt-in checkbox includes that newly added time
if it is unrated. Use separate dated `entry.add` calls for precise backdated work.

Corrections retain the original interval and adjust the recorded duration within
it proportionally, with an audit record. Zero-duration corrected entries remain
in the ledger. Corrections cannot make duration negative and cannot alter time
allocated to a live issued/paid invoice or marked externally billed. Void the
invoice, correct the entries, then prepare a linked replacement. Undo records an
inverse correction rather than deleting history.

### Renaming and deleting entities

```sh
omatracker agent task.update --input '{"id":"TASK_ID","name":"Design review"}'
omatracker agent project.update --input '{"project":"PROJECT_ID","name":"Website redesign"}'
omatracker agent client.update --input '{"id":"CLIENT_ID","name":"Acme Ltd"}'
omatracker agent task.remove --input '{"id":"TASK_ID"}' --key remove-task-1
omatracker agent project.remove --input '{"project":"PROJECT_ID"}' --key remove-project-1
omatracker agent client.remove --input '{"id":"CLIENT_ID"}' --key remove-client-1
```

Use the target's `entityRevision` from a get/list response for optional optimistic
concurrency on these operations. Names must be nonblank. `task.update` accepts `name` or `title`;
when both are provided they must agree. Renaming retains IDs and settings. Recorded
time keeps its original task title and issued invoices retain all captured names.
Client renames update the display name on linked projects while preserving the
client's other details. Existing drafts may require refresh before issuance.

Task deletion records any running interval before removing the task. Dated entries
and issued invoices remain; undated legacy counters follow the existing task-delete
behavior and leave the active counter list with the task.

Task archiving records any running interval, retains the task ID, settings, rates and
dated entries, and hides the task from normal `task.list` responses and the widget.
Use `includeArchived: true` with `task.list` to inspect archived tasks. Archived
tasks cannot be started, edited, repriced, or used for new entries. `task.restore`
returns an archived task to active lists; restoring is rejected when its project is
archived.

Project deletion is archival: it hides the project from active lists and the panel,
stops its timers, disables scheduling, removes repository associations, and selects
Unassigned if necessary. Tasks, dated time, rates, client links and invoices remain
available by explicit ID for historical queries and final billing. New tracking and
configuration changes on archived projects are rejected. The fallback Unassigned
project cannot be removed.

Clients still assigned to active projects return `CLIENT_IN_USE`. First reassign
those projects, unlink with `project.update` and `"client":""`, or archive the
projects. Client removal archives the profile so historical billing still has the
original contact details. Archived clients cannot be assigned to new projects.
`project.list` and `client.list` hide archives by default; `includeArchived: true`
includes them with an `archived` flag. Direct get operations expose archive status.
Repeated project/client removal is a no-op; use retry keys for task removal.
When recreating a deleted client, omit `id` in `client.set` and use a fresh key.
Cloning an archived project preserves its settings but unlinks any archived client;
assign the replacement client's new ID explicitly.

The panel's old counter amount remains a **current-rate counter estimate**. Use
`summary` or invoice data for actual historical billing amounts. Resets/deleting a
task affect panel counters, not recorded invoice history.

### Invoices

| Action | Input |
| --- | --- |
| `invoice.list` | (`project`, pagination) |
| `invoice.get` | `id` |
| `invoice.create` | `project`, `from`, `to`, `currency` |
| `invoice.period` | `project`, `cadence` (`weekly`/`monthly`); drafts previous completed period for each currency |
| `invoice.check` | `{}`; prepare scheduled drafts |
| `invoice.refresh` | `id`, `revision` |
| `invoice.preview` | `id` of draft |
| `invoice.issue` | `id`, `revision`, `date` (issue date) |
| `invoice.render` | `id` of issued/paid/void invoice; returns captured original PDF |
| `invoice.upload` | `id` of issued/paid invoice |
| `invoice.paid` | `id`, `revision`, `date` (payment date) |
| `invoice.void` | `id`, `revision`, `reason` |
| `invoice.reissue` | `id`, `revision` of void invoice; creates linked replacement draft |

Lifecycle: draft → issued → paid, or void. Numbering is ledger-wide
`INV-YYYY-NNNNN`, allocated atomically at issuance; void numbers are not reused.
Issue/due dates and payment instructions are captured with invoice data. No tax,
discount, partial-payment, or credit-note calculations are performed.

Drafts show excluded non-billable, unresolved, already-billed time and running
timer counts. Live timer time is not billable until stopped. Multiple currencies
require separate invoices. Empty drafts can be inspected but cannot be issued.

Before `invoice.create`, require a `summary` for that exact project/range unless
an equivalent, still-current summary is already available. Explain the excluded
time/running timers and resolve unexpected exclusions using established intent.
Rerun the summary after entry, historical pricing, allocation, or range changes.
After changing a draft's source, use `invoice.get` and `invoice.refresh` with its
current numeric revision before previewing; `invoice.preview` does not refresh
billing. Avoid another unchanged preview while the underlying pricing is still wrong.

Monthly drafting is the default. Scheduling never issues or uploads. Repeated
checks reuse existing drafts without silently refreshing reviewed content. Late
entries require a draft refresh, or create a supplemental draft when the earlier
invoice has already been issued. Issuance rechecks source revisions and allocated
entry intervals, preventing duplicate charges across overlapping periods.

Issuance captures data, selected template, images, and logo. Render and upload are
separate operations/statuses. A failed render does not consume another invoice
number; retry it. A failed upload does not invalidate the local PDF. Upload status
is not payment status. Voiding records document state; it does not delete remote
files or reverse a bank payment. The original issued PDF remains archived.

### Templates and Drive

| Action | Input |
| --- | --- |
| `template.list` | `{}` |
| `template.create` | `name` (`copyFrom` template ID, default `invoice`) |
| `template.path` | `id` |
| `template.asset` | `id` user template, `source` image path; returns relative reference |
| `template.validate` | `id` |
| `artifact.open` | `path`: explicit existing local PDF path (not a URI); no retry key |
| `drive.configure` | `remote` (`driveFolder`, `syncOnStartup`) |
| `drive.check` | `{}`; read-only remote access test |
| `drive.test` | `{}`; explicitly uploads a small uniquely named test file |

Edit the user-owned `.typ` path, validate, select it with `project.configure`, then
preview a draft. `TEMPLATES.md` documents the invoice template data contract.

`template.validate` preserves `data.valid: true` for successful compilation of
representative invoice/report fixtures. It additionally returns `checks.compile`
(`status: passed`, fixture input description), `checks.text` (`passed`, `warnings`,
`skipped` if optional `pdftotext` cannot start, or `failed` on extraction failure),
`checks.visual` (`status: not_performed`), and a `warnings` array. Text checks are
heuristic: missing expected fixture content or leaked Typst expressions is a
warning, not a compilation error or proof of bad layout. Compilation errors still
return the normal error envelope. Inspect the rendered actual draft after edits.

`artifact.open` canonicalizes a local PDF path and dispatches an encoded `file://`
URL through `xdg-open` (Linux) or `open` (macOS), consistent with the QML opener.
It rejects URLs, missing/non-PDF files and retry keys; it does not access the ledger.
The response includes `path`, `status`, `launchRequested`, `visiblyOpened: null`,
and `message`. With valid input the envelope can be `ok: true` while the launch
status is `launch_failed`: inspect **data.status**, not just the envelope.
Missing desktop integration, missing opener, or immediate nonzero exit preserves
the usable PDF path with a diagnostic (and `exitCode` when available).
`launch_requested` means the opener exited successfully or is still running after
a 250 ms observation window. Standard streams and the viewer process group are
detached; later failures and document visibility cannot be confirmed by the CLI.

For “show/open”, obtain a current PDF and call `artifact.open` in the same turn.
For “review”, read the PDF. Reuse a known existing preview only when all relevant
inputs are known unchanged; no persistent preview cache is maintained. Refresh
draft billing before rendering after source changes: `invoice.preview` is not a
refresh operation. Template/assets/logo changes invalidate preview reuse even
without a draft revision change. Issued invoices always render captured originals.

Run `rclone config` for OAuth setup; the user completes browser authentication.
OmaTracker stores the remote name, not OAuth credentials. `drive.test` leaves its
test file in `setup-tests/`. Invoice uploads return the remote destination; they
do not invent public Google Drive links or change sharing permissions.

## Administration

### Global skill installation

Install the CLI from its repository with `make install` first (or `make install-bin`
for a prebuilt release). This puts the `omatracker` command in `~/.local/bin` and
an independent binary/template bundle in `${XDG_DATA_HOME:-~/.local/share}/omatracker`.
Ensure `~/.local/bin` is on `PATH`. Then the following commands work from any directory:

```sh
omatracker skill install --harness opencode
omatracker skill install --harness claude,codex
omatracker skill install --harness gemini,cursor --dry-run
omatracker skill targets --json
```

When the widget is installed, use `make install-plugin` from the same checkout and
`omarchy restart shell` to load matching runtime files (a rescan can retain cached
QML components). This pins the
widget's backend to the standalone installation, so subsequent CLI updates also
update its backend. `omarchy-shell omatracker status` reports the widget's actual
ledger/backend paths and cached entity IDs; `omarchy-shell omatracker refresh`
requests an immediate refresh. Normal polling picks up external CLI edits within
five seconds. An old 0.4 backend must not write a version 3 or newer ledger: it drops billing
and archive metadata it doesn't understand.

The installer is user-global and needs no sudo. It does not open or migrate a ledger.
Harness names may be comma-separated or passed in repeated `--harness` options.

| Harness | Global skill directory |
| --- | --- |
| `shared` (default), `codex` | `~/.agents/skills/omatracker` |
| `opencode` | `${XDG_CONFIG_HOME:-~/.config}/opencode/skills/omatracker` |
| `claude` (alias `claude-code`) | `${CLAUDE_CONFIG_DIR:-~/.claude}/skills/omatracker` |
| `gemini` (alias `gemini-cli`) | `~/.gemini/skills/omatracker` |
| `cursor` | `~/.cursor/skills/omatracker` |

Codex uses the shared directory, so selecting both deduplicates the destination.
OpenCode, Gemini CLI and Cursor also discover shared skills; prefer the shared
installation for those harnesses together rather than creating duplicate copies.
Claude Code has its own personal skill directory. Environment overrides must be
absolute paths. These are local-machine installations, not cloud provisioning.

The binary embeds the matching skill and reference docs and records its absolute
path in `references/installation.md`. Reinstall after moving the executable or
upgrading OmaTracker. Unmodified managed installations update automatically;
identical installations are a no-op. Existing/modified content is preserved unless
`--force` is supplied. Replacements and updates keep the old directory under
`<harness-config>/omatracker-skill-backups/`, outside the scanned `skills/` tree.
Symlink destinations must be moved aside explicitly. Multi-harness requests
preflight all conflicts; publication is per destination, not an all-target transaction.

`--dry-run` creates no files or directories. `--json` returns destinations, planned
or performed actions, and any backup paths. Quit and restart OpenCode after
installation. Restart/reload skills in other harnesses to discover the skill.

### Global skill removal

```sh
omatracker skill remove --harness opencode
omatracker skill remove --harness claude,codex --dry-run
omatracker skill uninstall --harness gemini --json
```

`remove` and its alias `uninstall` accept the same harness names, repeated or
comma-separated, and default to `shared`. Codex and shared select the same directory;
removing it affects all harnesses loading that shared skill. Removal is scoped to
the selected directories; other copies may remain discoverable through a harness's
compatibility paths.

Unmodified managed installations are deleted. Their contents are checked against
their own installation manifest, so a CLI update does not make an older skill
look modified. Modified/unmanaged installations require `--force`; their directory
is then moved to `omatracker-skill-backups/` outside skill discovery rather than
discarding customizations. Symlink destinations must be moved aside explicitly.
Previous backups, unrelated skills, the tracker binary, templates, and ledger remain.

Absent installations are successful no-ops. `--dry-run` writes nothing, including
when the harness directories don't exist. `--json` returns a `removals` array with
the harnesses, path, action, and optional backup path. Actions are `remove` or
`remove-with-backup` for planned removal, `removed` after removal, or `absent`.
All targets are preflighted before changes; each target shares the installation lock
and is rechecked before removal. As with install, this is not an all-target transaction.
Quit and restart OpenCode after removal; restart/reload skills in other harnesses.

### Clear-all and the protected workspace

`Unassigned` has the fixed ID `project-unassigned`. It is the internal destination
for unassigned tasks and the fallback when an active project is removed. The state
model recreates it if missing. **Protected is an application invariant, not an
authorization or filesystem permission.** Project list/get results label it with
`protected: true`. Renaming it does not change its reserved identity.

Normal `.remove` operations preserve history. For a deliberate fresh start, the
CLI has a separate operation:

```sh
omatracker data clear --dry-run
omatracker data clear --include-drive --dry-run --json
```

Execute only when a full reset is intended. If the user asks for a command or an
explanation, provide it without executing it on their data:

```sh
omatracker data clear
```

To include Drive, use this **instead of** the local-only command, before removing
the metadata that identifies uploaded files, and only when explicitly requested:

```sh
omatracker data clear --include-drive
```

The agent equivalent is `agent data.clear --input '{"dryRun":true,"includeDrive":true}'`.
Without `dryRun`, it executes. This operation does not accept retry keys: inspect
the backup/progress after an interruption before starting another clear, particularly
if new work has been recorded since. Tests use disposable ledgers/fake remotes;
running the examples against your actual ledger is a user action, not a required
installation step.

The operation clears all user projects and clients (including archives), tasks,
time entries, corrections, task rates, invoice/report records, repository bindings
and request receipts. It resets Unassigned to an empty default. Its result reports
`remaining.userProjects: 0` and a separate `systemWorkspace`; a raw project list
still contains that internal workspace. Issuer details, invoice-number sequences,
Drive settings, reusable templates/images and local UI/audio preferences survive.
Number sequences are retained to avoid reusing already-issued invoice numbers.

Before deletion it backs up the raw ledger, feedback checkpoint and generated
files under `<ledger>.backups/clear-<id>/`. `manifest.json` records the plan,
original local paths, remote targets and incremental completion status. Local
invoice storage `<ledger>.invoices/` is cleared, including orphaned captured
bundles and invoice previews. Tracked report artifacts are removed only within the
report cache with matching generated filenames. Unrelated caches, anonymous
legacy previews and previous backups are not swept. A backup intentionally retains
the old data for recovery; remove that backup separately when you no longer need it.

`--include-drive` requires a configured rclone remote. It inventories exact recorded
report destinations and recorded/derivable invoice destinations, including known orphaned invoice snapshots,
and the current `state.json`. Legacy reports without a recorded upload destination
are not guessed by project name, which could collide with another ledger.
The remote ledger must share identifiers with this
ledger or match its contents/configuration (apart from sync bookkeeping). Unknown,
moved or renamed remote files are not guessed. Unrelated documents and setup-test
files are not included; no remote directories are deleted.

All remote targets are downloaded to the local backup **before any remote deletion**.
Their metadata is checked again, then `rclone deletefile` removes each exact file
and its absence is verified. Rclone's configured deletion behavior applies (Google
Drive normally moves deleted files to Trash). A remote failure leaves the local
records intact; already-completed remote deletions and backups are recorded for
recovery/retry. After remote success, the local ledger is reset and generated
local files are removed. A later local cleanup failure is reported with the backup
path and completion journal rather than being presented as a complete success.

The command waits for upload/render workers and holds the ledger lock to prevent
new writes during the reset. A cloud dry-run makes read-only remote requests but
creates no data backups or deletions. Lock files may be created. The widget retries
status polling after temporary lock timeouts and picks up the resulting empty state.

### Migration and storage

| Action | Input |
| --- | --- |
| `migration.preview` | `{}`; no ledger mutation |
| `migration.apply` | `{}`; persist schema upgrade and audit event |
| `migration.resolve` | `project`, `from`, `to`, either `rate` + `currency` or `noRate: true` (`externallyBilled: true`, `revision`) |

First write upgrades to ledger version 4. Version 3 ledgers are backed up to
`<ledger>.pre-task-rates.bak`; pre-invoice ledgers use `<ledger>.pre-invoices.bak`.
Backups are not overwritten. Version 3 billing metadata is preserved and tasks
initially continue inheriting their project rates. Older 0.5 backends reject a
version 4 ledger, preventing silent loss of task-rate overrides and adjustments.
Existing pre-invoice dated entries have **unresolved** billing,
not a guessed rate or silently non-billable status. Resolve complete entry
intervals in a selected range; boundary-crossing entries remain unresolved until
the range includes them. Existing archived reports do not establish that work was
paid: explicitly identify already-billed time. Undated legacy counters cannot be
invoiced without manually creating dated entries.

Issued artifacts live in `<ledger>.invoices/`; editable templates and imported
logos remain under the existing XDG configuration template library. Back up the
ledger, its invoice directory, and the template library together. Restore them as
one set: restoring an old ledger alone also rolls back numbering/allocation state.
The `sync` command uploads the ledger snapshot; invoice PDFs use `invoice.upload`.

Old `report export`/`report retry` explicitly operate on archived time reports.
`report check` now prepares invoice drafts, including when invoked by an existing
systemd timer. `report archive-check` is an explicit legacy maintenance command.
New projects should use the invoice workflow and the `invoice` template.
