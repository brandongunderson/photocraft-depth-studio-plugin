#!/usr/bin/env python3
"""Exercise the actual packaged worker with synthetic images.

Developer-only dependencies: Python 3.11+, Pillow, NumPy. No installed PhotoCraft
instance or user documents are used. Generates diagnostics beneath verification/.
Includes independently recorded sample-pixel goldens. If the optional full-frame
reference from verification/model-reference.py exists, compares every map pixel.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import time
import zipfile

import numpy as np
from PIL import Image, ImageDraw


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OPTIONS = {"output": "map", "focus": 0.65, "range": 0.1, "radius": 16, "side": "background", "invert": False}


def synthetic_fixture():
    """Original fixture matching the independent ONNX reference, no remote images."""
    y, _ = np.mgrid[0:518, 0:518]
    data = np.empty((518, 518, 3), dtype=np.uint8)
    data[:, :, 0] = np.clip(80 + 95 * y / 517, 0, 255)
    data[:, :, 1] = np.clip(130 + 65 * y / 517, 0, 255)
    data[:, :, 2] = np.clip(220 - 70 * y / 517, 0, 255)
    image = Image.fromarray(data)
    drawing = ImageDraw.Draw(image)
    drawing.polygon([(0, 280), (518, 280), (518, 518), (0, 518)], fill=(115, 122, 104))
    for side in range(-3, 5):
        drawing.line([(259, 280), (259 + side * 150, 518)], fill=(190, 188, 165), width=3)
    for horizontal in [305, 340, 395, 475]:
        drawing.line([(0, horizontal), (518, horizontal)], fill=(190, 188, 165), width=3)
    drawing.rectangle((320, 180, 395, 310), fill=(155, 91, 66))
    drawing.polygon([(320, 180), (357, 153), (432, 160), (395, 180)], fill=(195, 128, 85))
    drawing.polygon([(395, 180), (432, 160), (432, 284), (395, 310)], fill=(108, 65, 48))
    drawing.ellipse((60, 305, 262, 480), fill=(41, 80, 146))
    drawing.ellipse((80, 322, 234, 455), fill=(49, 111, 196))
    drawing.ellipse((109, 340, 167, 388), fill=(120, 179, 235))
    return image


class ProcessMemoryCounters(ctypes.Structure):
    _fields_ = [
        ("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD),
        ("PeakWorkingSetSize", ctypes.c_size_t), ("WorkingSetSize", ctypes.c_size_t),
        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t), ("QuotaPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t), ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
        ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t),
    ]


def process_memory(process):
    if os.name != "nt":
        return {}
    getter = ctypes.WinDLL("psapi", use_last_error=True).GetProcessMemoryInfo
    getter.argtypes = [wintypes.HANDLE, ctypes.POINTER(ProcessMemoryCounters), wintypes.DWORD]
    getter.restype = wintypes.BOOL
    counters = ProcessMemoryCounters()
    counters.cb = ctypes.sizeof(counters)
    if not getter(int(process._handle), ctypes.byref(counters), counters.cb):
        return {}
    return {"peakWorkingSetBytes": counters.PeakWorkingSetSize, "peakPrivateCommitBytes": counters.PeakPagefileUsage}


def extract_checked(package, destination):
    destination = destination.resolve()
    destination.mkdir(parents=True)
    total = 0
    seen = set()
    with zipfile.ZipFile(package) as archive:
        for member in archive.infolist():
            pure = PurePosixPath(member.filename)
            if pure.is_absolute() or not pure.parts or any(p in (".", "..") for p in pure.parts) or "\\" in member.filename or ":" in member.filename:
                raise ValueError("Unsafe archive path")
            target = destination.joinpath(*pure.parts).resolve()
            if not target.is_relative_to(destination) or target == destination:
                raise ValueError("Archive path escapes the test installation")
            if str(target).casefold() in seen or (member.external_attr >> 16) & 0o170000 == 0o120000:
                raise ValueError("Duplicate archive path or symbolic link")
            seen.add(str(target).casefold())
            total += member.file_size
            if total > 512 * 1024 * 1024 or member.file_size > 256 * 1024 * 1024:
                raise ValueError("Package exceeds test extraction limits")
            if member.is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(member) as source, target.open("wb") as output:
                    shutil.copyfileobj(source, output)


def execute(worker, directory, name, source, options=None, size=None, mutate=None, cancel_progress=None, timeout=120):
    image = Image.open(source)
    dimensions = image.size
    image.close()
    result = directory / f"{name}.png"
    request = {"apiVersion": 1, "input": str(source.resolve()), "output": str(result.resolve()),
               "outputSize": {"width": dimensions[0], "height": dimensions[1]}, "colorSpace": "srgb",
               "options": {**DEFAULT_OPTIONS, **(options or {})}}
    if size:
        request["outputSize"] = {"width": size[0], "height": size[1]}
    if mutate:
        mutate(request)
    job = directory / f"{name}.json"
    job.write_text(json.dumps(request), encoding="utf-8")
    log = directory / f"{name}.log"
    environment = os.environ.copy()
    windows = Path(environment.get("SystemRoot", r"C:\Windows"))
    environment["PATH"] = os.pathsep.join([str(windows / "System32"), str(windows)])
    environment.pop("PYTHONPATH", None)
    environment.pop("ORT_DYLIB_PATH", None)
    memory = {}
    started = time.perf_counter()
    cancelled_at = None
    with log.open("wb") as output:
        executable = "\\\\?\\" + str(worker.resolve()) if os.name == "nt" else str(worker)
        process = subprocess.Popen([executable, "--job", str(job)], executable=executable, cwd=directory,
                                   stdin=subprocess.PIPE, stdout=output, stderr=subprocess.STDOUT, env=environment)
        try:
            while process.poll() is None:
                for key, value in process_memory(process).items():
                    memory[key] = max(memory.get(key, 0), value)
                if time.perf_counter() - started > timeout:
                    raise TimeoutError(f"Worker timeout: {name}")
                if cancel_progress is not None and cancelled_at is None:
                    for line in log.read_text(encoding="utf-8", errors="replace").splitlines():
                        try:
                            event = json.loads(line)
                        except json.JSONDecodeError:
                            continue
                        if event.get("progress", -1) >= cancel_progress:
                            cancelled_at = time.perf_counter()
                            process.stdin.close()
                            break
                time.sleep(0.01)
            returncode = process.wait()
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=10)
            if process.stdin and not process.stdin.closed:
                process.stdin.close()
    elapsed = time.perf_counter() - started
    log_text = log.read_text(encoding="utf-8", errors="replace")
    record = {"name": name, "exitCode": returncode, "elapsedSeconds": round(elapsed, 4), "outputExists": result.exists(), **memory}
    if cancel_progress is not None:
        record["cancellationTriggered"] = cancelled_at is not None
        record["cancellationLatencySeconds"] = round(time.perf_counter() - cancelled_at, 4) if cancelled_at is not None else None
    return record, result, log_text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=Path, default=ROOT / "dist/PhotoCraft-Depth-Studio.pcplugin")
    parser.add_argument("--skip-performance", action="store_true", help="Skip the 24 MP blur and 36 MP input-bound probes")
    parser.add_argument("--sampled-golden", action="store_true", help="Exercise the portable sample-pixel goldens even if a full-frame reference exists")
    args = parser.parse_args()
    if os.name != "nt":
        raise RuntimeError("This package and deep-path test target Windows x64")
    directory = ROOT / "verification" / ("worker-smoke-files-" + time.strftime("%Y%m%d-%H%M%S"))
    directory.mkdir(parents=True)
    install = directory / ("long-path-installation-" + "a" * 55) / ("separate-plugin-package-" + "b" * 55) / ("nested-runtime-directory-" + "c" * 55)
    extract_checked(args.package, install)
    worker = install / "depth-studio-worker.exe"
    assert len(str(worker)) > 260
    results = []
    reference_depth = ROOT / "verification/model-reference-depth.npy"
    source = directory / "source-opaque.png"
    synthetic_fixture().convert("RGBA").save(source)

    record, mapped, log = execute(worker, directory, "golden-map", source)
    assert record["exitCode"] == 0, log
    rgba = np.asarray(Image.open(mapped).convert("RGBA"))
    if reference_depth.is_file() and not args.sampled_golden:
        depth = np.load(reference_depth)
        expected = np.floor(((depth - depth.min()) / (depth.max() - depth.min())) * 255 + 0.5).astype(np.uint8)
        delta = np.abs(rgba[:, :, 0].astype(np.int16) - expected.astype(np.int16))
        record["goldenMode"] = "full-frame independent ONNX reference"
    else:
        # Independently measured by Python ONNX Runtime 1.22.0 for the pinned
        # afb6a5c2...a1df10c model. Coordinates are (y, x, normalized gray byte).
        samples = [(0, 0, 9), (120, 380, 7), (259, 259, 2), (400, 150, 198), (517, 517, 255)]
        delta = np.asarray([abs(int(rgba[y, x, 0]) - expected) for y, x, expected in samples])
        record["goldenMode"] = "five independently recorded sample pixels"
    assert rgba.shape == (518, 518, 4) and np.all(rgba[:, :, 3] == 255)
    assert np.array_equal(rgba[:, :, 0], rgba[:, :, 1]) and np.array_equal(rgba[:, :, 1], rgba[:, :, 2])
    record["maximumGoldenDifference8bit"] = int(delta.max())
    record["meanGoldenDifference8bit"] = float(delta.mean())
    assert delta.max() <= 1, record
    results.append(record)

    record, inverted, log = execute(worker, directory, "inverted-map", source, {"invert": True})
    assert record["exitCode"] == 0, log
    inverted_array = np.asarray(Image.open(inverted))
    inversion_delta = np.abs(inverted_array[:, :, :3].astype(np.int16) + rgba[:, :, :3].astype(np.int16) - 255)
    assert inversion_delta.max() <= 1
    record["maximumInversionError8bit"] = int(inversion_delta.max())
    results.append(record)

    alpha_source = directory / "source-alpha.png"
    original = np.asarray(Image.open(source)).copy()
    yy, xx = np.indices(original.shape[:2])
    original[:, :, 3] = ((xx * 7 + yy * 13) % 256).astype(np.uint8)
    Image.fromarray(original).save(alpha_source)
    record, blurred, log = execute(worker, directory, "alpha-blur", alpha_source, {"output": "blur", "side": "both", "focus": 0.5, "range": 0.05, "radius": 24})
    assert record["exitCode"] == 0, log
    actual = np.asarray(Image.open(blurred))
    assert actual.shape == original.shape and np.array_equal(actual[:, :, 3], original[:, :, 3])
    changed = int(np.count_nonzero(np.any(actual[:, :, :3] != original[:, :, :3], axis=2)))
    assert changed > 100
    record.update(alphaPreserved=True, changedRgbPixels=changed)
    results.append(record)

    record, identity, log = execute(worker, directory, "radius-zero", alpha_source, {"output": "blur", "radius": 0})
    assert record["exitCode"] == 0 and np.array_equal(np.asarray(Image.open(identity)), original), log
    record["allPixelsExact"] = True
    results.append(record)

    for name, options, size, expected_error in [
        ("invalid-size", None, (517, 518), "dimensions must match"),
        ("invalid-radius", {"radius": 65}, None, "blur radius"),
        ("invalid-focus", {"focus": 1.1}, None, "Focus"),
        ("invalid-mode", {"output": "unknown"}, None, "unknown variant"),
    ]:
        record, output, log = execute(worker, directory, name, source, options, size)
        assert record["exitCode"] != 0 and not output.exists() and expected_error in log, log
        record["expectedRejection"] = True
        results.append(record)

    model = install / "runtime/models/depth-anything-v2-small.onnx"
    hidden = model.with_suffix(".temporarily-absent")
    model.rename(hidden)
    try:
        record, output, log = execute(worker, directory, "missing-model", source)
        assert record["exitCode"] != 0 and not output.exists() and "model is missing" in log, log
        record["expectedRejection"] = True
        results.append(record)
    finally:
        hidden.rename(model)

    record, output, log = execute(worker, directory, "cancel-inference", source, cancel_progress=25)
    assert record["cancellationTriggered"] and record["exitCode"] != 0 and not output.exists(), log
    assert "Cancelled" in log or "cancelled" in log, log
    assert record["cancellationLatencySeconds"] < 5, record
    results.append(record)

    if not args.skip_performance:
        large = np.empty((4000, 6000, 4), dtype=np.uint8)
        x = np.arange(6000, dtype=np.uint16)[None, :]
        y = np.arange(4000, dtype=np.uint16)[:, None]
        large[:, :, 0] = (x // 12 + y // 12) % 256
        large[:, :, 1] = ((x // 24 + y // 24) % 2) * 220 + 20
        large[:, :, 2] = (y // 9) % 256
        large[:, :, 3] = 255
        large_source = directory / "source-24mp.png"
        Image.fromarray(large).save(large_source)
        del large
        record, output, log = execute(worker, directory, "blur-24mp-radius64-both", large_source,
                                      {"output": "blur", "radius": 64, "side": "both", "focus": 0.5, "range": 0.05}, timeout=180)
        assert record["exitCode"] == 0 and output.exists(), log
        with Image.open(output) as image:
            assert image.size == (6000, 4000) and image.mode == "RGBA"
            assert image.getextrema()[3] == (255, 255)
        record.update(pixels=24_000_000, radius=64, areas="foreground-and-background")
        results.append(record)

        over_limit = directory / "source-36mp.png"
        Image.new("RGBA", (6000, 6000), (127, 80, 200, 255)).save(over_limit)
        record, output, log = execute(worker, directory, "reject-36mp-header", over_limit, size=(518, 518))
        assert record["exitCode"] != 0 and not output.exists() and "24 megapixels" in log, log
        record["expectedRejection"] = True
        results.append(record)
        record, output, log = execute(worker, directory, "reject-36mp-request", source, size=(6000, 6000))
        assert record["exitCode"] != 0 and not output.exists() and "24 megapixels" in log, log
        record["expectedRejection"] = True
        results.append(record)

    report = {
        "status": "passed", "packageSha256": hashlib.sha256(args.package.read_bytes()).hexdigest(),
        "packageBytes": args.package.stat().st_size,
        "executablePathCharacters": len(str(worker)), "modelPathCharacters": len(str(model)),
        "minimalChildPath": "Windows System32 and Windows directory only; parent environment unchanged",
        "nativeDependencies": "Only the DLLs extracted from this package plus Windows system DLLs",
        "testDirectory": str(directory.relative_to(ROOT)), "tests": results,
        "limitations": "Synthetic numerical/processing checks. These do not certify photographic model accuracy, blur boundary quality, or performance on other hardware.",
    }
    serialized = json.dumps(report, indent=2) + "\n"
    (directory / "report.json").write_text(serialized, encoding="utf-8")
    report_name = "worker-smoke-sampled.json" if args.sampled_golden else "worker-smoke.json"
    (ROOT / "verification" / report_name).write_text(serialized, encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
