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
