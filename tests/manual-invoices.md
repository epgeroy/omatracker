# Manual acceptance: agent-first invoices

Build with `make backend`. Run this in the repository root. These checks use a
disposable ledger and configuration; no existing tracking data is changed.

```bash
backend="$PWD/bin/omatracker"
sandbox="$(mktemp -d)"
mkdir -p "$sandbox/home" "$sandbox/config" "$sandbox/data" "$sandbox/cache" "$sandbox/state"
tracker() {
  HOME="$sandbox/home" XDG_CONFIG_HOME="$sandbox/config" \
    XDG_DATA_HOME="$sandbox/data" XDG_CACHE_HOME="$sandbox/cache" XDG_STATE_HOME="$sandbox/state" \
    "$backend" --data-path "$sandbox/ledger.json" "$@"
}
tracker agent help
tracker agent doctor
```

Commands always return JSON. Substitute the returned IDs/revisions for the
uppercase placeholders below. `--key` identifies a logical operation; reuse it
only when retrying exactly the same request.

## 1. Project, client, rate, branding

```bash
tracker agent issuer.set --input '{"details":{"name":"Demo Studio","address":"1 Example Street","email":"studio@example.test","paymentInstructions":"Bank transfer; use invoice number as reference"}}'
tracker agent client.set --input '{"details":{"name":"Demo Client","address":"2 Example Street"}}' --key demo-client
tracker agent project.create --input '{"name":"Demo project","client":"CLIENT_ID","rate":"80.00","currency":"USD","effectiveAt":"2025-01-01T00:00:00Z","timezone":"Europe/London","dueDays":30}' --key demo-project
tracker agent template.create --input '{"name":"demo-invoice","copyFrom":"invoice"}'
tracker agent template.path --input '{"id":"user:demo-invoice"}'
```

Edit the returned Typst file. Import an image with `template.asset` and use the
returned `assets/...` reference in `image("assets/...")`.

```bash
tracker agent template.asset --input '{"id":"user:demo-invoice","source":"/absolute/path/image.png"}'
tracker agent project.configure --input '{"project":"PROJECT_ID","template":"user:demo-invoice","logo":"/absolute/path/logo.svg","accentColor":"#336699"}'
tracker agent template.validate --input '{"id":"user:demo-invoice"}'
```

Expected: images are copied into managed storage; bad paths or invalid templates
return errors. Project creation through the agent does not switch the panel selection.

## 2. Track and correct time

```bash
tracker agent task.create --input '{"project":"PROJECT_ID","title":"Design"}' --key demo-task
tracker agent task.start --input '{"id":"TASK_ID"}'
# Wait a few seconds.
tracker agent task.stop --input '{"id":"TASK_ID"}'
tracker agent entry.add --input '{"id":"TASK_ID","start":"2025-08-10T10:00:00+01:00","seconds":5400,"note":"Design review"}' --key demo-entry
tracker agent entry.list --input '{"project":"PROJECT_ID","from":"2025-08-01","to":"2025-09-01"}'
tracker agent entry.correct --input '{"id":"ENTRY_ID","revision":0,"delta":-1800,"reason":"Included a break"}' --key demo-correction
tracker agent summary --input '{"project":"PROJECT_ID","from":"2025-08-01","to":"2025-09-01"}'
```

Expected: 1 hour, USD 80.00. Retry `demo-entry` with identical input: no duplicate.
Retry that key with different seconds: `IDEMPOTENCY_CONFLICT`. Subtract too much:
error without changing the ledger. `entry.undo` should restore the original time
and retain both audit records. Correct it again before continuing if needed.

Change the rate effective after August and verify August remains USD 80/hour.
Remove the rate, record new time, and verify it is non-billable. Set a zero rate
and verify new work is billable at zero rather than classified as non-billable.

### Historical-time intent replay

Use a separate project/task in the sandbox. Fix the conversation's local “today”
at September 19, 2026; all entry timestamps below must be in the past when replayed.
The supplied `effectiveAt` controls rate history independently of the execution date.

```bash
tracker agent project.create --input '{"name":"Historical demo","timezone":"Europe/London","rate":"50","currency":"USD","effectiveAt":"2026-09-19T00:00:00+01:00"}' --key historical-project
tracker agent task.create --input '{"project":"HISTORY_PROJECT_ID","title":"Yesterday demo"}' --key historical-task
tracker agent project.get --input '{"project":"HISTORY_PROJECT_ID"}'
tracker agent entry.add --input '{"id":"HISTORY_TASK_ID","start":"2026-09-18T20:00:00+01:00","end":"2026-09-18T23:00:00+01:00"}' --key historical-entry
tracker agent summary --input '{"project":"HISTORY_PROJECT_ID","from":"2026-09-18","to":"2026-09-19"}'
```

