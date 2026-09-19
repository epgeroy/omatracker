# Plan 04: Operational documentation and discovery

Status: Proposed. Priority: P1. Progressive documentation with optional scoped help.

## Goal and evidence

Allow common operations to reach their exact contract without loading unrelated
installation, reset, migration, and invoice documentation.

Source: OpenCode session `ses_f4504c0ecffeJwUJ521N9vjAGD`, messages 1–11.
Creating and starting one task took 12 tool calls and approximately 37 seconds.
The agent read the first 240 lines of `AGENT_API.md`, but the task action table
started at line 265, requiring another search/read. It also called both `context`
and `project.list`, although `context` already provided the target project ID.

The recorded per-response token total grew from roughly 25k to 82k across the
session. This includes cached context and is not a measure of uncached cost alone.

## Information architecture

Keep `SKILL.md` concise and route by user intent:

| Intent | Load first |
| --- | --- |
| Create/start a task | Task/time request examples and targeting rules |
| Add historical work | Time recipe plus billing-result checks from Plan 01 |
| Record many dated tasks | Bulk recipe from Plan 03 once implemented |
| Prepare or send an invoice | Summary, draft, issue/render/upload recipe |
| Customize or show a template | Template contract and Plan 05's preview semantics |
| Install, reset, or migrate | Only the matching administrative reference |

Move a linked quick index, request conventions, and common examples to the top of
`AGENT_API.md`. Move lengthy administrative material into later sections or
dedicated references while preserving useful heading links where practical.
Keep one authoritative field contract; recipes should link to it rather than
copying large tables that can drift.

## Discovery rules

- Read the installed executable reference once per session/installation context.
- Use explicit project IDs already returned by authoritative commands.
- Use `repository.resolve` when repository binding is relevant, `project.list`
  for project enumeration, and `context` when timers/draft summaries are needed.
- Do not automatically run both `context` and `project.list`. Fetch additional
  fields only when the operation needs them, such as an entity revision for editing.
- Preserve pagination and archived-entity handling; a partial page is not proof
  that a target is absent.
- Refresh entity metadata on a conflict or meaningful intervening change rather
  than indiscriminately re-fetching every entity before every operation.
- Batch independent reads and key preparation. Keep ID-dependent writes ordered.

## Implementation steps

- [ ] Restructure `skills/omatracker/SKILL.md` around the intent router and concise
  invariants: exact executable, explicit project, retry identity, historical rates.
- [ ] Reorganize `AGENT_API.md` so a basic task request needs no administrative
  sections. Add complete create/start and dated-entry examples near the quick index.
- [ ] Split `skills/omatracker/references/workflows.md` only if task-focused loading
  materially improves; avoid creating many tiny references that require extra reads.
- [ ] Add the discovery decision rules and grouped-read examples to the recipes.
- [ ] Optionally add action-scoped help, provisionally `agent help --input
  '{"action":"entry.add"}'`, with required/optional fields and one valid example.
  This syntax is proposed and must be implemented before being advertised.
- [ ] Keep unscoped `agent help` backward compatible. If scoped help is implemented,
  derive or validate it against accepted request fields to prevent schema drift.
- [ ] Update `src/skills.rs::bundle` for every new reference. The installer embeds
  an explicit file list; adding a Markdown file alone does not distribute it.
- [ ] Verify installed links/paths from a working directory outside this repository.
  Rebuild before testing installation, because the docs are embedded at build time.

## Files and integration points

- `skills/omatracker/SKILL.md` and its `references/` directory.
- `AGENT_API.md`, `TEMPLATES.md`, and relevant `README.md` links.
- `src/agent.rs` if scoped help is added.
- `src/skills.rs`, `tests/skills.rs`, `tests/install-check.py` for distribution.

Planning-time upstream impact of `src/skills.rs::bundle`: LOW, one direct caller
(`skills::execute`), one indirect caller (`skills::run`), no indexed processes.
The index was three commits behind, so this is not implementation-time clearance.

## Verification and acceptance criteria

Replay these prompts with a freshly installed skill in an isolated environment:

1. “Create demo under project X and start tracking.”
2. “Add yesterday's three hours under project X.”
3. “Create the invoice for this range and upload it.”
4. “Show the existing invoice preview.”
5. “Rename an archived project's client” to exercise discovery edge cases.

Check that the assistant loads only relevant references, uses valid requests,
does not repeat discovery without a reason, and follows pagination when needed.
For the basic task scenario, target one project-discovery call and no more than
two targeted documentation reads after skill loading, including installation lookup.

Validate all relative links from both source and installed layouts. Exercise
`cargo test --test skills` for packaging changes; use the isolated install check
when the bundle layout changes. Documentation-only review should not introduce
tests that merely match prose. Scoped-help code needs field/unknown-action tests.

## Dependencies and implementation preflight

The router can ship independently. Link completed recipes from
[Plan 01](01-historical-time-and-intent-handling.md),
[Plan 02](02-retry-keys-and-round-trips.md),
[Plan 03](03-bulk-dated-work-helper.md), and
[Plan 05](05-preview-opening-and-template-validation.md) as they land. Never
advertise proposed actions in installed documentation before the binary supports them.

Review current uncommitted documentation and installer changes before editing.
Refresh stale graph data and analyze affected help/bundle symbols upstream.
Run graph change analysis before any requested commit. Test installation using
temporary HOME/XDG paths rather than overwriting the user's installed skill.
