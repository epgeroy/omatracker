# OmaTracker Privacy Policy

Effective date: September 19, 2026

OmaTracker is a local-first Omarchy plugin. This policy describes how the
plugin handles information when it is installed and used.

## Information the plugin stores

OmaTracker stores the following information locally on the user's computer:

- Project, client, task, and time-entry details.
- Rates, corrections, invoice/report settings, snapshots and generated document status.
- The configured rclone remote name and destination folder.

By default, this data is stored in
`~/.config/omarchy/omatracker.json`. Issued invoice inputs and PDFs live beside it
in `<ledger>.invoices/`. Legacy report inputs and PDFs use
`~/.cache/omarchy/omatracker/`. Explicit clear-all backups use `<ledger>.backups/`.

## Information the plugin does not collect

OmaTracker does not operate a server and does not collect analytics, telemetry,
advertising identifiers, usage statistics, or personal information on behalf of
the maintainer. Optional Drive operations are performed by the user's local rclone.

## Optional Google Drive integration

Google Drive integration is optional. OmaTracker invokes the user-installed
`rclone` program to copy the local state file and generated PDFs to the remote
and folder selected by the user. When the user explicitly runs
`data clear --include-drive`, rclone also inventories identified tracker files,
downloads backup copies, and deletes those exact remote files.

OAuth authorization, access tokens, and refresh tokens are managed by rclone
in its own configuration. OmaTracker does not read, store, transmit, or share
those credentials. Users should configure rclone with the `drive.file` scope so
rclone can access only files it creates.

When Drive integration is used, Google processes uploaded files under the
user's agreement with Google, including Google's applicable privacy policy and
terms. The project maintainer does not receive those files or OAuth credentials.

## Data control and deletion

Users control their local data. `omatracker data clear` clears user tracking and
billing records and identified generated local documents, after creating a backup.
An empty internal Unassigned workspace remains. Reusable templates, issuer settings,
invoice-number counters, Drive configuration and local preferences are preserved.
The optional `--include-drive` flag also backs up and deletes identified remote
invoice/report files and the matching ledger snapshot, not the whole remote folder.
Use `--dry-run` to inspect the scope. Backups retain old data until the user deletes
them separately. Unknown remote files, previous backups and unrelated caches are
not removed automatically. To revoke Drive access, remove or reconnect the rclone
remote through `rclone config`.

## Security

OmaTracker uses local filesystem permissions, atomic file replacement, and a
local advisory lock to protect its ledger. Users remain responsible for securing
reports.

## Policy changes and contact

Changes to this policy will be published in this repository. Questions can be
opened as an issue at <https://github.com/epgeroy/omatracker>.
