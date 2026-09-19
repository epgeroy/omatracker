# Plan 05: Preview opening and template validation

Status: Implemented. Priority: P1. Automated checks passed; real desktop dispatch
was exercised, with visible-window confirmation still pending.

## Goal and evidence

Make “show me” display the requested artifact, avoid redundant renders, and
distinguish successful compilation from a correct-looking invoice.

Source: OpenCode session `ses_f4504c0ecffeJwUJ521N9vjAGD`, September 19, 2026.

- Five invoice previews were generated.
- The user had to ask “open it” twice after receiving only a path.
- The two `xdg-open` tool calls occupied approximately 23 and nine seconds.
- An unchanged zero-total preview was generated and read again.
- `template.validate` succeeded before PDF inspection exposed literal footer
  markup. Missing Typst `#` prefixes were then corrected.
- The final invoice preview was generated but not visually inspected before issue.

## Intended behavior

| User intent | Behavior |
| --- | --- |
| Generate/export a preview | Return a PDF path and relevant billing summary. |
| Show/open a preview | Obtain a current PDF and launch it in the desktop viewer. |
| Review/check the layout | Inspect the rendered artifact and report findings. |
| Show the same unchanged preview | Reuse the known existing artifact. |
| Show a draft after source billing changes | Refresh the draft, then render/open. |
| Show an issued invoice | Render/open its captured original, preserving immutability. |

`invoice.preview` renders a draft; it is not a billing refresh operation.
Keep visual review distinct from a compile smoke test. Generating a PDF without
reading it establishes renderability, not a completed visual inspection.

## Implementation steps

### Phase A: Recipes and validation guidance

- [x] Add the intent table and refresh/render distinction to the skill workflows.
- [x] Reuse known PDF paths when the artifact still exists and relevant inputs are
  known unchanged. Regenerate when freshness is uncertain; do not infer freshness
  from the invoice ID alone.
- [x] Require a rendered inspection after changing a template. For an unchanged
  template, inspect a new preview when content/layout differences warrant it.
- [x] Document that `template.validate` compiles representative fixture data.
  Explain why it can pass while unintended literal text appears in the PDF.
- [x] Add a correct Typst footer example showing `#text`, `#h`, and expression
  interpolation inside content blocks.

### Phase B: Reliable desktop opening

- [x] Review `Service.qml::openTemplateFile` and existing desktop-opening behavior
  before choosing a CLI helper location; avoid divergent path/URI handling.
- [x] Add a narrowly scoped opener, provisionally `artifact.open`, or an equivalent
  documented helper. Proposed behavior: accept an explicit existing local PDF,
  dispatch through the platform opener, and return promptly with launch status.
- [x] Detach long-lived viewer execution and its inherited output streams while
  preserving actionable launch failures. A bare shell background operator is not
  sufficient evidence that the tool will return promptly.
- [x] Report “launch requested” separately from “document visibly opened”. Retain
  the PDF path as a usable result if desktop integration is unavailable.
- [x] Test paths with spaces and non-ASCII characters, missing files, headless
  sessions, and missing associations without launching real GUI programs in tests.

### Phase C: Stronger evidence and reusable branding

- [x] Add a regression fixture with compiling-but-literal footer markup. Where a
  PDF text extractor is available, verify known content and detect obvious leaked
  markup. Surface heuristic findings as warnings rather than proof of bad layout.
- [x] If validation returns richer metadata, preserve `valid` compatibility and
  explicitly identify compile checks versus text checks and visual review.
- [x] Record optional template metadata: client, source URL, logo origin, palette,
  and review date. Only record provenance actually established by the workflow.
  Existing metadata-free templates remain usable.
- [x] Prefer one suitable existing template before reading every alternative.
  When website matching is requested, verify the supplied website or explain that
  an existing local asset was reused without website verification.

## Files and integration points

