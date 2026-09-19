# Plan 03: Bulk dated-work helper

Status: Implemented. Priority: P1. Domain-specific CLI workflow.

## Contract review decisions

- `work.record-batch` supports 1–50 new tasks and up to 200 dated intervals,
  with required explicit/historical-inheritance pricing and a project-range summary.
- Explicit pricing is entry-scoped at creation, using `entry.add.pricing`; it
  handles already-rated history without backfill skips or future task overrides.
- Plan 02's journal ships with this consumer in `src/workflows.rs`. The detailed
  journal is stored in the ledger receipt map; a content-free sidecar ledger-token
  binding survives clear/reset to reject stale workflow identities. See the revised
  [journal contract](workflow-journal.md) for the storage decision and ordering.
- Full preflight simulates existing mutation/billing paths on an in-memory copy.
  Recovery uses retained step keys, explicit project targeting and no global
  revision guard. Partial errors retain successful work and expose resume state.

## Goal and evidence

Let an agent submit a set of tasks and dated work in one structured request, with
local handling of returned IDs, pricing, retries, and final verification.

Source: OpenCode session `ses_f4504c0ecffeJwUJ521N9vjAGD`, messages 71–79.
Six tasks took 37 tool calls: 18 key-generation calls, six task creations, six
entry additions, six rate applications, and one summary. Each six-call group was
already parallelized. The response took approximately 85 seconds, with about two
seconds spent inside tools after overlapping intervals were merged.

## Implemented contract

The domain workflow is `work.record-batch`. The complete supported contract,
bounds and retry semantics are documented in `AGENT_API.md`.

```json
{
  "project": "PROJECT_ID",
  "items": [
    {
      "ref": "webhook-mapping",
      "newTask": {"title": "Webhook event mapping"},
      "entries": [
        {"start": "2026-09-01T09:00:00Z", "end": "2026-09-01T13:00:00Z"}
      ]
    }
  ],
  "pricing": {"mode": "explicit", "rate": "50", "currency": "USD"},
  "summary": {"from": "2026-09-01", "to": "2026-09-16"}
}
```

The initial version creates new tasks only. This keeps any explicit pricing
adjustment scoped to the entries created for those tasks; extending it to existing
tasks requires an entry-scoped pricing design first.

Require a stable workflow retry identity, using Plan 02's retained-key/journal
mechanism. Support explicit pricing and historical-inheritance modes. Reject a
missing or ambiguous pricing mode rather than deciding billing intent implicitly.
Do not silently change the project's historical rate or future rate policy.

Explicit pricing uses scoped entry pricing rather than `task.rate` backfill.
There is no task-rate override or future-policy side effect.

## Execution and recovery model

1. Parse and validate every item, unique reference, timestamp, duration, pricing
   mode, and project before the first mutation. Apply documented batch-size limits.
2. Support a validation-only dry run that returns planned steps without recording
   tasks/time. Any historical-rate uncertainty must be visible in that result.
3. Persist normalized input and all per-step retry keys before execution.
4. For each item, create its task, carry the returned ID forward, add dated entries,
   and apply only the requested pricing. Inspect returned billing metadata.
5. Execute sequentially inside the helper initially. The observed bottleneck was
   model round trips, so add internal concurrency only if measurement justifies it.
6. Stop on a failed step, report completed/pending items, and retain resume state.
   Do not delete successfully recorded work to simulate an atomic rollback.
7. Return item results and one authoritative Rust-calculated range summary.

The batch is resumable, not an all-or-nothing transaction. Reuse existing mutation
and receipt paths; avoid holding a ledger lock while recursively invoking a path
that acquires the same lock. Keep new workflow coordination separate from billing
calculations.

## Implementation steps and files

- [x] Finalize request/response fields, bounds, future-rate semantics, and partial
  failure behavior in `AGENT_API.md` before implementation.
- [x] Add dispatch in `src/agent.rs` and a dedicated workflow module if appropriate;
  wire it through `src/lib.rs` rather than expanding unrelated UI code.
- [x] Integrate Plan 02's durable workflow journal and exact-retry behavior.
- [x] Reuse `src/task_rates.rs` and `src/billing.rs` semantics after checking their
  current APIs and graph impact. Do not duplicate money arithmetic.
- [x] Return `ref`, task/entry IDs, billing outcome, skipped adjustments, completion
  status, resume identity, and summary. Make verbose detail opt-in if necessary.
- [x] Add a “many dated tasks” recipe to `skills/omatracker/references/workflows.md`.
- [x] Add focused integration tests in `tests/workflows.rs` and recovery tests in
  `src/workflows/tests.rs`.
- [x] Update embedded skill delivery and installation checks as required.

## Verification and acceptance criteria

- Replay the six-task September 1–15 scenario on a disposable ledger: six tasks,
  24 recorded hours, USD 1,200 billable, no unintended non-billable exclusions.
- The agent can submit the batch with one execution call after discovery/key
  preparation, rather than issuing 37 low-level tool calls. The helper returns the
  summary, so a second verification call is needed only when results warrant it.
- Invalid timestamps or one malformed item fail preflight before creating tasks.
- Mixed historical rates, zero rates, and intervals crossing rate boundaries
  preserve existing accounting behavior in historical-inheritance mode.
- Replaying a completed request produces no duplicates; changed input under the
  same workflow identity is rejected.
- Inject interruption between every mutation and journal update. Resume completes
  the intended work exactly once and reports partial state accurately.
- Concurrent unrelated ledger changes do not cause inappropriate global-revision
  failures or redirect the explicit project target.

Run new workflow tests with existing task-rate, invoice, and lifecycle regression
tests. Measure tool calls, model rounds, helper wall time, and correctness separately.
Do not promise an elapsed-time speedup solely from fewer calls.

Verification: full `make check` passes on the combined Plan 01/02/03/05 tree:
115 Rust tests, formatting, Clippy (`-D warnings`), five Typst fixtures, plugin
validation, QML service tests, UI tests and isolated installation/embedded docs.
The additional `python tests/ui-check.py --wayland` passes the popup case skipped
by the headless suite. The six-task integration case verifies 86,400 seconds,
USD 120,000 minor units, no non-billable/unresolved exclusions and duplicate-free
replay. Recovery covers all 27 prepared/mutated/checkpointed step windows, final
response loss, storage failure, reset/replacement and concurrent resumes.

Call accounting: one workflow execution call after discovery/key preparation;
12 internal mutations for six single-entry tasks (entry pricing is atomic with
entry creation). This replaces per-task model interaction with one execution
round. The test reports helper wall time separately; model latency is not measured
and no end-to-end elapsed-time speedup is claimed.

## Dependencies and implementation preflight

The shared recovery contract is [workflow-journal.md](workflow-journal.md), including
schema versioning, ledger identity, input fingerprints, persistence ordering and
uncertain-step replay. Implement and test it alongside this first consumer.

Depends on [Plan 01](01-historical-time-and-intent-handling.md) for pricing intent
and [Plan 02](02-retry-keys-and-round-trips.md) for durable workflow recovery.
Coordinate discoverability with [Plan 04](04-operational-documentation-and-discovery.md).

Review existing uncommitted code, refresh stale GitNexus data, and run upstream
impact analysis on affected dispatch, billing, and rate symbols before editing.
Inspect direct callers and preserve their contracts. Analyze graph changes before
any requested commit. All test work uses isolated storage and fake remotes.
