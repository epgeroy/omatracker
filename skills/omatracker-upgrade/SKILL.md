---
name: omatracker-upgrade
description: Back up, verify or restore OmaTracker local data; upgrade the installed CLI, widget and skills from a GitHub release or the pinned main branch with --edge; recover from a failed upgrade.
---

# OmaTracker backup and upgrade

Read `references/installation.md` to locate the exact installed executable and ledger. Use its `--data-path` when the user specifies a ledger. Read [upgrade and backup reference](references/upgrade.md) for commands and recovery rules.

1. Discover the actual widget backend/ledger with `omarchy-shell omatracker status` when available. Choose the same ledger for CLI operations. Verify dependencies with `doctor` and inspect the selected release or `--edge` SHA using `upgrade --dry-run`.
2. Run `backup create` and `backup verify PATH`; retain the path and revision. Backups contain private records: share paths, not contents. Explicit backup requests stop here.
3. For an upgrade, use the pinned executable's `upgrade` command (default latest published tag, `--tag TAG` for a specified release, `--edge` for main). This creates another pre-upgrade backup, validates the candidate, installs CLI/widget/skills and checks the live ledger. Report the selected tag/SHA, resulting version and backup path.
4. On failure, report whether automatic rollback completed. If writes occurred since the backup, preserve both versions and ask before any manual restore. `backup restore PATH --dry-run` previews; `backup restore PATH` creates a recovery snapshot before replacing files. Verify the restored binary/ledger and restart the shell when the widget was affected.

Never run `data clear` to test recovery. A Drive sync is a single remote snapshot, not a versioned backup. Restores do not revert Drive uploads. Keep the ledger, invoice bundles, templates and installed runtime together; older binaries may reject newer ledgers.
