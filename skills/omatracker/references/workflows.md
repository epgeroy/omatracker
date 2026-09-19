# Workflow recipes

Read `AGENT_API.md` in the OmaTracker installation for the complete field contract.
All commands below are `bin/omatracker agent ACTION --input JSON`.

## Prepare keys together, execute dependencies in order

Use `request.keys` with `{"labels":["invoice:create","invoice:issue","invoice:render","invoice:upload"]}`
to prepare all four keys in one call. The response is a label-to-key mapping at
`data.keys`. Labels must be unique, 1–80 UTF-8 bytes, without surrounding whitespace
or control characters; a batch contains 1–64 labels. Preserve labels exactly.
Each invocation returns fresh keys and does not access a ledger; omit `--key`.

Allocate keys for later steps now, even if an earlier mutation must return their
target ID. For six dated tasks, prepare 18 labels in one batch: `task-1:create`,
`task-1:add-entry`, `task-1:price-entry`, and the same three labels for tasks 2–6.
Do not wait for each create/add/price stage just to generate its next keys. Collect
independent discovery results (project, issuer, client, range summary) in the same
tool round as preparation. If help advertises only `request.key`, group those
independent calls in one round instead.

Retain the selected ledger, step-to-key mapping, and each exact action/input before
starting writes. Use the create key for create, validate its returned ID, then resolve
and retain the entry input with that ID before using the entry key. Validate billing
metadata and revisions before the dependent pricing step. Key preparation may be
grouped; mutations with dependencies must still wait for validated results.

If generation was interrupted and none of its keys has been used, prepare again.
After any write starts, replay uncertain steps with their original resolved key and
exact input; do not rerun `request.keys` or `--key auto` to replace those keys. An
idempotency conflict means the request changed; inspect it rather than silently
starting another write. A retained key is not enough if the original arguments were
lost. For many new dated tasks, `work.record-batch` persists a durable workflow
journal before its first task/entry mutation; it does not make a batch atomic.

For invoices this reduces four dedicated preparation rounds to at most one; for six
tasks it reduces three six-call generation stages to one batch call. Report model
rounds separately from internal CLI calls: the underlying writes still run.

## New client and project

1. `issuer.get`; use `issuer.set` with complete `details` to record sender name,
   address, contact details, registration ID, and payment instructions.
2. `client.list`; reuse a matching client or `client.set` with complete `details`.
3. `project.create` with `name`, `client`, `rate`, `currency`, `effectiveAt`,
   `timezone`, `dueDays`, and optional `logo`. Do not invent historical effective dates.
4. `template.list`; inspect one suitable existing template first. If needed,
   `template.create` from `invoice`; get its path with `template.path`.
5. `template.asset` imports images and returns the reference for Typst `image()`.
6. Edit the template, `template.validate`, then `project.configure` with `template`.
   Render a real draft and read the PDF after edits; compilation is not layout review.
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

Prepare fresh keys together with `agent request.keys` and retain each for retries.
Do not reuse creation keys after deleting an entity. Recreate clients without the
old ID; use the new returned ID in new projects. If a creation key returns
`REQUEST_TARGET_REMOVED`, use a new key for the new intended creation. `--key auto`
is a convenience for new operations; its stderr/JSON gives the actual retry key.
An `IDEMPOTENCY_CONFLICT` is not fixed by altering arguments with the same key.

## Rates on existing tasks

Read `task.get`. Set `task.rate` with `id`, `rate`, `currency` and its
`entityRevision`. This works independently of a project rate and defaults to new
work only. Ask about pricing old time only when the conversation has not already
established the historical rate and scope. Explicit authorization to price existing
unrated time calls for `applyExisting: true`, without another authorization question.
Use `inheritRate: true` or `noRate: true` for future policy changes.

`task.get` exposes current-rate metadata and override history; it does not prove
that recorded entries have that rate. Check `entry.list` and `summary` for historical
pricing. A current project rate does not establish a historical rate either.

### Historical time decision table

Check `billing.resolved` first: unresolved billing is excluded until explicitly
resolved, not silently classified as non-billable. For resolved historical entries:

| Situation | Required behavior |
| --- | --- |
| Historical entry has a rate | Report recorded time and its actual billing status; zero is billable. |
| Rate is absent; no billing expectation was stated | Report the non-billable result accurately. |
| Rate is absent; billable work was expected but historical pricing is unclear | Ask one focused question about the historical rate/scope. |
| User explicitly requests pricing the existing unrated time | Apply the authorized rate with `applyExisting: true` after checking its scope; do not ask again. |
| User says “edit or recreate it” | Choose the history-preserving edit when it satisfies the request. |
| More than one task, entry, or rate is plausible | Resolve that ambiguity before changing historical billing. |

### Correct existing unrated time

1. Identify the task and authorized rate/currency and historical scope from the
   conversation. Read `task.get` and retain its `entityRevision`.
2. Inspect **all dates** with `entry.list` for the project, following `nextOffset`
   until complete and selecting rows by `entry.taskId`. The action's `id` filter
   selects an entry, not a task. Inspect billing and, where relevant, issued/paid
   invoice allocations via `invoice.list`/`invoice.get`. Eligible entries have
   positive seconds, no rate, no external-billing marker and no issued/paid allocation.
   Explicit backfill can also resolve previously unresolved entries.
3. Compare that eligible set with the authorized scope. `applyExisting` is task-wide
   across all recorded dates; neither `effectiveAt` nor an invoice range narrows it.
   A running timer's elapsed time is recorded, backfilled if eligible, and the timer
   continues. If only one slot was authorized and other eligible time exists, explain
   the mismatch and ask whether that wider scope is intended; do not silently expand
   it. There is no entry-ID-scoped rate edit. Deleting a task retains its dated entries
   and is not a billing correction strategy. This operation also sets the task's
   future rate; resolve any explicit historical-only/future-policy constraint first.
4. Once scope is established, call `task.rate` with `id`, `rate`, `currency`,
   `applyExisting: true`, the task's `entityRevision`, an explanatory `reason`, and
   a fresh retained retry key. Inspect `rateChange.appliedEntryIds`, `adjustmentId`,
   and `skipped.alreadyRated`, `skipped.invoiced`, `skipped.externallyBilled`.
   Skips are counts, not IDs; use inspected entries/allocations to identify them
   when needed. Already-priced time (including zero), externally billed time, and
   issued/paid allocations stay unchanged. Report the actual applied/skipped result.
5. Verify the entries' intervals/durations were preserved and their billing/audit
   metadata changed as intended. Run a fresh project/range `summary` to check the
   backend-calculated amount and exclusions; do not infer success from `task.get`.
6. For an existing draft, get its current numeric `revision` with `invoice.get`,
   then `invoice.refresh` with that ID/revision and a fresh retry key. Check the
   returned total/exclusions before `invoice.preview`. Preview alone does not refresh
   billing. Do not create a replacement draft or repeat an unchanged preview to fix
   source pricing. Issued invoices require the void/correction/reissue workflow.

## Tracking and corrections

Use `task.list` and `task.create` as needed. Start/stop explicitly when requested;
do not infer that all agent execution is human billable work. Inspect running
tasks before suggesting stopping a timer.

For “add yesterday's work”, get the project's `billing.timezone` with `project.get`.
Resolve the calendar date relative to now in that timezone, and the UTC offset at
each endpoint on that date (including daylight-saving changes), then supply RFC3339
start/end or start/seconds. If a local time is ambiguous or nonexistent, resolve it
before writing; do not use the machine's timezone or today's offset by default.

After `entry.add`, inspect **every** `data.entries[]` segment's `entry`,
`billing.rate`, and `billing.resolved`: sessions may split at rate boundaries.
Report mixed statuses separately. `resolved: false` means unresolved/excluded;
resolved with `rate: null` means non-billable; a present rate, including zero, means
billable. Report unexpected non-billable time immediately and use the decision table
above rather than waiting for the user to discover an empty invoice.

