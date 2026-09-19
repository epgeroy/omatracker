"""Reproducible, original 90 ms wooden click. No downloaded/licensed assets."""
import math
import random
import struct
import wave
from pathlib import Path

random.seed(7)
rate = 44100
samples = []
for i in range(int(rate * 0.09)):
    t = i / rate
    attack = min(1.0, t / 0.0008)
    tone = (math.sin(2 * math.pi * 740 * t) * math.exp(-t * 100)
            + 0.35 * math.sin(2 * math.pi * 1290 * t) * math.exp(-t * 160)
            + 0.16 * random.uniform(-1, 1) * math.exp(-t * 350))
    samples.append(int(16000 * attack * tone))
destination = Path(__file__).resolve().parents[1] / "sounds" / "wood-click.wav"
destination.parent.mkdir(exist_ok=True)
with wave.open(str(destination), "wb") as sound:
    sound.setparams((1, 2, rate, 0, "NONE", "not compressed"))
    sound.writeframes(struct.pack("<" + "h" * len(samples), *samples))
