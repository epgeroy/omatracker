# Custom PDF templates

OmaTracker includes **Detailed** and **Summary** Typst layouts. To change a layout,
create a user-owned copy. Plugin updates never replace user copies.

## Panel workflow

1. Open project settings and expand **PDF → Customize**.
2. Select a starting layout, enter a name such as `client-report`, and click
   **Create editable copy and select**. The copy opens in the desktop-associated
   application for `.typ` files. If no editor is associated, open the file path
   returned by `template path` in your preferred editor.
3. Edit `template.typ`, save it, and click **Preview**. Previews use the selected
   project's **saved** settings and the preceding completed week's entries.
4. Set the accent hex color, A4/Letter paper, and optional logo image path, then
   click **Save appearance** before previewing. Clear the logo field to remove it.
5. Use **Refresh** to discover templates added outside the panel. Source edits
   are read automatically on the next preview or newly queued report.

The template selector saves immediately. **Edit file** is enabled for user
templates. Built-ins remain available as starting points. The template section
scrolls when expanded, including long compiler errors. Custom templates decide
whether to use the project's appearance settings; both built-ins honor them.

## Storage and identifiers

```text
${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/omatracker/templates/
  client-report/
    template.typ
    helpers.typ
    assets/
      mark.svg
```

`XDG_CONFIG_HOME` must be absolute; an unset or relative value falls back to
`$HOME/.config`. Template names start with an ASCII letter or digit and contain
only ASCII letters, digits, `-`, or `_`, up to 80 characters.

Built-in IDs are `detailed` and `summary`; custom IDs are `user:<name>`.
Names are case-sensitive. Creating an existing name fails without replacing it.
Delete a custom template by removing its directory. Projects retain a missing
template's ID and report an error instead of silently switching layouts.

`OMATRACKER_TEMPLATE_DIR` overrides the **built-in** source directory, using
`detailed.typ` and `summary.typ`. A missing override file is an error, with no
fallback. Custom IDs always use the user library. Otherwise, built-ins resolve
from the plugin's `templates/`, then the development working directory.

## CLI

```bash
bin/omatracker template list --json
bin/omatracker template create client-report --from detailed
bin/omatracker template path user:client-report
bin/omatracker template validate user:client-report
bin/omatracker template preview user:client-report --project <project-id>
bin/omatracker project update <project-id> --template-id user:client-report
bin/omatracker project update <project-id> \
  --accent-color '#336699' --paper letter --logo-path /absolute/path/logo.svg
```

`create` returns JSON including `id` and `path`. `preview` returns the PDF path;
omitting `--project` uses the active project. `validate` compiles representative
sample data; use preview to check actual project metadata and logos. Typst errors
include source locations. Tracking and template management work without Typst;
validation, preview, and PDF rendering require it.

## Template contract

Each `template.typ` exports a `render(data)` function, for example:

```typst
#let render(data) = {
  set page(paper: data.project.paper)
  set text(size: 10pt)
  text(size: 20pt, fill: rgb(data.project.accentColor))[#data.project.name]
  if data.project.logoPath != "" {
    image(data.project.logoPath, width: 25mm)
  }
  [Total: #data.totalDuration]
  for entry in data.entries {
    [#entry.date — #entry.task — #entry.duration]
    linebreak()
  }
}
```

Available fields (camelCase):

| Object | Fields |
| --- | --- |
| Root | `generatedAt`, `project`, `period`, `totalSeconds`, `totalDuration`, `entries` |
| `project` | `id`, `name`, `clientName`, `companyName`, `logoPath`, `accentColor`, `paper` |
| `period` | `kind`, `label`, `start`, `end`, `startAt`, `endAt` |
| Each entry | `task`, `note`, `date`, `started`, `ended`, `duration`, `seconds` |

Timestamps are milliseconds since the Unix epoch. Durations are `HH:MM:SS` and
numeric seconds. `entries` can be empty. See `tests/report-snapshot.json` for a
complete example. Interpolate data as text rather than evaluating it as source.

