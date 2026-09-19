"""Deploy widget runtime files and pin its backend to the user-installed CLI."""
import argparse
import json
import os
from pathlib import Path
import shutil
import tempfile
import uuid

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--source", type=Path, required=True)
parser.add_argument("--destination", type=Path, required=True)
parser.add_argument("--backend", type=Path, required=True)
parser.add_argument("--backup-root", type=Path, required=True)
args = parser.parse_args()
source = args.source.resolve()
destination = args.destination.absolute()
backend = args.backend.resolve(strict=True)
backup_root = args.backup_root.absolute()

if destination.is_symlink() or destination.resolve() == source:
    raise SystemExit("Use a separate, non-symlink plugin destination")
if backup_root.resolve().is_relative_to(destination.resolve()):
    raise SystemExit("Plugin backups must be outside the plugin directory")
manifest = json.loads((source / "manifest.json").read_text())
if manifest["id"] != "epgeroy.omatracker":
    raise SystemExit("Source is not the OmaTracker plugin")
if not os.access(backend, os.X_OK):
    raise SystemExit("Install the standalone CLI first")
if destination.exists():
    existing = json.loads((destination / "manifest.json").read_text())
    if existing["id"] != manifest["id"]:
        raise SystemExit("Refusing to overwrite a different plugin")

files = [p for pattern in ("*.qml", "*.js") for p in source.glob(pattern)]
for folder in ("templates", "sounds"):
    files.extend(p for p in (source / folder).rglob("*") if p.is_file())
files.append(source / "manifest.json")  # Publish the manifest last.
for file in files:
    if file.is_symlink():
        raise SystemExit(f"Runtime source must be a regular file: {file}")
    target = destination / file.relative_to(source)
    for parent in target.parents:
        if parent == destination:
            break
        if parent.is_symlink():
            raise SystemExit(f"Runtime directory is a symlink: {parent}")
if (destination / "bin").is_symlink():
    raise SystemExit("Plugin bin directory must not be a symlink")

backup = None
if destination.exists():
    backup_root.mkdir(parents=True, exist_ok=True)
    backup = backup_root / f"omatracker-{uuid.uuid4()}"
    shutil.copytree(destination, backup, symlinks=True,
                    ignore=shutil.ignore_patterns(".git", "target"))

destination.mkdir(parents=True, exist_ok=True)
(destination / "bin").mkdir(exist_ok=True)
link = destination / "bin" / f".omatracker-{uuid.uuid4()}"
try:
    link.symlink_to(backend)
    os.replace(link, destination / "bin" / "omatracker")
finally:
    link.unlink(missing_ok=True)

# Replace individual files atomically; preserve unrelated plugin files and .git.
for file in files:
    target = destination / file.relative_to(source)
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as stream:
        temporary = Path(stream.name)
    try:
        shutil.copy2(file, temporary)
        os.replace(temporary, target)
    finally:
        temporary.unlink(missing_ok=True)

print(json.dumps({"plugin": str(destination), "backend": str(backend),
                  "backup": str(backup) if backup else None,
                  "nextStep": "omarchy restart shell"}, indent=2))