Example: with local “today” fixed at September 19, 2026 in `Europe/London`,
“yesterday, 20:00–23:00” means `2026-09-18T20:00:00+01:00` through
`2026-09-18T23:00:00+01:00`. If the project rate first became effective on
September 19, the returned 10,800 seconds have no historical rate. Say “Recorded
3 hours for September 18, 20:00–23:00 Europe/London; this time is non-billable
because no rate covered that interval.” If billing was expected but the rate is
unknown, ask “What hourly rate and currency should apply to those three hours?”
If the user instead says “Make that existing work billable at USD 50/hour; edit or
recreate it”, use the correction recipe immediately once the scope check matches.
Preserve the interval, verify `summary` reports `USD 150.00`, refresh any existing
draft using its revision, and preview the corrected draft once.

For subtraction, inspect `entry.list`, identify
the intended entry, and send `entry.correct` with signed seconds, revision, reason.
Use the returned correction ID with `entry.undo` and the current entry revision.

## Many dated tasks in one request

Discover the explicit project ID and resolve all dates/timezones and pricing intent
first. Generate one `request.key`, retaining it with the ledger path and JSON input.
For independent operations outside a batch, prepare known step keys together rather
than waiting for dependent IDs just to generate the next key.

Submit `agent work.record-batch --input-file batch.json --key RETAINED_KEY`:

```json
{
  "project": "PROJECT_ID",
  "items": [
    {"ref":"mapping","newTask":{"title":"Webhook mapping"},
     "entries":[{"start":"2026-09-01T09:00:00Z","end":"2026-09-01T13:00:00Z"}]},
    {"ref":"testing","newTask":{"title":"Webhook tests"},
     "entries":[{"start":"2026-09-15T09:00:00Z","end":"2026-09-15T13:00:00Z"}]}
  ],
  "pricing":{"mode":"explicit","rate":"50","currency":"USD"},
  "summary":{"from":"2026-09-01","to":"2026-09-16"}
}
```

Use `{"mode":"historical-inheritance"}` to honor effective-dated history instead.
Explicit pricing applies only to submitted entries, even if history has another
rate; future task/project rates are unchanged. Zero is billable at zero. The batch
supports new tasks only, up to 50 items and 200 total entries, each at most 31 days.
For validation only, add `dryRun: true` (no key required) and inspect billing
segments and summary exclusions. Remove it for execution. A dry run does not pin
historical rates; each actual entry captures history when written.

The helper creates tasks, carries IDs locally, records/prices entries, and returns
one project-range summary. Inspect `items`, `skippedAdjustments`, `summary.excluded`,
and each currency's amount; the range may include work outside this batch. Six
four-hour entries at USD 50 should report 24 hours and USD 1,200 on an empty range.
There is no need for separate task/entry/rate calls or a routine second summary.

On failure, inspect `data.status: partial`, item progress, `failedStep`, and
`resume`. Completed and receipt-confirmed `recordedSteps` remain saved. Resolve the
cause and resend the same input with the same key against the same ledger; do not
delete successes, invent replacement keys, or change input to evade a conflict.
Completed retries return the captured summary; query a fresh summary only when
later changes or unexpected exclusions warrant it. Fewer model/tool round trips
do not by themselves establish an elapsed-time speedup.

## Invoice preparation

Prepare create/issue/render/upload keys together as described above; use each only
when its step is requested and its dependencies have been validated.

1. Require `summary` for the exact project/from/to before creating an invoice,
   unless an equivalent, still-current summary is already available. `to` is
   exclusive in the project timezone. Rerun after changes to entries, historical
   billing, allocations, or the range; an old preview or current task rate is not
   an equivalent summary. Explain `excluded` non-billable, unresolved, already-billed
   time and running timers. If expected billable work is excluded, apply an already
   authorized correction or resolve the unclear rate/scope before creating a draft.
2. Stop a timer only if requested; active timer time is excluded from final invoices.
3. `invoice.create` for each currency. `invoice.preview` returns a PDF path.
4. When asked to finalize, `invoice.issue` with draft ID/revision and issue date.
5. `invoice.render` gets the issued PDF. `invoice.upload` transfers that PDF to Drive.
6. Report local success separately from upload success. Uploads do not mean paid.

