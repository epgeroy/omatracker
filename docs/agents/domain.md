# Domain Docs

## Before exploring

Read the root `CONTEXT.md` and the ADRs in `docs/adr/` relevant to the area
being changed. If a `CONTEXT-MAP.md` is ever introduced, use it to locate the
relevant context documentation instead.

## Layout

This is a single-context repository. Domain language lives in root
`CONTEXT.md`; durable technical decisions live in `docs/adr/`.

## Consumer rules

Use the terms defined in `CONTEXT.md` in issues, specs, tickets, tests, and
other engineering output. Do not replace glossary terms with their listed
avoid-terms. Explicitly flag a proposal that conflicts with an applicable ADR.
