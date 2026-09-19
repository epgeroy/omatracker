---
name: omatracker
description: Create, rename, and delete OmaTracker tasks, projects and clients; set task/project rates, track or correct time, customize invoice templates and images, issue PDF invoices, upload to Google Drive, and diagnose dependencies using the local CLI.
---

# OmaTracker

Use the installed plugin's `bin/omatracker`, or this repository's `bin/omatracker`.
Do not assume a binary with the same name on PATH is the intended installation.
Respect a user-specified ledger using `--data-path`. No MCP server is required.

1. Run `agent help`; read `AGENT_API.md` from this installation for request fields.
2. Use `agent context` and `project.list` to discover projects. Prefer an explicit
   project or `repository.resolve`; never change the panel selection to target work.
3. Send JSON with `agent ACTION --input-file -` or `--input`. Parse `ok`, `data`,
   `error.code`, and pagination. Use the returned IDs and revisions.
4. Before each new logical write, generate and retain a key with `agent request.key`.
   Pass it using `--key`, reusing it only for an exact retry. Never reuse a key merely
   because an entity has the same name, especially after deletion/recreation.
   Interactive `--key auto` prints/returns a new key; retry with the resolved key,
   not `auto`. Render/upload accept keys too.
5. Keep outputs focused: query a project/date range, request summaries, and follow
   pagination. Rust calculates amounts; do not recompute money in the model.

## Workflows

- **Setup:** `doctor`; help install missing Typst/rclone; guide `rclone config`
  browser authentication; `drive.configure`, `drive.check`. Use `drive.test` when
  asked to verify uploads. Detailed recipes: `references/workflows.md`.
- **Project:** inspect/set issuer and client profiles, create/configure the project,
  set rate/currency and intended effective date, timezone, cadence, template, logo,
  and payment terms. Clone settings with `copyFrom` when useful.
- **Rename/delete:** use `task.update`, `project.update`, or `client.update` with
  `name`; task IDs use `id`, project IDs use `project`, and client IDs use `id`.
  Use the corresponding `.remove` (or `.delete`) action when asked to delete.
  Task removal retains dated entries; project/client removal archives history.
  Reassign or unlink clients used by active projects before removing them. Use
  `includeArchived: true` when searching project/client history. Use the target's
  opaque `entityRevision` for task/project/client edits, not the global ledger
  revision: unrelated operations should not block the edit. For a new client,
  omit its old ID. `REQUEST_TARGET_REMOVED` requires a fresh creation key.
- **Task rates:** use `task.rate` with `id`, `rate`, and `currency` even when the
  project has no rate. Default behavior is new work only. Add `applyExisting: true`
  only when asked to price existing unrated, uninvoiced time; explain skipped priced
  or billed entries. `inheritRate: true` restores project inheritance; `noRate: true`
  makes future task work non-billable. Inspect `task.get` for effective rate/history.
- **Time:** discover/create a task, start/stop by ID. Manual entries require actual
  dates, not a guessed current period. Subtraction targets an identified entry;
  ask which entry/date if ambiguous. Record reasons and use revisions for corrections.
- **Invoices:** summarize the selected range, create/refresh draft, preview PDF,
  issue when requested, render, then upload when requested. Report invoice number,
  amount/currency, local PDF path, and actual upload status/destination.
- **Recovery:** refresh stale drafts; inspect revision conflicts; retry rendering
  or uploading the existing invoice. Do not create another invoice to retry a PDF.
- **Clear everything:** ordinary removal archives history. Only when the user
  explicitly wants a full reset, use `data clear` / `agent data.clear`, previewing
  with `--dry-run` / `dryRun: true`. This deletes time and invoice/history records
  too. Include Drive only when explicitly requested. If the user wants the command
  or an explanation, provide it without executing it on their data. The protected
  Unassigned workspace is internal, not a permission restriction; report zero user
  projects separately from the empty system workspace. Report the backup path and
  any partial failures. Do not automatically retry a clear after a lost response.

## Accounting rules

A rate makes work billable, including zero. No rate makes it non-billable. Recorded
time retains its historical rate; backdated entries use effective-dated history.
Explicit `task.rate` with `applyExisting` can price previously unrated, uninvoiced
entries; this is an opt-in adjustment with an audit record, not automatic repricing.
Existing pre-invoice entries require an explicit migration decision. Issued
invoices are immutable; void, correct, and prepare a linked replacement when asked.
Automatic monthly checks generate drafts only. `to` dates are exclusive and use
the project timezone. Different currencies remain separate invoices.

Read `tests/manual-invoices.md` for an isolated end-to-end exercise. Preview template
changes and use invoice data for billing, not the panel's current-rate counter estimate.
