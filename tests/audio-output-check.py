"""Record only a temporary null sink to verify actual Qt audio samples.

Requires a running PipeWire/PulseAudio server, pactl and parec. Only this
component's null sink is recorded. The temporary sink has low priority; if
audio policy selects it anyway, restore the previous output on cleanup.
"""
import array
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

root = Path(__file__).resolve().parents[1]
name = f"omatracker_audio_test_{os.getpid()}"
original_default = subprocess.check_output(["pactl", "get-default-sink"], text=True).strip()
module = subprocess.check_output([
    "pactl", "load-module", "module-null-sink", f"sink_name={name}",
    "sink_properties=device.description=OmaTrackerAudioTest priority.session=0",
], text=True).strip()
try:
    if subprocess.check_output(["pactl", "get-default-sink"], text=True).strip() == name:
        subprocess.run(["pactl", "set-default-sink", original_default], check=True)
    with tempfile.TemporaryDirectory(prefix="omatracker-audio-") as temporary:
        work = Path(temporary)
        plugin = work / "plugin"
        plugin.mkdir()
        shutil.copyfile(root / "HourlySound.qml", plugin / "HourlySound.qml")
        shutil.copytree(root / "sounds", plugin / "sounds")
        shutil.copyfile(root / "tests/AudioOutputTest.qml", work / "shell.qml")
        with (work / "capture.pcm").open("wb") as pcm:
            capture = subprocess.Popen([
                "parec", f"--device={name}.monitor", "--raw", "--format=float32le",
                "--rate=48000", "--channels=2", "--latency-msec=10",
            ], stdout=pcm)
            try:
                time.sleep(0.2)
                result = subprocess.run([
                    "quickshell", "--no-color", "--path", str(work),
                ], env=dict(os.environ, QT_QPA_PLATFORM="offscreen"), timeout=15,
                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
                time.sleep(0.3)
            finally:
                capture.terminate()
                capture.wait(timeout=5)
        print(result.stdout, end="")
        if result.returncode or "ERROR" in result.stdout or "Audio completed 3" not in result.stdout:
            raise SystemExit("Audio component did not finish its playback attempts")
        samples = array.array("f", (work / "capture.pcm").read_bytes())
        peak = max(map(abs, samples), default=0)
        nonzero = sum(abs(sample) > 0.0001 for sample in samples)
        print(f"Captured peak={peak:.5f}, audible samples={nonzero}")
        if peak < 0.001 or nonzero < 100:
            raise SystemExit("Qt requested playback but emitted no usable audio")
        # Count separated audible bursts: one successful attempt must not hide
        # silent first/subsequent plays. 10 ms windows ignore sub-cycle zeros.
        active = [max(map(abs, samples[i:i + 960]), default=0) > 0.0001
                  for i in range(0, len(samples), 960)]
        bursts = sum(on and (i == 0 or not active[i - 1]) for i, on in enumerate(active))
        if bursts != 3:
            raise SystemExit(f"Expected three recorded clicks, got {bursts}")
        print("Three completed plays produced three non-silent output bursts")
finally:
    selected_test_sink = subprocess.check_output(["pactl", "get-default-sink"], text=True).strip() == name
    subprocess.run(["pactl", "unload-module", module], check=True)
    if selected_test_sink:
        sinks = subprocess.check_output(["pactl", "list", "short", "sinks"], text=True)
        if any(line.split()[1] == original_default for line in sinks.splitlines()):
            subprocess.run(["pactl", "set-default-sink", original_default], check=True)
