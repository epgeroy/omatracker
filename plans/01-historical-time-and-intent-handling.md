# Plan 01: Historical time and intent handling

Status: Implemented and verified September 19, 2026. Priority: P0. Skill/documentation work.

## Goal and evidence

Detect unexpected non-billable historical entries immediately and complete clearly
authorized corrections without asking the user to choose implementation details.

Source: OpenCode session `ses_f4504c0ecffeJwUJ521N9vjAGD`, September 19, 2026,
“Add Omatrack Demo Tracking Task”, model `openai/gpt-5.6-luna-fast`.

- Message 25 returned a three-hour historical entry with `billing.rate: null`.
- Message 26 reported successful recording without explaining its billing status.
- Message 40 created a USD 0.00 invoice with 10,800 non-billable seconds.
- Messages 51–57 added clarification and another unchanged preview before the
  user repeated that the existing time should be rated.
- Messages 60–67 finally priced the entry, refreshed the draft, and previewed USD 150.

## Scope and design

Keep the existing historical-rate accounting rules. A current project rate does
not establish a historical rate, and a zero rate is billable rather than absent.
The improvement is explicit result checking and better interpretation of user intent.

Use this decision table in the skill:

| Situation | Required behavior |
| --- | --- |
| Historical entry has a rate | Report recorded time and its actual billing status. |
| Rate is absent; no billing expectation was stated | Report the non-billable result accurately. |
| Rate is absent; billable work was expected but historical pricing is unclear | Ask one focused question about the historical rate/scope. |
| User explicitly requests pricing the existing unrated time | Apply the authorized rate with `applyExisting: true`; do not ask again. |
| User says “edit or recreate it” | Choose the history-preserving edit when it satisfies the request. |
| More than one task, entry, or rate is plausible | Resolve that ambiguity before changing historical billing. |

`applyExisting` affects all eligible unrated entries for a task, not one selected
interval. If the user authorizes only one slot and other eligible entries exist,
explain the scope mismatch rather than silently broadening it. Deleting a task
does not delete its dated entries and is not a billing correction strategy.

## Implementation steps

- [x] Update the time workflow to resolve dates/offsets from the project timezone
  and inspect every returned entry segment's `billing.rate` and `resolved` fields.
- [x] Add the decision table and a concrete “yesterday, 20:00–23:00” example to
  `skills/omatracker/references/workflows.md`.
- [x] Replace unconditional “ask whether old time should become billable” wording
  with “ask only when the conversation has not already established intent”.
- [x] Add a correction recipe: inspect task and eligible entries, obtain its
  `entityRevision`, apply the authorized rate, inspect applied IDs and skipped counts,
  refresh an existing draft using its revision, then preview.
- [x] Require a project/range `summary` before creating an invoice unless an
  equivalent, still-current summary is already available. Explain exclusions.
- [x] Explicitly distinguish task current-rate metadata from entry historical
  pricing. Never infer a correct historical invoice from `task.get` alone.
- [x] Add concise examples to `AGENT_API.md` and the manual invoice exercise.
- [x] Rebuild and verify the installed skill carries the updated recipes.

## Files and integration points

- `skills/omatracker/SKILL.md`
- `skills/omatracker/references/workflows.md`
- `AGENT_API.md`
- `tests/manual-invoices.md`
- Existing behavior to consult: `src/agent.rs`, `src/task_rates.rs`,
  `tests/task_rates.rs`, and `tests/invoices.rs`.
- Documentation distribution: `src/skills.rs::bundle` embeds the source files.

## Verification and acceptance criteria

Use disposable ledgers and controlled timestamps for these scenarios:

1. Create a project rate effective today; add yesterday's work. The assistant
   reports the absent historical rate before the user discovers a zero invoice.
2. Ask to make that existing work billable at USD 50/hour, allowing edit or recreate.
   The assistant edits, preserves the interval, and verifies USD 150 for three hours.
3. Repeat with an explicit zero rate, already-priced time, and invoiced time.
   Existing protections and skipped-entry reporting remain correct.
4. Include two eligible entries but authorize pricing only one. The assistant
   recognizes that task-wide backfill is too broad.
5. Change a billed draft's source. The assistant refreshes before previewing it.

Record clarification count, redundant previews, tool calls, and result accuracy.
Target: no repeated authorization question when rate and historical scope are clear,
and no unreported non-billable result when the request expects billable work.
For documentation-only changes, use scenario review/replay rather than tests that
assert prose. If runtime accounting changes become necessary, run focused
`cargo test --test task_rates --test invoices` regression checks.

## Dependencies and implementation preflight

Can ship independently. Coordinate final wording with
[Plan 04](04-operational-documentation-and-discovery.md) and preview behavior with
[Plan 05](05-preview-opening-and-template-validation.md).

Inspect existing uncommitted work before implementation. Refresh a stale GitNexus
index and run upstream impact analysis for every code symbol being changed; the
planning-time graph was three commits behind. Run graph change analysis before
any requested commit. Test skill installation in isolated HOME/XDG directories.

## Review and verification record

Implemented in `/tmp/opencode/omatracker-historical-time` on
`feat/historical-time-intent`, based on the committed prerequisite refactor.
Review confirmed the existing accounting API is sufficient. Clarified two contract
details: skipped entries are returned as counts, not IDs; task-wide backfill also
records/rates eligible elapsed running time and sets the future task-rate policy.
`billing.resolved` takes precedence when classifying historical billing.

- Refreshed the worktree's GitNexus index. Upstream impact for the embedding path
  `skills::bundle` was LOW: direct caller `skills::execute`, then `skills::run`;
  no indexed affected processes. Runtime accounting functions were not changed.
- `make check`: formatting, all 89 Rust tests, Clippy, template compilation, plugin
  validation, and QML checks passed. The command timeout interrupted UI execution;
  `make ui-check install-check` completed the remaining checks successfully (the
  existing Wayland-only popup case is skipped by the offscreen UI suite).
- Disposable CLI replay with fixed September 18/19 timestamps: 10,800 historical
  seconds initially non-billable despite the current project rate; explicit USD 50
  backfill preserved the entry/interval/duration, added audit metadata, and produced
  USD 150.00 in summary and the refreshed draft/PDF text. The correction used 10
  API calls including two `request.key` calls: `entry.list`, `task.get`,
  `request.key`, `task.rate`, `entry.list`, `summary`, `invoice.get`, `request.key`,
  `invoice.refresh`, `invoice.preview`.
- Replay also verified explicit zero remains billable, already-rated/issued/paid
  skips, pagination identifying two eligible entries with no write for single-slot
  authorization, mixed-rate segment reporting, and correction of a priced draft
  from USD 150.00 to USD 100.00 via refresh before preview. Existing Rust tests cover
  externally billed skips, unresolved migration, and running-timer preservation.
- Scenario review: clear historical authorization needs zero additional questions;
  missing rate or excess task-wide scope needs one focused question. The replay
  rendered only the two distinct corrected draft states: zero redundant previews.
  These are recipe review/API replay results, not measurements of an independent
  model session. The complete API replay used 76 calls, including retry-key creation.
- Rebuilt `bin/omatracker`; installed in isolated HOME/XDG directories. Bundled
  workflow/API/manual files matched the sources byte-for-byte; `SKILL.md` matched
  with the expected installation preamble, executable path was correct, and a
  repeated installation returned `unchanged`.