If the source changes, get the draft's current revision, `invoice.refresh` with
that revision, then preview again. For an issued invoice
correction: `invoice.void` with reason, correct entries, `invoice.reissue`, preview,
issue. This preserves a link to the original and never reuses its number.

## Showing and reviewing previews

| Request | Workflow |
| --- | --- |
| Generate/export | Render; return path, amount/currency, period and exclusions. |
| Show/open | Get current PDF and call `artifact.open` with `path` in the same turn. |
| Review/check layout | Inspect the rendered PDF and report findings. |
| Show the same unchanged preview | Reuse its existing path when inputs are known unchanged. |
| Show draft after source billing changes | Refresh draft, render, then open. |
| Show issued invoice | Render/open its immutable captured original. |

Example: `artifact.open --input '{"path":"/absolute/path/preview.pdf"}'` (with the
usual `bin/omatracker agent` prefix). Check `data.status`: `launch_requested` only
means dispatch succeeded or is still running, not that a window is visible.
`launch_failed` includes a diagnostic and the usable path. Avoid blocking shell
opener calls; a background `&` alone can leave inherited tool output pipes open.

`invoice.preview` renders the saved draft; it is not a billing refresh. After
entries, historical rates, client/project billing settings or allocations change,
use `invoice.refresh` with the current draft revision before previewing. A current
project rate does not rewrite historical entry rates. Template source/imported
asset edits require a new preview even if the draft revision is unchanged; logo
or project appearance changes also require refreshing captured draft settings.
Never refresh an issued invoice: `invoice.render` uses its captured original.

Reuse is session-level: remember the path and the relevant unchanged inputs, and
check the file still exists. Invoice ID or draft revision alone is insufficient.
When freshness is uncertain or the file was removed, regenerate. Do not repeat an
unchanged zero-total render just to reopen it; report the zero/exclusions instead.

After a template change, **read the rendered PDF**. Check footer literal text,
client/issuer, logo, service and due dates, amounts, long lines, pagination and
clipping. For unchanged templates, inspect new previews when content differences
can affect layout. Inspect the final changed preview before issue. If inspection
is unavailable, say it has only compiled/rendered and leave visual review pending.

`template.validate` compiles representative invoice/report fixtures, not the
current draft. `valid: true` means compilation passed. `checks.text` reports
optional `pdftotext` heuristics; missing extractor means `skipped`, not passed.
Warnings can find leaked `text(...)`/`h(...)` or missing expected fixture text but
cannot prove layout quality. `checks.visual.status` remains `not_performed`.
Typst content blocks need `#` before function calls **and** expressions:

```typst
#let render(data) = {
  set page(footer: [
    #text(size: 9pt)[#data.issuer.name]
    #h(1fr)
    #text(size: 9pt)[#data.invoice.totalText]
  ])
  [#data.client.name — #data.invoice.totalText]
}
```

Without those prefixes, markup can compile as literal visible text.

## Reusable branding and provenance

Prefer one suitable template before reading all alternatives. If asked to match a
website, visit and verify the supplied URL. If it cannot be checked, say so; local
logo reuse is not verified website matching. Never infer a logo's origin from its
filename. Record only established facts in an optional `metadata.json` beside
`template.typ`: `client`, `sourceUrl`, `logoOrigin`, `palette`, `reviewDate`.
Omit unknown fields; set reviewDate only after actual rendered inspection, using
the date of that review. Describe logoOrigin as the verified URL or the actual
local asset path and note if the website was unverified. This sidecar is workflow
documentation, not a freshness key; existing metadata-free templates still work.

## Dependencies and migration

`doctor` supplies dependency status and install hints. On Omarchy, missing tools
can be installed with `sudo pacman -S typst rclone`. Use the user's package manager
on other distributions. Browser OAuth is completed by the user through rclone.
`drive.check` verifies access; `drive.test` explicitly writes a small test file.

Before working on old history, inspect `migration.preview`. Ask for the intended
historical rates/no-rate and whether time was already billed; then resolve ranges.
Do not reinterpret archived report PDFs as numbered invoices or proof of payment.
