"""Prepare the continuous Santa bell loop locally from the original MP3.

Only quiet outer edges and a 40 ms overlap are edited. Embedded PCM comes directly
from the original decode. The separately exported MP3 is never decoded into the app.
"""

from array import array
import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys
import wave


ROOT = Path(__file__).resolve().parents[1]
RATE = 44_100
SOURCE_SHA256 = "f555288d54f88bdc66689286e3fca04bfc56f98d1ada82ed0986ac0f53f27b49"
LOOP_SHA256 = "552a25be50820311abb9644ced9253edbf7d839f8319bb407d14b77771051456"
TRACTOR_SHA256 = "52fe63ea35eeba66dafd80d21432cbe929b608c8530ea77075fde8a146c3f475"
START = 1764  # 40 ms: discard the MP3's quiet lead-in.
END = 268128  # 6.080 s: discard the final 32.65 ms of encoded silence.
OVERLAP = 1764  # 40 ms: join the remaining tail and head with equal-power weights.


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(args):
    result = subprocess.run(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    if result.returncode:
        raise RuntimeError(result.stderr.decode("utf-8", errors="replace"))
    return result.stdout


def pcm16(samples):
    if max(abs(value) for value in samples) >= 1.0:
        raise ValueError("Loop would clip; do not silently normalize or change the recording's volume.")
    output = array("h", (max(-32768, min(32767, round(value * 32768.0))) for value in samples))
    if sys.byteorder != "little":
        output.byteswap()
    return output.tobytes()


def write_wave(path, raw):
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as sound:
        sound.setnchannels(1)
        sound.setsampwidth(2)
        sound.setframerate(RATE)
        sound.writeframes(raw)


def samples_from_pcm(raw):
    samples = array("h")
    samples.frombytes(raw)
    if sys.byteorder != "little":
        samples.byteswap()
    return [sample / 32768.0 for sample in samples]


def rms(samples):
    return math.sqrt(sum(value * value for value in samples) / len(samples))


def measures(samples):
    block = RATE // 100
    powers = [rms(samples[index:index + block]) for index in range(0, len(samples) - block + 1, block)]
    quiet = [value < 0.001 for value in powers]
    longest = current = 0
    for silent in quiet + quiet:
        current = current + 1 if silent else 0
        longest = max(longest, current)
    differences = sorted(abs(samples[index] - samples[index - 1]) for index in range(1, len(samples)))
    return {
        "frames": len(samples),
        "seconds": len(samples) / RATE,
        "peak": max(abs(value) for value in samples),
        "rms": rms(samples),
        "clipped_samples": sum(abs(value) >= 32767.0 / 32768.0 for value in samples),
        "longest_quiet_run_across_wrap_ms": min(longest, len(quiet)) * 10,
        "quiet_threshold_rms": 0.001,
        "first_10ms_rms": rms(samples[:block]),
        "last_10ms_rms": rms(samples[-block:]),
        "wrap_100ms_rms": rms(samples[-RATE // 20:] + samples[:RATE // 20]),
        "wrap_sample_step": abs(samples[0] - samples[-1]),
        "p99_ordinary_sample_step": differences[int(len(differences) * 0.99)],
    }


def restore_release_loop(exe, destination):
    """Recover the exact embedded loop for a local source build."""
    data = exe.read_bytes()
    size = 44 + 6 * RATE * 2
    header = b"RIFF" + (size - 8).to_bytes(4, "little") + b"WAVE"
    start = data.find(header)
    while start != -1:
        candidate = data[start:start + size]
        if hashlib.sha256(candidate).hexdigest() == LOOP_SHA256:
            destination.write_bytes(candidate)
            print(f"Prepared {destination.relative_to(ROOT)} (SHA-256 {LOOP_SHA256}).")
            return
        start = data.find(header, start + len(header))
    raise ValueError("The executable does not contain the verified 0.2.2 Santa loop.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    inputs = parser.add_mutually_exclusive_group(required=True)
    inputs.add_argument("--source", type=Path, help="original Sleigh Bells MP3 (see RELEASING.md)")
    inputs.add_argument("--from-release", type=Path, help="0.2.2 app or setup executable")
    parser.add_argument("--ffmpeg", type=Path, default=ROOT / ".tools" / "ffmpeg" / "ffmpeg.exe")
    arguments = parser.parse_args()
    destination = ROOT / "windows" / "assets" / "audio" / "santa-bells.wav"
    if arguments.from_release is not None:
        restore_release_loop(arguments.from_release, destination)
        return

    source = arguments.source.resolve()
    tractor = ROOT / "windows" / "assets" / "audio" / "tractor-snow.wav"
    preview = ROOT / ".tools" / "audio-previews" / "santa-bells-loop-3x.wav"
    edited_mp3 = preview.with_name("sleigh-bells-loop.mp3")
    if sha256(source) != SOURCE_SHA256 or sha256(tractor) != TRACTOR_SHA256:
        raise ValueError("Original MP3 or unchanged tractor asset differs from its expected SHA256.")

    decoded = command([
        str(arguments.ffmpeg), "-hide_banner", "-loglevel", "error", "-nostdin",
        "-i", str(source), "-map", "0:a:0", "-vn", "-ar", str(RATE),
        # An explicit stereo average matches the existing integer-PCM downmix. FFmpeg's
        # default float downmix otherwise uses -3 dB per channel and raises the sum by 3 dB.
        "-af", "pan=mono|c0=0.5*c0+0.5*c1",
        "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1",
    ])
    floats = array("f")
    floats.frombytes(decoded)
    if sys.byteorder != "little":
        floats.byteswap()
    original = list(floats)
    if len(original) != 269568:
        raise ValueError("Unexpected decoded recording length; reassess cut points before editing.")

    trimmed = original[START:END]
    overlap = [
        trimmed[-OVERLAP + index] * math.cos(index / (OVERLAP - 1) * math.pi / 2.0)
        + trimmed[index] * math.sin(index / (OVERLAP - 1) * math.pi / 2.0)
        for index in range(OVERLAP)
    ]
    # Rotate the cyclic start past the incoming overlap. The wrap is then two adjacent
    # original samples, rather than a fresh cut: head[OVERLAP-1] -> head[OVERLAP].
    processed = trimmed[OVERLAP:-OVERLAP] + overlap
    raw = pcm16(processed)
    quantized = samples_from_pcm(raw)
    before = measures(samples_from_pcm(pcm16(original)))
    after = measures(quantized)
    gain_change_db = 20.0 * math.log10(after["rms"] / before["rms"])
    if len(processed) != RATE * 6 or after["clipped_samples"] or after["longest_quiet_run_across_wrap_ms"] != 0:
        raise ValueError("The edited loop failed duration, clipping, or continuous-energy checks.")
    if abs(gain_change_db) > 0.2 or abs(after["peak"] - before["peak"]) > 0.001:
        raise ValueError("The edit changed the recording's overall level unexpectedly.")
    if after["wrap_sample_step"] > after["p99_ordinary_sample_step"]:
        raise ValueError("Unexpected waveform discontinuity at the cyclic wrap.")

    write_wave(destination, raw)
    write_wave(preview, raw * 3)
    command([
        str(arguments.ffmpeg), "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
        "-i", str(destination), "-map", "0:a:0", "-c:a", "libmp3lame", "-b:a", "256k",
        "-map_metadata", "-1", "-write_xing", "1", str(edited_mp3),
    ])
    # Check LAME/Xing delay and padding information by decoding the exported MP3.
    # This check never feeds that lossy re-encode back into the embedded WAV.
    exported_pcm = command([
        str(arguments.ffmpeg), "-hide_banner", "-loglevel", "error", "-nostdin",
        "-i", str(edited_mp3), "-map", "0:a:0", "-ar", str(RATE), "-ac", "1",
        "-c:a", "pcm_s16le", "-f", "s16le", "pipe:1",
    ])
    exported_measures = measures(samples_from_pcm(exported_pcm))
    if exported_measures["frames"] != len(processed) or exported_measures["clipped_samples"]:
        raise ValueError("Edited MP3 did not preserve the loop length without clipping.")
    if sha256(source) != SOURCE_SHA256 or sha256(tractor) != TRACTOR_SHA256:
        raise ValueError("An unrelated original asset changed.")

    fingerprint = 14695981039346656037
    for byte in raw:
        fingerprint = ((fingerprint ^ byte) * 1099511628211) & ((1 << 64) - 1)
    report = {
        "original": before,
        "loop": after,
        "edited_mp3_decoded": exported_measures,
        "overall_rms_change_db": gain_change_db,
        "trim_start_ms": START / RATE * 1000,
        "trim_end_ms": (len(original) - END) / RATE * 1000,
        "equal_power_overlap_ms": OVERLAP / RATE * 1000,
        "pcm_fnv1a64": f"{fingerprint:016x}",
        "wav_sha256": sha256(destination),
        "edited_mp3_sha256": sha256(edited_mp3),
        "source_mp3_sha256": sha256(source),
        "tractor_sha256": sha256(tractor),
        "preview": str(preview.relative_to(ROOT)),
        "notes": "No normalization, tempo/pitch processing, synthetic sound, or automatic playback.",
    }
    preview.with_suffix(".json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
