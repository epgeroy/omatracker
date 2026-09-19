"""Exercise the real Omarchy controls without touching the installed plugin."""
import os
from pathlib import Path
import subprocess
import tempfile
import shutil
import sys

root = Path(__file__).resolve().parents[1]
shell = Path(os.environ.get("OMARCHY_SHELL_DIR", "/usr/share/omarchy/shell"))
with tempfile.TemporaryDirectory(prefix="omatracker-ui-") as work:
    config = Path(work) / "shell"
    shutil.copytree(shell, config)
    plugin = config / "plugin"
    plugin.mkdir()
    for source in list(root.glob("*.qml")) + list(root.glob("*.js")):
        shutil.copyfile(source, plugin / source.name)
    shutil.copytree(root / "bin", plugin / "bin")
    shutil.copytree(root / "sounds", plugin / "sounds")
    shutil.copyfile(root / "tests/UiTest.qml", config / "shell.qml")
    platform = "wayland" if "--wayland" in sys.argv else "offscreen"
    env = dict(os.environ, QT_QPA_PLATFORM=platform, QT_QUICK_BACKEND="software")
    env["OMATRACKER_UI_RESULT"] = str(Path(work) / "passed")
    env["OMATRACKER_UI_LEDGER"] = str(Path(work) / "ledger.json")
    result = subprocess.run(["quickshell", "--no-color", "--path", str(config)], env=env,
                            timeout=45, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    print(result.stdout, end="")
    for error in ["TypeError", "ReferenceError", "Binding loop", "Cannot assign", "is not a type", "FATAL", "ERROR"]:
        if error in result.stdout:
            raise SystemExit("Runtime UI error: " + error)
    if not (Path(work) / "passed").exists():
        raise SystemExit("UI tests did not produce a passing result")
    raise SystemExit(result.returncode)
