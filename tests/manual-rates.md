# Manual project-rate checks

Build the bundled backend (`make backend`) and reload the OmaTracker plugin
using your normal shell/plugin reload workflow so the new QML is loaded.
Run these checks in a new test project. If your installed plugin is a separate
checkout, update that checkout to the merged commit first.

## Panel

1. Open the panel and create a project named **Rate test**.
2. Enter hourly rate **80** and currency **USD**, and save project settings.
3. Add a task, edit it, and add **1h30m** of manual time. Expect
   **USD 80.00/h · Estimated: USD 120.00** and **01:30:00**.
4. Start the task. Confirm time and the estimated amount increase, then stop it.
5. Enter an invalid rate such as **abc** or **80.001**. Save. An error should
   stay visible, and the previously saved rate and amount should remain.
6. Save a valid rate of **100**. The error should disappear and the estimate
   should use the new rate. With exactly 1h30m it would be **USD 150.00**.
7. Create another project, set **125 JPY**, and add **30m** to a task. Expect
   **JPY 63**. Switch back and confirm the first project's USD rate is restored.
8. Close/reopen settings and reload the plugin. Confirm saved rates persist.
9. Set a rate of **0** and save: the monetary section should display zero.
   Empty the rate field and save: the section should disappear, with time intact.
10. Check narrow panel layouts, keyboard tabbing through both new fields, and
    saving metadata on an unrated project. Existing template/report toggles
    should preserve any configured rate.

## Isolated CLI smoke test

From the repository root:

```bash
sandbox=$(mktemp -d)
project=$(bin/omatracker --data-path "$sandbox/ledger.json" project create "Rate test")
bin/omatracker --data-path "$sandbox/ledger.json" project update "$project" --hourly-rate 80 --currency USD
task=$(bin/omatracker --data-path "$sandbox/ledger.json" task add "Design")
bin/omatracker --data-path "$sandbox/ledger.json" task edit "$task" --add 1h30m
bin/omatracker --data-path "$sandbox/ledger.json" status --json --compact
```

Expect `activeProjectSeconds: 5400`, `rate.amountMinor: 8000`, and
`activeProjectEstimate.amountText: "USD 120.00"`. Test `--clear-rate`, missing
currency on a new rate, `--currency XYZ`, and conflicting `--clear-rate` and
`--hourly-rate` options. Invalid updates must leave the saved values intact.

## PDFs and frozen snapshots

Exports use the **previous completed** week/month, not newly added time in the
current period. This disposable fixture creates 1h30m in the previous week:

```bash
sandbox=$(mktemp -d)
cp -r templates "$sandbox/templates"
python3 - "$sandbox/ledger.json" <<'PY'
import datetime as dt
import json
import sys

today = dt.datetime.now().replace(hour=0, minute=0, second=0, microsecond=0)
start = today - dt.timedelta(days=today.weekday() + 7) + dt.timedelta(hours=9)
project = "project-unassigned"
data = {
    "version": 2, "activeProjectId": project,
    "projects": [{"id": project, "name": "Rate PDF test",
                  "rate": {"amountMinor": 8000, "currency": "USD"}}],
    "entries": [{"id": "test-entry", "projectId": project, "taskId": "test-task",
                 "taskTitle": "Design", "startedAt": int(start.timestamp() * 1000),
                 "endedAt": int((start + dt.timedelta(minutes=90)).timestamp() * 1000),
                 "seconds": 5400}]
}
with open(sys.argv[1], "w") as output:
    json.dump(data, output)
PY

HOME="$sandbox" OMATRACKER_TEMPLATE_DIR="$sandbox/templates" \
  bin/omatracker --data-path "$sandbox/ledger.json" report export weekly
```

Open the PDF under `$sandbox/.cache/omarchy/omatracker/`. Expect **01:30:00**,
**USD 80.00/h**, and **Estimated amount: USD 120.00**. With no Drive remote set,
upload remains pending; the local PDF is still available.

Change the rate and retry:

```bash
bin/omatracker --data-path "$sandbox/ledger.json" project update project-unassigned --hourly-rate 100 --currency EUR
HOME="$sandbox" OMATRACKER_TEMPLATE_DIR="$sandbox/templates" \
  bin/omatracker --data-path "$sandbox/ledger.json" report retry
```

The existing JSON snapshot and PDF must still show **USD 120.00**. To check the
summary template, repeat with a fresh sandbox and set `--template-id summary`
before the first export. With a fresh unrated project, PDFs should show time only.

For a quick visual comparison of both templates with/without amounts, run
`make template-check` and open `target/template-rates.pdf` (four pages). The
separate detailed/summary PDFs use an older snapshot without monetary fields.
