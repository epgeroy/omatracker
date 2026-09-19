# Agent API and invoicing

OmaTracker's agent interface is a local, noninteractive CLI. It needs no MCP server.

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

## Conventions

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
- Render/upload also support durable retry keys. After interruption, an upload
  retries the same pinned destination with `rclone copyto --checksum`.
- Other external operations (template/filesystem management, diagnostics, previews,
  and explicit Drive tests) do not accept retry keys. Template creation refuses
  to overwrite an existing name; inspect it after an interrupted create.
- Entry corrections and invoice edits require the entity's `revision`. Project,
  client, issuer, and migration updates optionally accept the ledger revision.
  `REVISION_CONFLICT` and `STALE_DRAFT` mean inspect/refresh, not blindly retry.
- Ledger changes and retry receipts are committed under one advisory lock. External
  rendering/uploads use separate worker locks so timers remain responsive.

## Operations

Fields below are inside the `--input` object. Optional fields are in parentheses.
`details` replaces a complete party profile; read it before updating it.

### Discovery, setup, and projects

| Action | Input |
| --- | --- |
| `help` | `{}` — operation names and basic conventions |
| `context` | `{}` — projects, running timers, draft summaries, ledger revision |
| `doctor` | `{}` — tool availability/versions, scheduler diagnostics, setup instructions |
| `issuer.get` | `{}` |
| `issuer.set` | `details` (`revision`) |
| `client.list` | pagination |
| `client.set` | `details` (`id` to replace, `revision`) |
| `project.list` | pagination |
| `project.get` | `project` or `repository` |
| `project.create` | `name` (configuration fields below, `copyFrom` project ID) |
| `project.configure` | `project`, any configuration fields (`revision`) |
| `project.rate` | `project`, either `rate` + `currency` or `noRate: true` (`effectiveAt`, `revision`) |
| `repository.bind` | `project`, `repository` existing directory |
| `repository.resolve` | `repository` |

Party `details`: `name`, `address`, `email`, `registrationId`,
`paymentInstructions` (issuer). Address/payment text may contain newlines.

Project configuration fields: `client` (client ID), `template`, `logo` (image path;
empty removes), `accentColor`, `paper` (`a4`/`letter`), `cadence`
(`monthly`/`weekly`/`manual`), `timezone`, `dueDays` (default 30), `driveFolder`.
Creation/configuration can also include `rate`, `currency`, `effectiveAt` or
`noRate`. Supplied logos are copied into managed storage. `copyFrom` copies
settings and the current rate, not tasks, entries, invoices, or rate history.

### Time and corrections

| Action | Input |
| --- | --- |
| `task.list` | `project` (`running: true`, pagination) |
| `task.create` | `project`, `title` |
| `task.start` / `task.stop` | `id` (task ID) |
| `entry.list` | `project` (`id` entry ID, `from` + `to`, pagination) |
| `entry.add` | `id` task ID, `start`, either `end` or `seconds` (`note`) |
| `entry.correct` | `id` entry ID, `revision`, signed `delta`, `reason` |
| `entry.undo` | `id` correction ID, current **entry** `revision`, `reason` |
| `summary` | `project`, `from`, `to` |

An entry's rate is captured when it is recorded using effective-dated rate history.
Sessions spanning rate changes are split. A missing rate is non-billable; zero is
a real billable rate. Later rate changes do not reprice recorded entries. Backdated
entries use the history applicable to their timestamps.

Corrections retain the original interval and adjust the recorded duration within
it proportionally, with an audit record. Zero-duration corrected entries remain
in the ledger. Corrections cannot make duration negative and cannot alter time
allocated to a live issued/paid invoice or marked externally billed. Void the
invoice, correct the entries, then prepare a linked replacement. Undo records an
inverse correction rather than deleting history.

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
| `drive.configure` | `remote` (`driveFolder`, `syncOnStartup`) |
| `drive.check` | `{}`; read-only remote access test |
| `drive.test` | `{}`; explicitly uploads a small uniquely named test file |

Edit the user-owned `.typ` path, validate, select it with `project.configure`, then
preview a draft. `TEMPLATES.md` documents the invoice template data contract.

Run `rclone config` for OAuth setup; the user completes browser authentication.
OmaTracker stores the remote name, not OAuth credentials. `drive.test` leaves its
test file in `setup-tests/`. Invoice uploads return the remote destination; they
do not invent public Google Drive links or change sharing permissions.

### Migration and storage

| Action | Input |
| --- | --- |
| `migration.preview` | `{}`; no ledger mutation |
| `migration.apply` | `{}`; persist schema upgrade and audit event |
| `migration.resolve` | `project`, `from`, `to`, either `rate` + `currency` or `noRate: true` (`externallyBilled: true`, `revision`) |

First write upgrades to ledger version 3, keeping an original sibling
`<ledger>.pre-invoices.bak`. Existing dated entries have **unresolved** billing,
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
