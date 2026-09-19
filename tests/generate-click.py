"""Reproducible wooden click with output startup/drain padding.

The audible body is 150 ms. Its fuller decay stays perceptible on headphones;
silence around it gives short-lived audio streams time to start and drain.
No downloaded/licensed assets.
"""
import math
import random
import struct
import wave
from pathlib import Path

random.seed(7)
rate = 44100
samples = [0] * int(rate * 0.15)
for i in range(int(rate * 0.15)):
    t = i / rate
    attack = min(1.0, t / 0.0008)
    tone = (math.sin(2 * math.pi * 740 * t) * math.exp(-t * 40)
            + 0.35 * math.sin(2 * math.pi * 1290 * t) * math.exp(-t * 80)
            + 0.16 * random.uniform(-1, 1) * math.exp(-t * 350))
    samples.append(int(22000 * attack * tone))
samples.extend([0] * int(rate * 0.2))
destination = Path(__file__).resolve().parents[1] / "sounds" / "wood-click.wav"
destination.parent.mkdir(exist_ok=True)
with wave.open(str(destination), "wb") as sound:
    sound.setparams((1, 2, rate, 0, "NONE", "not compressed"))
    sound.writeframes(struct.pack("<" + "h" * len(samples), *samples))
