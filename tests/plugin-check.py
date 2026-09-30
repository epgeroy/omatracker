"""Validate the publishable runtime, not source-only skill symlinks."""
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="omatracker-plugin-check-") as temp:
    plugin = Path(temp)
    for source in [root / "manifest.json", *root.glob("*.qml"), *root.glob("*.js")]:
        shutil.copy2(source, plugin / source.name)
    for folder in ("sounds", "templates"):
        shutil.copytree(root / folder, plugin / folder)
    (plugin / "bin").mkdir()
    shutil.copy2(root / "bin/omatracker", plugin / "bin/omatracker")
    subprocess.run(["omarchy", "plugin", "validate", str(plugin)], check=True)