- `skills/omatracker/SKILL.md`, `skills/omatracker/references/workflows.md`.
- `TEMPLATES.md`, `AGENT_API.md`, `tests/manual-invoices.md`.
- `src/templates.rs::validate` / `compile`, `src/billing.rs` invoice rendering,
  and `src/agent.rs` dispatch if runtime support is added.
- `Service.qml::openTemplateFile`, `TemplateServiceTest.qml`: existing UI behavior.
- `tests/templates.rs`, `tests/invoices.rs`, and Typst fixtures in `tests/`.
- `src/skills.rs`, `tests/skills.rs`: embedded documentation delivery.

## Freshness boundaries

Start with session-level reuse; a persistent preview cache is a follow-up only if
measurement justifies it. Such a cache would need fingerprints for draft data,
template source and imported assets, project/logo inputs, and relevant render
configuration. A matching draft revision alone does not cover external template
edits. Deleted temporary PDFs must be regenerated.

## Verification and acceptance criteria

1. “Show me a preview” launches the PDF in the same user turn; no follow-up “open it”
   is needed in a working desktop environment.
2. A repeated request with known unchanged inputs reuses the artifact.
3. A rate change refreshes draft billing before preview; a template/logo change
   regenerates the PDF. Issued documents remain captured originals.
4. The broken-footer fixture compiles, but the review workflow catches its visible
   defect. A missing optional text extractor is reported as a skipped check.
5. A fake long-running opener does not keep the tool waiting for viewer exit;
   immediate dispatch errors are surfaced without losing the generated PDF path.
6. Branding claims distinguish verified website matching from local-logo reuse.

Run focused template/invoice tests and `make template-check` when fixtures or
rendering change. Run the relevant QML checks only if shared UI opening behavior
changes. Use mocked openers/remotes and disposable ledgers for automated scenarios;
perform one real desktop smoke check when validating launch behavior.

## Dependencies and implementation preflight

Phase A can ship independently. Coordinate historical draft refresh with
[Plan 01](01-historical-time-and-intent-handling.md), workflow retry behavior with
[Plan 02](02-retry-keys-and-round-trips.md), and documentation routing with
[Plan 04](04-operational-documentation-and-discovery.md).

Before changing rendering, opener, or dispatch code, review existing uncommitted
work, refresh stale GitNexus data, and run upstream impact analysis. A CLI agent
action is not automatically an HTTP route; use route-specific impact checks if an
actual API route handler is changed. Analyze graph changes before any requested
commit. Keep source changes and installed-skill rollout in sync.

## Implementation and verification record

- Implemented in `feat/preview-validation`, in a separate worktree, and merged
  with the existing local refactor after testing their combined source tree.
- `artifact.open` accepts a local PDF path, follows the QML file-URL encoding,
  detaches streams/process group and observes immediate dispatch errors for
  250 ms. It preserves the path and distinguishes launch status from visibility.
- `template.validate` retains `valid`, adds compile/text/visual evidence, and
  reports optional text extraction as skipped when unavailable. The real Typst
  regression verifies literal-footer warnings disappear after fixing prefixes.
- Preview reuse and optional `metadata.json` provenance are workflow-level;
  there is no persistent cache or required template metadata migration.
- Full `make check` passed on the feature and combined integration: 96 Rust tests
  in the latter, formatting, Clippy, five Typst fixtures, plugin validation,
  QML/UI checks and installation checks. The offscreen UI suite skips its existing
  Wayland-only popup case. The merged source tree was verified byte-for-byte
  against the tested integration before rebuilding the bundled binary.
- Real desktop smoke: an encoded path containing a space and `é` returned
  `launch_requested` in 0.26 s. A visible viewer window could not be confirmed;
  this is not recorded as a successful visual-open check. The invoice PDF was
  separately inspected and the broken fixture's literal text was confirmed.
- Rebuilt the local CLI and updated the existing OpenCode skill through its
  managed installer; a subsequent dry run reported `unchanged`. Restart OpenCode
  to load the updated skill.