Inspect every `data.entries[]` segment, including `billing.rate` and
`billing.resolved`. Expected: 10,800 recorded seconds, `resolved: true`,
`rate: null`; summary excludes 10,800 non-billable seconds. Even if `task.get`
reports the current USD 50 rate, say that yesterday's time is non-billable before
creating an invoice. For “record yesterday's work” without a billing expectation,
no question is needed. For expected billing without a known historical rate, ask
one focused rate/currency/scope question. After the main replay, check mixed-rate
segments separately: on a separate project with the same rate history, add an
interval spanning the effective date and verify both the unrated segment and the
rated segment are reported.

To exercise recovery of an existing empty draft, intentionally create one **after
explaining these exclusions**. This is a test fixture, not a reason to create an
empty invoice before investigating unexpected non-billable work:

```bash
tracker agent invoice.create --input '{"project":"HISTORY_PROJECT_ID","from":"2026-09-18","to":"2026-09-19","currency":"USD"}' --key historical-draft
tracker agent task.get --input '{"id":"HISTORY_TASK_ID"}'
tracker agent entry.list --input '{"project":"HISTORY_PROJECT_ID"}'
```

User: “Make that existing work billable at USD 50/hour; edit or recreate it.”
Follow entry pagination and select `entry.taskId`; confirm there is no other
eligible entry or elapsed running time outside this scope. Use the returned task
`entityRevision` below. Do not ask for the same authorization again or delete/recreate
the task (deleting retains its dated entries).

```bash
tracker agent task.rate --input '{"id":"HISTORY_TASK_ID","rate":"50","currency":"USD","applyExisting":true,"entityRevision":"HISTORY_TASK_TOKEN","reason":"User authorized existing three-hour slot"}' --key historical-backfill
tracker agent entry.list --input '{"project":"HISTORY_PROJECT_ID"}'
tracker agent summary --input '{"project":"HISTORY_PROJECT_ID","from":"2026-09-18","to":"2026-09-19"}'
tracker agent invoice.get --input '{"id":"HISTORY_DRAFT_ID"}'
# Replace HISTORY_DRAFT_REVISION with the numeric revision returned above.
tracker agent invoice.refresh --input '{"id":"HISTORY_DRAFT_ID","revision":HISTORY_DRAFT_REVISION}' --key historical-refresh
tracker agent invoice.preview --input '{"id":"HISTORY_DRAFT_ID"}'
```

Expected: `rateChange.appliedEntryIds` identifies the original entry, skipped
counts are zero, and `adjustmentId` identifies the audit record. Entry ID, interval,
and 10,800-second duration remain unchanged; only billing/audit metadata changes.
The fresh summary returns `amountMinor: "15000"` and `amountText: "USD 150.00"`.
The refreshed draft also has `totalMinor: "15000"` and its single preview shows
USD 150.00. A preview of the unrefreshed draft would still show its old amount.

Additional disposable variants:

| Variant | Expected review/replay result |
| --- | --- |
| Explicit USD 0/hour instead of 50 | Present zero rate is billable: 10,800 billable seconds, USD 0.00, no non-billable exclusion. Later backfill skips it as `alreadyRated`. |
| Already-priced time plus a new unrated entry | Only the unrated entry appears in `appliedEntryIds`; old historical pricing stays intact and `skipped.alreadyRated` increases. |
| Issued or paid time; externally billed legacy time | Backfill leaves these unchanged and reports `skipped.invoiced` / `skipped.externallyBilled`; inspect invoice allocations and entry flags to identify skipped IDs. |
| Two eligible entries on the task, only one slot authorized | Read all dates, detect both entries, explain that backfill is task-wide and ask one scope question; no rate write or preview until scope is resolved. `effectiveAt` does not narrow backfill. |
| Unresolved legacy time | Report unresolved/excluded, not ordinary non-billable. Resolve only with explicit historical pricing/no-rate and already-billed intent. |
| A priced draft's duration changes | On an unissued draft, use `entry.correct` with its entry revision (for example `delta: -3600`); rerun summary, get/refresh the draft revision, then preview USD 100.00 for the remaining two hours. |

Record the clarification count, redundant previews, actual tool-call sequence/count
(including pagination/retry-key calls), and result accuracy for each replay. For the
fully authorized USD 50 correction above: target **0** repeated authorization
questions, **0** redundant previews, one corrected preview, preserved interval and
USD 150.00. For unclear rate or single-slot/task-wide mismatch: one focused question
and no unauthorized write. These are scenario reviews of agent behavior, not tests
that assert documentation wording; API replay alone does not measure a live model's
clarification behavior.

### Verify the installed recipes

After `make backend`, install from the rebuilt executable into the sandbox:

```bash
tracker skill install --harness opencode --json
skill="$sandbox/config/opencode/skills/omatracker"
cmp skills/omatracker/references/workflows.md "$skill/references/workflows.md"
cmp AGENT_API.md "$skill/AGENT_API.md"
cmp tests/manual-invoices.md "$skill/tests/manual-invoices.md"
```

Read the installed `SKILL.md` and `references/installation.md`: the former includes
the historical billing/intent guidance plus the installer's path preamble, and the
latter pins this sandbox-tested executable. Reinstall should be a no-op. Do not use
your real HOME/XDG skill directories for this acceptance check.

