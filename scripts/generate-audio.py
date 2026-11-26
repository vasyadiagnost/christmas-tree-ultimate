"""Rebuild only the procedural tractor loop. User-provided Santa bells are never overwritten."""

from array import array
import json
import math
from pathlib import Path
import random
import sys
import wave


RATE = 44_100
SECONDS = 8
COUNT = RATE * SECONDS
TAU = 2.0 * math.pi
DESTINATION = Path(__file__).resolve().parents[1] / "windows" / "assets" / "audio"


def tractor():
    """A muffled motor and soft snow brushing. Circular noise filtering joins the loop cleanly."""
    rng = random.Random(2026111202)
    noise = [rng.uniform(-1.0, 1.0) for _ in range(COUNT)]
    fast = slow = 0.0
    # The first complete circle establishes the periodic filter state, rather than fading from silence.
    for sample in noise:
        fast += 0.14 * (sample - fast)
        slow += 0.022 * (sample - slow)
    output = []
    for index, sample in enumerate(noise):
        fast += 0.14 * (sample - fast)
        slow += 0.022 * (sample - slow)
        time = index / RATE
        pulse = 0.87 + 0.08 * math.sin(TAU * 7.0 * time) + 0.05 * math.sin(TAU * 11.0 * time)
        motor = (
            0.42 * math.sin(TAU * 72.0 * time)
            + 0.23 * math.sin(TAU * 144.0 * time + 0.3)
            + 0.10 * math.sin(TAU * 216.0 * time + 0.7)
            + 0.045 * math.sin(TAU * 360.0 * time + 1.2)
        )
        brush = (fast - slow) * (0.80 + 0.12 * math.sin(TAU * 1.375 * time))
        output.append(motor * pulse + brush * 0.65)
    return output


def write_and_validate(name, raw, peak):
    if name != "tractor-snow.wav":
        raise ValueError("This generator must not overwrite imported Santa bells or other audio assets.")
    dc = sum(raw) / len(raw)
    centered = [sample - dc for sample in raw]
    scale = peak / max(abs(sample) for sample in centered)
    samples = array("h", (round(sample * scale * 32767.0) for sample in centered))
    if sys.byteorder != "little":
        samples.byteswap()
    path = DESTINATION / name
    with wave.open(str(path), "wb") as sound:
        sound.setnchannels(1)
        sound.setsampwidth(2)
        sound.setframerate(RATE)
        sound.writeframes(samples.tobytes())
    with wave.open(str(path), "rb") as sound:
        assert (sound.getnchannels(), sound.getsampwidth(), sound.getframerate(), sound.getnframes()) == (1, 2, RATE, COUNT)
        assert sound.getcomptype() == "NONE"
        pcm = array("h")
        pcm.frombytes(sound.readframes(COUNT))
        if sys.byteorder != "little":
            pcm.byteswap()
    values = [sample / 32768.0 for sample in pcm]
    deltas = [values[index] - values[index - 1] for index in range(1, len(values))]
    ordered_deltas = sorted(abs(delta) for delta in deltas)
    seam = abs(values[0] - values[-1])
    p99_delta = ordered_deltas[int(len(ordered_deltas) * 0.99)]
    measured_peak = max(abs(sample) for sample in values)
    rms = math.sqrt(sum(sample * sample for sample in values) / COUNT)
    assert 0.01 < rms < 0.4
    assert measured_peak <= 0.6
    assert max(abs(sample) for sample in pcm) < 32767
    assert seam <= max(0.001, p99_delta * 2.0), f"Discontinuous loop: {path}: {seam}"
    return {
        "file": str(path.relative_to(DESTINATION.parent.parent.parent)),
        "seconds": SECONDS,
        "rate": RATE,
        "channels": 1,
        "bits": 16,
        "peak": round(measured_peak, 6),
        "rms": round(rms, 6),
        "boundary_step": round(seam, 6),
        "p99_sample_step": round(p99_delta, 6),
        "clipped_samples": sum(abs(sample) >= 32767 for sample in pcm),
    }


def main():
    DESTINATION.mkdir(parents=True, exist_ok=True)
    results = [write_and_validate("tractor-snow.wav", tractor(), 0.36)]
    print(json.dumps(results, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
