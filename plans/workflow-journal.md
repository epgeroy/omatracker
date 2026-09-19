# Workflow journal v1 (Plan 02 / Plan 03)

Status: Implemented with `work.record-batch` in `src/workflows.rs`.
`request.keys` remains independent and never opens a ledger.

## Contract-review storage decision

The detailed journal uses `billing.requests[workflowKey]`, alongside (but logically
separate from) the per-mutation receipts. This reuses the atomic ledger storage and
backup paths, avoids another store of task names/time data outside clear-all, and
reserves workflow keys in the existing idempotency namespace. A minimal sidecar
binding supplies reset detection without retaining work content after a clear.
This supersedes Plan 02's proposed full-journal sidecar location.

The journal receipt's `fingerprint` is SHA-256 of compact canonical JSON containing
`action: work.record-batch`, `normalizationVersion: 1`, and normalized input.
Serde JSON's default sorted object maps supply recursive key ordering; arrays and
JSON types retain their order/types. Normalization trims validated task titles,
canonicalizes timestamps to UTC whole seconds and expands defaults. Monetary text
remains exact request text; money parsing/arithmetic belongs to the existing rate
and billing implementation.

The receipt `result` is the journal:

- `version: 1`; unsupported versions are rejected before step execution.
- `ledger`: canonical absolute selected path; `ledgerId`: persistent incarnation
  token assigned to `billing.ledgerId` on first workflow creation.
- `request`: normalized input, including explicit project ID and pricing intent.
- `steps`: ordered `item`, `action`, `key`, `input`, `taskStep`, `ready`, `result`.
  The item index plus action/position identifies the logical step. `taskStep` binds
  a dependent entry to the earlier create step's `data.id`; it never resolves names
  or UI selection. `result` retains IDs, revisions, and captured billing metadata.
- Pending steps have `ready: false` and no result. Ready steps have exact input
  durably saved and may have committed a write. Complete steps have validated
  results saved. `lastError` retains the latest failure when checkpointing succeeds.
- `completed` retains the validated final response and authoritative summary.

All keys and dependency bindings are stored before the first task/entry mutation.
Each resolved input is checkpointed before its first invocation; retries use the
same saved arguments/key. Changed normalized input is an idempotency conflict.

## Ledger identity, reset and lock ordering

Before executing any step, persist a binding at
`<ledger>.workflows/<SHA-256-of-workflow-key>.json`. It contains only `version: 1`
and `ledgerId`. Safe hashed filenames cannot traverse paths. Bindings survive
clear-all without retaining task names, input, results, or unhashed retry keys.
The detailed journal/receipts are cleared with the ledger and preserved only in
the clear operation's explicit backup.

Ordinary writes preserve `billing.ledgerId`. Clear-all invalidates it; the next
new workflow assigns a new token. A binding/token mismatch rejects same-path reset
or replacement. A moved/copied journal rejects another canonical path. A missing
receipt with an existing matching binding is `WORKFLOW_STATE_LOST`, not a fresh
workflow. Removing bindings intentionally discards reset detection. Exact restored
copies with the same token/path are indistinguishable: restore journals/receipts
consistently. Keys remain ledger-local; on a different ledger with no journal or
binding, the same string is a new key. Retain path and key together as the resume
identity. Path aliases resolve to the same canonical ledger.

An exclusive ledger-scoped `workflows-worker` lock serializes coordinators. Lock
ordering is workflow worker, then existing operation/ledger locks inside commands.
Clear-all acquires the workflow worker first. No ledger lock spans recursive
`agent::execute` calls, and unrelated normal writes can proceed between steps.

## Persistence and recovery ordering

1. Parse every field and validate the full plan/project/pricing/summary using the
   existing Rust mutations against an in-memory ledger copy. Dry run writes no
   journal, binding, ledger, migration or persistent keys (lock files may exist).
2. Under the workflow lock, check binding, version, fingerprint, path and token;
   load the existing journal or save the new complete plan and ledger token.
3. Every ledger checkpoint uses the existing temporary-file flush/`sync_all` and
   atomic rename, followed by a parent-directory sync. The binding uses the same
   atomic-write path plus directory syncs, including its new directory's parent.
   No task/entry executes until both journal and binding are durable.
4. Resolve the next step, save `ready` and exact input, then invoke existing mutation
   dispatch with its original key. Mutation and receipt commit atomically. Sync
   the ledger directory before depending on that receipt.
5. Validate returned ID/billing metadata and durably checkpoint the response before
   continuing. After interruption, replay ready steps through original receipts;
   do not generate replacement keys. Check removed targets on resume/completion.
6. Final target verification, project-range Rust summary and completion checkpoint
   share one ledger snapshot. Completed retries return the captured response.
7. Failures stop execution with partial item progress and the original error/resume
   identity. Receipt-confirmed but uncheckpointed writes count as `recordedSteps`.
   Journal-write failures also expose `journalError`; if storage cannot be read,
   report the in-memory checkpoint and reconcile through receipts on retry.

This is resumable coordination, not a transaction/rollback across the batch.

## Verification

`src/workflows/tests.rs` injects interruption at all nine steps' prepared, mutated
and checkpointed boundaries, plus plan, summary and final-response loss. Tests
cover journal-write failure, same-path clear/replacement, missing receipt, unsupported
version, unrelated writes/UI selection and failed steps. `tests/workflows.rs` covers
concurrent resumes, changed input, copied ledger, removed targets, preflight,
historical/zero rates, scoped explicit pricing and the six-task USD 1,200 scenario.
All use disposable ledgers; external regression tests use fake services.
