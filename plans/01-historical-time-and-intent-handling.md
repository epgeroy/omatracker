# Plan 01: Historical time and intent handling

Status: Proposed. Priority: P0. Primarily skill/documentation work.

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

- [ ] Update the time workflow to resolve dates/offsets from the project timezone
  and inspect every returned entry segment's `billing.rate` and `resolved` fields.
- [ ] Add the decision table and a concrete “yesterday, 20:00–23:00” example to
  `skills/omatracker/references/workflows.md`.
- [ ] Replace unconditional “ask whether old time should become billable” wording
  with “ask only when the conversation has not already established intent”.
- [ ] Add a correction recipe: inspect task and eligible entries, obtain its
  `entityRevision`, apply the authorized rate, inspect applied/skipped entry IDs,
  refresh an existing draft using its revision, then preview.
- [ ] Require a project/range `summary` before creating an invoice unless an
  equivalent, still-current summary is already available. Explain exclusions.
- [ ] Explicitly distinguish task current-rate metadata from entry historical
  pricing. Never infer a correct historical invoice from `task.get` alone.
- [ ] Add concise examples to `AGENT_API.md` and the manual invoice exercise.
- [ ] Rebuild and verify the installed skill carries the updated recipes.

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
