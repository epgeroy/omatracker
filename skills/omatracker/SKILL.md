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
4. Prepare independent keys together with `agent request.keys --input
   '{"labels":["create-task","add-entry","price-entry"]}'` (1–64 unique labels,
   each 1–80 UTF-8 bytes without surrounding whitespace/control characters).
   Retain `data.keys` by logical step, including known later steps whose IDs are not
   yet available. Combine preparation with independent discovery in the same tool
   round. On older installations, group independent `request.key` calls together.
   Persist keys before writes; pass each using `--key`, reusing it and its exact
   arguments only for a retry. Validate results before dependent mutations.
   Never reuse keys merely because an entity has the same name, especially after
   deletion/recreation. `--key auto` prints/returns a new key; retry with the resolved
   key, not `auto`. Render/upload accept keys too. See `references/workflows.md`.
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
  when existing unrated time is explicitly authorized for pricing; ask only when
  the conversation has not already established the historical rate and scope.
  Inspect all eligible task entries first: backfill is task-wide across dates,
  including elapsed running time, not limited to a selected slot. Explain scope
  mismatches and skipped priced/billed entries. Prefer a history-preserving edit
  for “edit or recreate”. `task.get` describes current rate/history, not proof of
  entry pricing. See the correction recipe in `references/workflows.md`.
  `inheritRate: true` restores inheritance; `noRate: true` makes future work non-billable.
- **Time:** discover/create a task, start/stop by ID. Manual entries require actual
  dates and offsets resolved in the project's timezone. Inspect every returned
  segment's `billing.rate` and `billing.resolved`; report billable (including zero),
  non-billable, or unresolved time accurately. A current rate does not establish
  historical pricing. If billable work was expected and pricing is unclear, ask
  one focused rate/scope question. Subtraction targets an identified entry;
  ask which entry/date if ambiguous. Record reasons and use revisions for corrections.
- **Many dated tasks:** use `work.record-batch` with explicit project, unique item
  refs, new task titles, dated entries, pricing mode, and summary range. Confirm
  explicit entry pricing versus historical inheritance; missing billing intent is
  not a default. Retain one workflow key and ledger path. Use `dryRun: true` to
  inspect planned billing; resume partial failures with the original input/key.
  The helper returns IDs, billing outcomes, progress, and a Rust-calculated summary.
- **Invoices:** require a project/range `summary` before creation unless an equivalent,
  still-current summary is available; explain exclusions. Create/refresh draft, preview PDF,
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

## Preview intent and evidence

| User intent | Action |
| --- | --- |
| Generate/export preview | Return PDF path and billing summary. |
| Show/open preview | Obtain a current PDF, then `artifact.open` with `path` in the same turn. |
| Review/check layout | Read the rendered PDF and report visual findings. |
| Show unchanged preview | Reuse a known existing PDF only when relevant inputs are known unchanged. |
| Show draft after billing changes | `invoice.refresh`, then `invoice.preview`, then open. |
| Show issued invoice | `invoice.render`, then open the captured original. |

`invoice.preview` renders the saved draft; it does **not** refresh billing.
An invoice ID alone does not prove freshness: consider entries/rates, draft data,
template source/imported assets, logo, and render settings. Regenerate when unsure
or when a temporary PDF was deleted. Keep reuse session-local.

After any template edit, render and inspect the actual PDF before calling the
layout reviewed. For unchanged templates, inspect new previews when content or
layout differences warrant it (long names, new logo, page breaks, more rows).
`template.validate` is a representative-fixture compile check with optional,
heuristic PDF text checks; neither it nor a viewer launch is visual inspection.
Always inspect the final changed preview before issuing. See the preview and
branding recipes in `references/workflows.md`.

`artifact.open` reports `launch_requested` or `launch_failed`, retains the PDF
path, and never proves visibility. On failure, explain the diagnostic and provide
the path; do not claim the PDF opened. Do not use retry keys for viewer launches.

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
