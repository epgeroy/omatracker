---
name: omatracker
description: Set up OmaTracker clients/projects, track or correct time, customize invoice templates and images, issue PDF invoices, upload them to Google Drive, and diagnose dependencies using the local CLI.
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
4. For retryable ledger writes, assign a unique `--key` to the logical operation
   and reuse it only with identical arguments. Render/upload accept keys too.
5. Keep outputs focused: query a project/date range, request summaries, and follow
   pagination. Rust calculates amounts; do not recompute money in the model.

## Workflows

- **Setup:** `doctor`; help install missing Typst/rclone; guide `rclone config`
  browser authentication; `drive.configure`, `drive.check`. Use `drive.test` when
  asked to verify uploads. Detailed recipes: `references/workflows.md`.
- **Project:** inspect/set issuer and client profiles, create/configure the project,
  set rate/currency and intended effective date, timezone, cadence, template, logo,
  and payment terms. Clone settings with `copyFrom` when useful.
- **Time:** discover/create a task, start/stop by ID. Manual entries require actual
  dates, not a guessed current period. Subtraction targets an identified entry;
  ask which entry/date if ambiguous. Record reasons and use revisions for corrections.
- **Invoices:** summarize the selected range, create/refresh draft, preview PDF,
  issue when requested, render, then upload when requested. Report invoice number,
  amount/currency, local PDF path, and actual upload status/destination.
- **Recovery:** refresh stale drafts; inspect revision conflicts; retry rendering
  or uploading the existing invoice. Do not create another invoice to retry a PDF.

## Accounting rules

A rate makes work billable, including zero. No rate makes it non-billable. Recorded
time retains its historical rate; backdated entries use effective-dated history.
Existing pre-invoice entries require an explicit migration decision. Issued
invoices are immutable; void, correct, and prepare a linked replacement when asked.
Automatic monthly checks generate drafts only. `to` dates are exclusive and use
the project timezone. Different currencies remain separate invoices.

Read `tests/manual-invoices.md` for an isolated end-to-end exercise. Preview template
changes and use invoice data for billing, not the panel's current-rate counter estimate.
