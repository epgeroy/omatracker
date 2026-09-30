# Commands and recovery

Use the executable in `installation.md`. All commands accept global `--data-path` before the subcommand.

```sh
omatracker backup create
omatracker backup list
omatracker backup verify /absolute/path/to/backup
omatracker backup restore /absolute/path/to/backup --dry-run
omatracker backup restore /absolute/path/to/backup
omatracker upgrade --dry-run
omatracker upgrade --tag v0.7.0
omatracker upgrade --edge
```

`upgrade` defaults to the latest published GitHub release. `--edge` checks out `main`, pins the resolved commit, builds it with Cargo and reports the SHA. A release requires `gh`, `git`, and the published binary plus SHA-256 asset; edge also needs Cargo. `--no-shell` is for a headless install. An upgrade captures a verified backup before publishing anything. The CLI will stop automatic rollback when the ledger revision changed independently; inspect the recovery copy and newer work before manually restoring. An installed standalone binary is required; upgrades refuse a plugin Git checkout with uncommitted local changes.

The local backup includes the ledger, feedback, invoice bundles, workflow journals, migration copies, report cache, user templates/assets, standalone binary/bundled templates, plugin runtime, managed skills and referenced logos under HOME. Existing `.git` metadata in a development plugin checkout is excluded; restore preserves it. A referenced logo outside HOME blocks backup until moved or backed up separately. Verify the manifest before restoring. Backups are private files under `<ledger>.backups/`; protect this directory like the ledger itself.
