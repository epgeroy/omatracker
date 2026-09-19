"""Exercise the real Omarchy controls without touching the installed plugin."""
import os
from pathlib import Path
import subprocess
import tempfile
import shutil
import sys
import json
import time

root = Path(__file__).resolve().parents[1]
shell = Path(os.environ.get("OMARCHY_SHELL_DIR", "/usr/share/omarchy/shell"))
preview = "--preview" in sys.argv
with tempfile.TemporaryDirectory(prefix="omatracker-ui-") as work:
    config = Path(work) / "shell"
    shutil.copytree(shell, config)
    plugin = config / "plugin"
    plugin.mkdir()
    for source in list(root.glob("*.qml")) + list(root.glob("*.js")):
        shutil.copyfile(source, plugin / source.name)
    shutil.copytree(root / "bin", plugin / "bin")
    shutil.copytree(root / "sounds", plugin / "sounds")
    shutil.copytree(root / "templates", plugin / "templates")
    shutil.copyfile(root / ("tests/Preview.qml" if preview else "tests/UiTest.qml"), config / "shell.qml")
    platform = "wayland" if preview or "--wayland" in sys.argv else "offscreen"
    env = dict(os.environ, QT_QPA_PLATFORM=platform, QT_QUICK_BACKEND="software")
    env["OMATRACKER_UI_RESULT"] = str(Path(work) / "passed")
    env["OMATRACKER_UI_LEDGER"] = str(Path(work) / "ledger.json")
    env["OMATRACKER_PREVIEW_SMOKE"] = "1" if "--smoke" in sys.argv else ""
    if preview:
        home = Path(work) / "home"
        theme = home / ".local/state/omarchy/current/theme"
        theme.mkdir(parents=True)
        for name in ["colors.toml", "shell.toml"]:
            source = Path.home() / ".local/state/omarchy/current/theme" / name
            if source.exists():
                shutil.copyfile(source, theme / name)
        env.update(HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"))
        backend = [str(plugin / "bin/omatracker"), "--data-path", env["OMATRACKER_UI_LEDGER"]]
        project_id = subprocess.check_output(backend + ["project", "create", "OmaTracker"], env=env, text=True).strip()
        subprocess.run(backend + ["project", "update", project_id, "--client-name", "Disposable preview", "--export-weekly", "false", "--export-monthly", "false"], env=env, check=True)
        for title in ["Making OmaTracker production ready", "Documentation", "Release checklist"]:
            subprocess.run(backend + ["task", "add", title], env=env, check=True, stdout=subprocess.DEVNULL)
        ledger = Path(env["OMATRACKER_UI_LEDGER"])
        state = json.loads(ledger.read_text())
        age = 3585 if "--hour-demo" in sys.argv else 608
        state["tasks"][0].update(running=True, startedAt=int(time.time() * 1000) - age * 1000)
        ledger.write_text(json.dumps(state))
        if "--smoke" in sys.argv:
            subprocess.run(backend + ["feedback", "configure", "--hourly-click", "true", "--volume", "0", "--reduced-motion", "false"], env=env, check=True)
        print("Preview uses disposable data: " + str(ledger), flush=True)
        print("Close the window or press Esc to finish. All preview data is removed on exit.", flush=True)
        result = subprocess.run(["quickshell", "--no-color", "--path", str(config)], env=env,
                                timeout=45 if "--smoke" in sys.argv else None)
        if "--smoke" in sys.argv and not Path(env["OMATRACKER_UI_RESULT"]).exists():
            raise SystemExit("Hourly preview smoke test did not reach its milestone")
        raise SystemExit(result.returncode)
    result = subprocess.run(["quickshell", "--no-color", "--path", str(config)], env=env,
                            timeout=45, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    print(result.stdout, end="")
    for error in ["TypeError", "ReferenceError", "Binding loop", "Cannot assign", "is not a type", "FATAL", "ERROR"]:
        if error in result.stdout:
            raise SystemExit("Runtime UI error: " + error)
    if not (Path(work) / "passed").exists():
        raise SystemExit("UI tests did not produce a passing result")
    raise SystemExit(result.returncode)
