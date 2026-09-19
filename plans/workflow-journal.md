# Workflow journal v1 contract (Plan 02 / Plan 03)

Status: Design contract for Plan 03's first workflow consumer. Not a currently
implemented API or a generic execution framework. `request.keys` is available
independently and does not persist a journal.

## Persistent record

The consumer owns a versioned JSON journal, separate from the ledger receipts:

- `schemaVersion: 1`; reject unsupported versions before execution.
- `workflowId`: stable caller-supplied retry identity, plus `action` and the
  workflow/normalization version. Reusing this identity selects the existing journal.
- `ledger`: canonical absolute selected path and a persistent ledger-incarnation
  token. Resolve aliases before comparing. Another path or a reset/replaced ledger
  must fail resume, even if it has the same project names or revisions.
- `request`: normalized workflow input with defaults made explicit, resolved project
  identity, pricing intent, and preserved ordered items. `fingerprint`: SHA-256 of
  canonical JSON containing action, normalization version, and normalized input.
  Canonicalization recursively sorts object keys, preserves array order and JSON
  types, and uses UTF-8 compact JSON. Persist the normalized input as well as its hash.
- `steps`: ordered records with unique logical `label`, `action`, `key`, `dependsOn`,
  input bindings, resolved `input` when available, `status`, and validated `result`.
  Bindings name prior steps and explicit returned ID/revision fields; they must not
  re-resolve a project from panel selection or resolve a task by display name.
- `status`/progress: pending, running, failed or complete, with completed/pending
  steps and the last error. Results retain needed IDs, revisions, billing metadata,
  and the final authoritative summary. A failed/uncertain write retains its key/input.

Before the first mutation, persist the entire dependency plan and all keys. Inputs
that depend on returned IDs are represented as bindings initially; persist each
fully resolved action/input **before that step's first invocation**, then freeze it.
Per-step status distinguishes `pending` (never invoked), `ready` (resolved input
durably saved; execution may be uncertain), and `complete` (validated result saved).

## Ledger identity and storage boundaries

Current storage has no persistent ledger-incarnation token. Plan 03 must introduce
one with its consumer, retain it across ordinary atomic ledger replacements, and
rotate/invalidate it on clear/reset. Canonical path alone, inode, modification time,
global revision, or a content hash cannot be a stable ledger identity. Revisions and
content legitimately change during work. Do not advertise same-ledger recovery until
reset/replacement rejection is tested. Exact cloned ledgers with the same token and
path are indistinguishable; copying/restoring storage must preserve journal/receipts
consistently or explicitly invalidate resumability.

Place journals in a ledger-scoped sidecar directory selected by Plan 03, with a
safe filename derived from the workflow identity (not arbitrary path components).
Use an exclusive per-workflow lock across load/validate/execute/update. Lock ordering:
workflow lock, then existing operation/ledger locks inside each command. Never hold
a ledger lock while recursively invoking `agent::execute`. Preserve external-worker
locks and per-entity revision checks. Different workflows still use normal receipts.

## Persistence and replay ordering

1. Validate the full workflow, dependency plan and pricing intent before any writes.
   Validation-only dry run creates neither a journal nor keys/ledger mutations.
2. Under the workflow lock, load or create the journal. On resume, compare version,
   workflow action, selected ledger identity, normalized input and fingerprint.
   Reject mismatches clearly before executing any step; never overwrite old state.
3. Persist the plan/keys to a temporary file in the destination directory, flush and
   `sync_all` the file, atomically rename, then sync the parent directory. Stop if any
   persistence operation fails. Current `atomic_write` syncs the file but not the
   parent directory; the consumer must address this durability gap before relying on
   rename durability. Account for initial directory creation too.
4. Resolve a ready step from validated dependencies. Persist its exact input and
   `ready` status with the same durability ordering, then invoke existing mutation
   dispatch with its original key. Ledger mutation and receipt remain atomic there.
5. Validate the response, save the returned IDs/revisions/results and completion
   durably, and only then resolve and execute the next dependent mutation.
6. After interruption between write and journal update, replay any ready/uncertain
   step through existing receipts with its original action/input/key. A receipt
   conflict or removed target halts recovery; generating a replacement key would
   create another logical operation. External receipts retain pinned destinations;
   recovery may repeat checksum upload attempts, not promise exactly-once transport.
7. Report partial progress on failure. Retain the journal; never delete successful
   work to simulate rollback. Completed workflow replay returns the saved validated
   result. This coordinates separate commands and does not provide atomic batches.

## Required tests with the Plan 03 consumer

Inject interruption after every mutation but before recording its response; resume
must use original keys and create no duplicate tasks, entries or pricing effects.
Also test journal-write failures, concurrent resumes, changed normalized input,
another ledger, same-path reset, unsupported versions, removed targets, and a lost
final response. Use disposable ledgers and fake external services. These tests ship
with actual journal execution; Plan 02 covers key generation and receipt regressions.
