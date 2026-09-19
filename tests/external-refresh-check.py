"""Verify external CLI deletion reaches the real QML service without an explicit refresh."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="omatracker-external-refresh-") as temporary:
    work = Path(temporary)
    env = dict(os.environ, HOME=str(work), XDG_CONFIG_HOME=str(work / "config"),
               QT_QPA_PLATFORM="offscreen", OMATRACKER_TEST_DIR=str(work))
    backend = [str(root / "bin/omatracker"), "--data-path", str(work / "external.json")]

    def agent(action, **input):
        return json.loads(subprocess.check_output(backend + ["agent", action, "--input", json.dumps(input)],
                                                 env=env, text=True))["data"]

    client = agent("client.set", details={"name": "External client"})["id"]
    project = agent("project.create", name="External project", client=client)["id"]
    task = agent("task.create", project=project, title="External task")["id"]
    subprocess.run(backend + ["project", "select", project], env=env, check=True)
    env.update(OMATRACKER_REFRESH_CLIENT=client, OMATRACKER_REFRESH_PROJECT=project,
               OMATRACKER_REFRESH_TASK=task)
    result = subprocess.run(["quickshell", "--no-color", "--path", str(root / "ExternalChangesTest.qml")],
                            cwd=root, env=env, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=25)
    print(result.stdout, end="")
    if result.returncode or not (work / "external-refresh-passed").is_file():
        raise SystemExit("External CLI changes did not reach the widget")
    assert agent("client.list")["total"] == 0
    assert agent("project.get", project=project)["archived"]
