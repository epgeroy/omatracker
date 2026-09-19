# Workflow recipes

Read `AGENT_API.md` in the OmaTracker installation for the complete field contract.
All commands below are `bin/omatracker agent ACTION --input JSON`.

## New client and project

1. `issuer.get`; use `issuer.set` with complete `details` to record sender name,
   address, contact details, registration ID, and payment instructions.
2. `client.list`; reuse a matching client or `client.set` with complete `details`.
3. `project.create` with `name`, `client`, `rate`, `currency`, `effectiveAt`,
   `timezone`, `dueDays`, and optional `logo`. Do not invent historical effective dates.
4. `template.create` from `invoice`; get its path with `template.path`.
5. `template.asset` imports images and returns the reference for Typst `image()`.
6. Edit the template, `template.validate`, then `project.configure` with `template`.
7. Optionally `repository.bind` so future sessions can discover the project.

## Rename or delete tasks, projects, and clients

Discover the entity ID with the appropriate list/get action. Rename using
`task.update` (`id`, `name` or `title`), `project.update` (`project`, `name`), or
`client.update` (`id`, `name`). Client name updates preserve the other profile
fields; `client.set` instead replaces the entire profile. Use the target's
`entityRevision` from get/list for edits; the top-level numeric revision covers the
whole ledger and changes on unrelated operations.

Delete using `.remove` or `.delete` with the same target fields and a retry key.
Task removal records running time before removal. Project removal archives the
project, stops its timers and disables schedules, while retaining history for
invoices. Client removal is blocked while active projects still reference it:
reassign, unlink with `project.update` and `client: ""`, or remove the projects as
requested. Do not cascade-delete projects merely to remove a client. List archives
with `includeArchived: true`. Issued invoice names and original task-entry titles
remain captured; refresh drafts when current billing metadata changes.

Generate a fresh `agent request.key` before each new write and retain it for retries.
Do not reuse creation keys after deleting an entity. Recreate clients without the
old ID; use the new returned ID in new projects. If a creation key returns
`REQUEST_TARGET_REMOVED`, use a new key for the new intended creation. `--key auto`
is a convenience for new operations; its stderr/JSON gives the actual retry key.
An `IDEMPOTENCY_CONFLICT` is not fixed by altering arguments with the same key.

## Rates on existing tasks

Read `task.get`. Set `task.rate` with `id`, `rate`, `currency` and optionally its
`entityRevision`. This works independently of a project rate. Ask whether old
unrated time should also become billable: only then use `applyExisting: true`.
Inspect the returned applied entry IDs and skipped counts. Already-priced,
externally billed and issued/paid-invoice entries stay unchanged. The operation
records an audit trail and continues running timers. Refresh invoice drafts after
backfilling. Use `inheritRate: true` or `noRate: true` for future policy changes.

## Tracking and corrections

Use `task.list` and `task.create` as needed. Start/stop explicitly when requested;
do not infer that all agent execution is human billable work. Inspect running
tasks before suggesting stopping a timer.

For “add yesterday's work”, resolve the date in the project timezone and supply
an RFC3339 start/end or start/seconds. For subtraction, inspect `entry.list`, identify
the intended entry, and send `entry.correct` with signed seconds, revision, reason.
Use the returned correction ID with `entry.undo` and the current entry revision.

## Invoice preparation

1. `summary` for project/from/to. Explain unresolved or non-billable exclusions.
2. Stop a timer only if requested; active timer time is excluded from final invoices.
3. `invoice.create` for each currency. `invoice.preview` returns a PDF path.
4. When asked to finalize, `invoice.issue` with draft ID/revision and issue date.
5. `invoice.render` gets the issued PDF. `invoice.upload` transfers that PDF to Drive.
6. Report local success separately from upload success. Uploads do not mean paid.

If the source changes, `invoice.refresh` then preview again. For an issued invoice
correction: `invoice.void` with reason, correct entries, `invoice.reissue`, preview,
issue. This preserves a link to the original and never reuses its number.

## Dependencies and migration

`doctor` supplies dependency status and install hints. On Omarchy, missing tools
can be installed with `sudo pacman -S typst rclone`. Use the user's package manager
on other distributions. Browser OAuth is completed by the user through rclone.
`drive.check` verifies access; `drive.test` explicitly writes a small test file.

Before working on old history, inspect `migration.preview`. Ask for the intended
historical rates/no-rate and whether time was already billed; then resolve ranges.
Do not reinterpret archived report PDFs as numbered invoices or proof of payment.
