# Issue tracker: GitHub

Issues and specs for this repo live as GitHub issues in `epgeroy/omatracker`.
Use the `gh` CLI with `--repo epgeroy/omatracker` for all operations: this clone
also has an upstream remote, so relying on automatic repository detection is
ambiguous.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body "..."`. Use a heredoc for multi-line bodies.
- **Read an issue**: `gh issue view <number> --comments`, including labels.
- **List issues**: `gh issue list --state open` with appropriate label filters.
- **Comment on an issue**: `gh issue comment <number> --body "..."`.
- **Apply or remove labels**: `gh issue edit <number> --add-label "..."` or `--remove-label "..."`.
- **Close an issue**: `gh issue close <number> --comment "..."`.

## Pull requests as a triage surface

**PRs as a request surface: no.**

## Publishing work

When an engineering skill says to publish a spec or ticket, create a GitHub
issue and apply the configured `ready-for-agent` label when requested.
