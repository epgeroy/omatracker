# Plan 02: Retry keys and model round trips

Status: Phases A/B implemented; Phase C implemented with the Plan 03 consumer.
Priority: P1. Skill improvements followed by a small CLI extension.

## Goal and evidence

Keep durable, exact-retry behavior while eliminating model turns used only to
generate one key at a time.

Source: OpenCode session `ses_f4504c0ecffeJwUJ521N9vjAGD`, September 19, 2026.
There were 33 `request.key` calls among 112 tool calls, across 17 key-generation
rounds. The final invoice sequence requested separate keys before create, issue,
render, and upload. The six-task workflow generated three separate groups of six.

## Design

### Phase A: Group key preparation using the current interface

Document preparing independent keys together, including keys for later dependent
steps whose identity is already known. Generation does not need to wait for the
previous mutation. Associate each returned key with a logical step, not an entity
name alone. Collect independent discovery results in the same tool round.

### Phase B: Add one bounded multi-key action

Proposed new action, not an existing API:

```json
{"action":"request.keys","input":{"labels":["create-task","add-entry","price-entry"]}}
```

Return a label-to-fresh-key mapping in the normal versioned response envelope.
Require unique, bounded labels and a bounded item count. Preserve `request.key`.
Like the existing single-key action, this should not open or mutate a ledger.
An interrupted generation can be repeated because no write has used its keys yet.

Once a mutation starts, retain and reuse its original resolved key and exact
arguments. `--key auto` remains a convenience, not a blanket retry strategy.

### Phase C: Reuse a durable execution journal for local workflows

For the bulk helper in Plan 03, prepare a journal before the first mutation:

- workflow identity and selected ledger identity;
- normalized request and its fingerprint;
- per-step key, arguments, dependencies, returned IDs/revisions, and completion;
- enough state to resume after a lost response without generating replacement keys.

Write the plan/keys durably before executing. On interruption, replay uncertain
steps through the existing receipt mechanism. Do not call the next mutation until
the prior result has been validated. A workflow journal coordinates steps; it
does not replace ledger receipts or claim multi-command atomicity.

## Implementation steps

- [x] Document grouped preparation and dependency-aware execution in the skill.
- [x] Inspect `src/agent.rs` key generation, request parsing, receipt lookup, and
  `--key auto` behavior; capture current guarantees in focused regression cases.
- [x] Add `request.keys`, validation, help advertisement, and request/response docs.
- [x] Return compact labeled results rather than repeating full request metadata.
- [x] Define the reusable journal format with Plan 03, including versioning,
  same-ledger resume checks, persistence ordering, and changed-input rejection.
- [x] Implement journal support alongside its first real workflow consumer in
  Plan 03, avoiding an unused generic execution framework.
- [x] Update embedded skill references and isolated installation coverage.

The Phase C implementation contract is in [workflow-journal.md](workflow-journal.md).
Plan 03 adds the persistent ledger-incarnation identity, directory-sync barriers,
durable journal and interrupted-write/changed-input/wrong-ledger recovery tests.
The reviewed implementation keeps detailed journals in the ledger receipt store
and content-free reset-detection bindings in a ledger-scoped sidecar directory.

## Files and integration points

- `src/agent.rs`: action registry, dispatch, key handling, receipt semantics.
- `AGENT_API.md`: new action contract and retry examples.
- `skills/omatracker/SKILL.md` and `references/workflows.md` beneath that skill.
- `tests/invoices.rs`, `tests/lifecycle_conflicts.rs`: existing retry/concurrency
  behavior to preserve; add a focused key test file if that is clearer.
- `src/skills.rs`, `tests/skills.rs`: delivery of updated documentation.
- Workflow journal implementation location to be selected with Plan 03 after
  reviewing the existing storage/locking boundaries.

## Verification and acceptance criteria

- Key generation produces distinct nonempty keys and rejects duplicate labels,
  empty batches, and requests beyond the documented bound.
- Key-only actions do not create, migrate, or lock a ledger.
- Exact write retries do not duplicate tasks, entries, invoices, or uploads.
- Different arguments with a previously used key remain an idempotency conflict.
- Deleted/recreated targets retain existing `REQUEST_TARGET_REMOVED` behavior.
- Interruption after a write but before its response is journaled resumes through
  the original key; it does not repeat the logical operation with a new key.
- Resume against another ledger or altered workflow arguments is rejected clearly.

For the final invoice scenario, target at most one dedicated key-preparation
round instead of four. For six tasks, prepare the known keys together instead of
three six-call generation stages. Report model rounds separately from internal
CLI calls; grouped execution does not make internal work disappear.

Run targeted retry/invoice/lifecycle tests and new key/journal tests, followed by
`cargo fmt --check` and the project's applicable Rust checks. Use fake external
services and disposable ledgers for recovery tests.

Implementation verification: focused key/invoice/lifecycle/installed-skill tests
and the full `make check` passed (Rust tests, formatting, Clippy with warnings
denied, Typst templates, plugin validation, QML service/UI and isolated installation
checks). The headless UI suite skips its existing Wayland-only popup case.

## Dependencies and implementation preflight

Phase A is independent. Phase B can ship before
[Plan 03](03-bulk-dated-work-helper.md); Phase C ships with its consumer.
[Plan 04](04-operational-documentation-and-discovery.md) owns the final navigation.

Before runtime edits, refresh stale graph data and run upstream impact analysis
on dispatch and receipt helpers. Existing uncommitted changes in these areas must
be reviewed first. Preserve external-operation worker locks and per-entity revision
guards. Run graph change analysis before any requested commit.