Use relative local imports and assets, e.g. `#import "helpers.typ": heading`
and `image("assets/mark.svg")`. Keep dependencies inside the template directory;
symlinks and special files are rejected. Compilation uses the captured bundle
as Typst's root, so imports outside it cannot read arbitrary project/home files.
The supplied `project.logoPath` is rewritten to the captured logo within this
root and can be passed directly to `image`. Supported logo extensions are PNG,
JPEG, SVG, and GIF. Keep portable templates self-contained; system fonts and
external Typst packages are not captured.

## Reports and previews

At queue time, OmaTracker captures report JSON, the template directory, and the
project logo into a report-specific cache bundle. `manifest.json` records the
template ID and a SHA-256 fingerprint of the captured inputs. Edits, renames,
deletions, and plugin updates do not change those inputs on retry. A missing
captured bundle is an error, not permission to use a different template.

Existing rendered PDFs are reused on upload retries. Old reports without a
bundle capture the currently available source the first time they need rendering;
the exact source originally used by an old version cannot be recovered.

Preview creates a separate bundle/PDF under
`~/.cache/omarchy/omatracker/previews/`. It never queues a report, changes the
ledger, or uploads to Drive. Previews can be deleted after viewing. Report
bundles should be retained for retries. Reports are still deduplicated by project
and period: exporting an already queued period does not apply subsequent edits.
Validate and preview changes before using them in scheduled exports.

## Manual acceptance tests

After `make backend` and `make check`, reload the widget to load the new QML.

1. **Create/edit/select:** Open project settings, expand PDF customization,
   create `manual-test` from Detailed, and verify that it is selected. Change
   a heading or font size in its `template.typ`. Preview and confirm the change.
   Switch to Summary and back, then restart the widget and verify persistence.
2. **Appearance:** Save a distinctive accent color, Letter paper, and an SVG or
   PNG logo. Preview both built-ins and the custom copy. Clear the logo, save,
   and check that it disappears. Try an invalid color and nonexistent logo;
   an error should appear without partially saving settings.
3. **Compiler errors:** Break the custom template's syntax and preview. Confirm
   a useful error appears and remains visible through status refreshes. Add,
   start, and stop a task. Fix the syntax and preview again; the error should clear.
4. **Discovery/missing files:** Add another template directory externally and
   refresh. Select it, move its directory away, then refresh and preview. The
   missing ID should remain selected with a clear error. Restore the directory.
5. **Preview isolation:** Compare `bin/omatracker status --json` before and after
   repeated previews. The report list should be unchanged; no Drive upload should
   occur. Each preview should open its own PDF.
6. **Captured retry:** Use an isolated ledger/home as shown below. Export with
   no Drive remote, note the queued report's `templateBundle` and `pdfPath`, then
   edit/delete its source template and logo. Delete only that report's generated
   PDF and run `report retry`. Its rebuilt PDF should retain the original layout
   and logo; the captured inputs and manifest should be unchanged.
7. **Regression:** Verify weekly/monthly exports and background checks with your
   normal setup. A period already queued retains its previous design. Use a new
   project/period to verify a newly selected template is used.

For the isolated retry test, run from the repository root:

```bash
backend="$PWD/bin/omatracker"
sandbox="$(mktemp -d)"
tracker() {
  HOME="$sandbox" XDG_CONFIG_HOME="$sandbox/config" \
    "$backend" --data-path "$sandbox/ledger.json" "$@"
}
tracker template create retry-test --from detailed
tracker project update project-unassigned --template-id user:retry-test
tracker report export weekly
tracker status --json
# Record templateBundle/pdfPath; edit the source shown by the next command.
tracker template path user:retry-test
# After deleting only the isolated report's PDF:
tracker report retry
```

With no remote configured, the report is expected to remain failed at the upload
step, while its rendered PDF remains available for inspection.