## 3. Draft, issue, render, payment, corrections

### Task-rate and lifecycle checks

On a separate unrated project/task, add a dated hour and call `task.rate` with
`rate: "50"`, `currency: "USD"`. Its existing hour must remain non-billable.
Repeat with `applyExisting: true`: that hour becomes USD 50.00 and gains a rate
adjustment audit record. A later rate change must not reprice that already-priced
hour. In the widget, edit a task, turn off **Use project rate**, enter a rate/currency,
and explicitly check the existing-time option when desired. Verify the result with
`task.get`, `entry.list`, and `summary`.

Read an entity's `entityRevision`, change an unrelated client or task, then update
the original entity using that token: it should succeed. Change the original
entity itself, then retry with the old token: it should return `REVISION_CONFLICT`.
Create/delete/recreate a client or project using a fresh `request.key` for each
operation; the replacement receives a new ID. Reusing the deleted entity's old
creation key must return `REQUEST_TARGET_REMOVED`, not an obsolete successful ID.
`--key auto` is for a new operation; use its printed/returned key for an exact retry.

### Invoice checks

```bash
tracker agent invoice.create --input '{"project":"PROJECT_ID","from":"2025-08-01","to":"2025-09-01","currency":"USD"}' --key demo-draft
tracker agent invoice.preview --input '{"id":"INVOICE_ID"}'
# Open the returned PDF path in your PDF viewer.
tracker agent invoice.issue --input '{"id":"INVOICE_ID","revision":1,"date":"2025-09-01"}' --key demo-issue
tracker agent invoice.render --input '{"id":"INVOICE_ID"}'
tracker agent invoice.get --input '{"id":"INVOICE_ID"}'
```

Check client/issuer, images, invoice number, service dates, issue/due dates,
payment instructions, hours, rate, currency, and total. Preview is marked DRAFT;
issued PDF is not. There should be no tax calculation.

Edit the original template and logo. Delete only the generated PDF, then rerun
`invoice.render` without the old retry key: the captured original design must be
used. Issued bundles live under `$sandbox/ledger.json.invoices/`.

Create a second overlapping draft: issued time must be excluded. Try correcting
issued time: expect `ENTRY_INVOICED`. Test `invoice.void`, correction, and
`invoice.reissue`; the replacement references the original and receives a new
number when issued. `invoice.paid` changes payment state without uploading anything.

## 4. Scheduling and panel

Run `invoice.check` twice. It creates monthly drafts for eligible completed
periods, assigns no invoice numbers, and uploads nothing. Late time should refresh
an existing draft when requested or produce a supplemental draft after issuance.

For an isolated real panel, use `make ui-check` for automated coverage. To inspect
your installed panel after release, reload the plugin and open Invoices. Verify:

- Weekly/monthly options are mutually exclusive; both off means manual.
- Creating drafts, previewing, issuing, rendering and uploading update the list.
- Failed PDF/upload work does not stop timers from responding.
- Templates show the invoice selection and open correctly.
- Current-rate counter estimates are distinct from historical invoice amounts.

## 5. Google Drive (explicit integration test)

The sandbox HOME intentionally has no existing rclone credentials. Configure a
test remote within it, completing the browser OAuth flow:

```bash
HOME="$sandbox/home" XDG_CONFIG_HOME="$sandbox/config" rclone config
tracker agent drive.configure --input '{"remote":"demo-drive","driveFolder":"OmaTracker-test"}'
tracker agent drive.check
tracker agent drive.test
tracker agent invoice.upload --input '{"id":"ISSUED_INVOICE_ID"}' --key demo-upload
```

Check the PDF in the returned Drive destination. Repeating the same key must not
create another upload operation. For a new invoice, test with a disconnected
network: local PDF remains available, upload reports failure, and retry after
reconnection succeeds at the same destination. Do not publish/share credentials.
The test upload leaves a uniquely named file in `setup-tests/` for inspection.

## 6. Existing-data migration

Copy an old ledger to a **different disposable path** and point the function at it.
Run `migration.preview`, then `migration.apply`. Verify original bytes in the
`.pre-invoices.bak` sibling (or `.pre-task-rates.bak` for a version 3 source) and
preservation of entries, timers, archived reports and invoice metadata. Verify the
resulting version is 4; an older 0.5 CLI should reject it rather than rewrite it.
Unresolved time must be excluded from invoices until `migration.resolve` specifies
a historical rate or `noRate`; identify already-billed ranges with
`externallyBilled: true`. Undated legacy counters must not appear in invoices.

Back up/restore the ledger, `.invoices/` directory, and template library together.
To test clear-all while the sandbox still contains its invoice metadata, first run
`tracker data clear --include-drive --dry-run --json` and inspect the exact paths.
When you choose to execute it, omit `--dry-run`. Verify zero user projects/clients,
an empty internal Unassigned workspace, a backup path, and removal of only the
listed remote files. Setup-test files and remote folders are outside this scope.
Remove the sandbox and the remaining test Drive folder separately when finished.
