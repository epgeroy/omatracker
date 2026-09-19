"""Check the installed CLI outside the repository, with an isolated user home."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="omatracker-install-") as temporary:
    work = Path(temporary)
    bindir = work / "local bin"
    datadir = work / "application data" / "omatracker"
    env = dict(os.environ, HOME=str(work / "home"), XDG_CONFIG_HOME=str(work / "config"),
               OMATRACKER_DATA_PATH=str(work / "ledger.json"))
    env.pop("OMATRACKER_TEMPLATE_DIR", None)
    env.pop("CLAUDE_CONFIG_DIR", None)
    env["PATH"] = str(bindir) + os.pathsep + env["PATH"]
    install = ["make", "--no-print-directory", "install-bin",
               f"BINDIR={bindir}", f"DATADIR={datadir}"]
    subprocess.run(install, cwd=root, env=env, check=True)
    assert (bindir / "omatracker").resolve() == datadir / "bin" / "omatracker"

    def cli(*args):
        return subprocess.check_output(["omatracker", *args], cwd=work, env=env, text=True)

    assert cli("--version").startswith("omatracker ")
    assert json.loads(cli("agent", "help"))["ok"]
    assert "valid" in cli("template", "validate", "invoice").lower()
    result = json.loads(cli("skill", "install", "--harness", "codex", "--json"))
    skill = Path(result["installations"][0]["path"])
    instructions = (skill / "references" / "installation.md").read_text()
    assert str(datadir / "bin" / "omatracker") in instructions
    assert (skill / "AGENT_API.md").is_file()
    assert "work.record-batch" in (skill / "AGENT_API.md").read_text()
    assert "work.record-batch" in (skill / "references" / "workflows.md").read_text()
    assert "work.record-batch" in json.loads(cli("agent", "help"))["data"]["actions"]
    assert not (work / "ledger.json").exists()
    assert not (work / "ledger.json.lock").exists()

    # Reinstalling updates the independent copy and keeps command resolution intact.
    subprocess.run(install, cwd=root, env=env, check=True)
    assert json.loads(cli("agent", "help"))["ok"]

    # Simulate an older widget checkout, then deploy the matching runtime/backend.
    plugin = work / "plugin"
    (plugin / "bin").mkdir(parents=True)
    (plugin / "manifest.json").write_text(json.dumps({"id": "epgeroy.omatracker", "version": "old"}))
    (plugin / "bin/omatracker").write_text("old backend")
    (plugin / "Service.qml").write_text("old service")
    (plugin / "personal-notes.txt").write_text("keep this")
    backups = work / "plugin backups"
    subprocess.run(["make", "--no-print-directory", "install-plugin-bin", f"BINDIR={bindir}",
                    f"DATADIR={datadir}", f"PLUGIN_DIR={plugin}", f"PLUGIN_BACKUP_DIR={backups}"],
                   cwd=root, env=env, check=True)
    assert (plugin / "bin/omatracker").resolve() == (bindir / "omatracker").resolve()
    assert (plugin / "Service.qml").read_bytes() == (root / "Service.qml").read_bytes()
    assert (plugin / "personal-notes.txt").read_text() == "keep this"
    backup = next(backups.iterdir())
    assert (backup / "bin/omatracker").read_text() == "old backend"
    assert (backup / "Service.qml").read_text() == "old service"

    project = json.loads(cli("agent", "project.create", "--input", '{"name":"Deleted from CLI"}'))["data"]["id"]
    cli("agent", "project.remove", "--input", json.dumps({"project": project}))
    widget_status = json.loads(subprocess.check_output([str(plugin / "bin/omatracker"), "--data-path",
                              str(work / "ledger.json"), "status", "--json", "--compact"],
                              cwd=work, env=env, text=True))
    assert all(p["id"] != project for p in widget_status["state"]["projects"])
    print("Installed CLI, templates, skill, widget alignment, and external deletion checks passed")
