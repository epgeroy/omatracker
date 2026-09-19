# OmaTracker Privacy Policy

Effective date: September 18, 2026

OmaTracker is a local-first Omarchy plugin. This policy describes how the
plugin handles information when it is installed and used.

## Information the plugin stores

OmaTracker stores the following information locally on the user's computer:

- Project, client, task, and time-entry details.
- Report settings and generated report status.
- The configured rclone remote name and destination folder.

By default, this data is stored in
`~/.config/omarchy/omatracker.json`. Generated report inputs and PDFs are
stored in `~/.cache/omarchy/omatracker/` until their upload completes.

## Information the plugin does not collect

OmaTracker does not operate a server and does not collect analytics, telemetry,
advertising identifiers, usage statistics, or personal information on behalf of
explicitly configures and requests a Google Drive upload through rclone.

## Optional Google Drive integration

Google Drive integration is optional. OmaTracker invokes the user-installed
`rclone` program to copy the local state file and generated PDFs to the remote
and folder selected by the user.

OAuth authorization, access tokens, and refresh tokens are managed by rclone
in its own configuration. OmaTracker does not read, store, transmit, or share
those credentials. Users should configure rclone with the `drive.file` scope so
rclone can access only files it creates.

When Drive integration is used, Google processes uploaded files under the
user's agreement with Google, including Google's applicable privacy policy and
terms. The project maintainer does not receive those files or OAuth credentials.

## Data control and deletion

Users control their local data. To remove it, delete the configured state file
and the report cache. To remove uploaded files, delete them from the configured
Google Drive folder. To revoke Drive access, remove or reconnect the rclone
remote through `rclone config`.

## Security

OmaTracker uses local filesystem permissions, atomic file replacement, and a
local advisory lock to protect its ledger. Users remain responsible for securing
reports.

## Policy changes and contact

Changes to this policy will be published in this repository. Questions can be
opened as an issue at <https://github.com/epgeroy/omatracker>.
