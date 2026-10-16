"""Build and package the independent Windows x64 Depth Studio plugin."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent
VC_NAMES = {"msvcp140.dll", "msvcp140_1.dll", "vcruntime140.dll", "vcruntime140_1.dll"}

def run(*args):
    environment = os.environ.copy()
    temporary = ROOT / "target/test-temp"
    temporary.mkdir(parents=True, exist_ok=True)
    environment["TEMP"] = str(temporary)
    environment["TMP"] = str(temporary)
    subprocess.run([str(arg) for arg in args], cwd=ROOT, check=True, env=environment)

def verify_sdk():
    vendor = ROOT / "vendor"
    manifest = json.loads((vendor / "SDK-RELEASE.json").read_text(encoding="utf-8"))
    for item in manifest["files"]:
        path = (vendor / item["path"]).resolve()
        if not path.is_relative_to(vendor.resolve()):
            raise RuntimeError("SDK release path escapes the private source directory")
        data = path.read_bytes()
        if len(data) != item["bytes"] or hashlib.sha256(data).hexdigest() != item["sha256"]:
            raise RuntimeError(f"Private SDK source checksum mismatch: {item['path']}")

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--package-only", action="store_true", help="Package already-built release binaries after integrity checks")
    args = parser.parse_args()
    if os.name != "nt" or sys.maxsize <= 2**32:
        raise RuntimeError("Build this release on 64-bit Windows.")
    run(sys.executable, ROOT / "scripts/fetch-assets.py", "--offline")
    verify_sdk()
    flags = ["--locked"] + (["--offline"] if args.offline else [])
    for manifest, package, target in [("Cargo.toml", "photocraft-depth-studio", "target"), ("native/Cargo.toml", "photocraft-depth-studio-native", "native/target")]:
        if args.check:
            run("cargo", "fmt", "--manifest-path", manifest, "--package", package, "--", "--check")
            run("cargo", "test", "--manifest-path", manifest, "--target-dir", target, *flags)
            run("cargo", "clippy", "--manifest-path", manifest, "--target-dir", target, *flags, "--all-targets", "--", "-D", "warnings")
        if not args.package_only:
            build = ["cargo", "build", "--manifest-path", manifest, "--target-dir", target, *flags, "--release"]
            if manifest == "Cargo.toml":
                build += ["--target", "wasm32-unknown-unknown"]
            run(*build)
    verify_sdk()
    run(sys.executable, ROOT / "scripts/collect-licenses.py")
    authoring = json.loads((ROOT / "authoring.json").read_text(encoding="utf-8"))
    worker = ROOT / "native/target/release/depth-studio-worker.exe"
    with worker.open("rb") as stream:
        header = stream.read(64)
        if header[:2] != b"MZ" or len(header) != 64:
            raise RuntimeError("Worker is not a Windows executable")
        stream.seek(struct.unpack_from("<I", header, 60)[0])
        if stream.read(6) != b"PE\x00\x00\x64\x86":
            raise RuntimeError("Worker must target Windows x86_64")
    output = ROOT / "dist/PhotoCraft-Depth-Studio.pcplugin"
    output.parent.mkdir(exist_ok=True)
    assets = json.loads((ROOT / "runtime/downloads.json").read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory(prefix="stage-", dir=output.parent) as directory:
        stage = Path(directory)
        (stage / "package.json").write_text(json.dumps(authoring["package"], indent=2) + "\n", encoding="utf-8")
        shutil.copy2(ROOT / "target/wasm32-unknown-unknown/release/photocraft_depth_studio.wasm", stage / "plugin.pcext")
        shutil.copy2(worker, stage / worker.name)
        # Only allowlisted, verified payloads enter the archive; no caches or test environments.
        for asset in assets["files"]:
            relative = Path(asset["path"])
            source = ROOT / "runtime" / relative
            destination = stage / (relative.name if relative.name in VC_NAMES else Path("runtime") / relative)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
        for name in ("downloads.json", "ASSET-PROVENANCE.md", "model-contract.json"):
            source = ROOT / "runtime" / name
            if source.is_file():
                shutil.copy2(source, stage / "runtime" / name)
        shutil.copytree(ROOT / "licenses", stage / "licenses")
        for name in ("SDK-RELEASE.json", "LICENSE-MIT", "LICENSE-APACHE"):
            source = ROOT / "vendor" / name
            if source.is_file():
                shutil.copy2(source, stage / "licenses" / ("PhotoCraft-SDK-" + name))
        for name in ("LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "THIRD-PARTY-NOTICES.md", "README.md", "CHANGELOG.md"):
            shutil.copy2(ROOT / name, stage / name)
        temporary = output.with_suffix(".partial")
        with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
            for path in sorted(stage.rglob("*")):
                if path.is_file():
                    info = zipfile.ZipInfo(path.relative_to(stage).as_posix(), date_time=(2026, 1, 1, 0, 0, 0))
                    info.compress_type = zipfile.ZIP_DEFLATED
                    with archive.open(info, "w") as destination, path.open("rb") as source:
                        shutil.copyfileobj(source, destination, 1024 * 1024)
        with zipfile.ZipFile(temporary) as archive:
            bad = archive.testzip()
            if bad:
                raise RuntimeError(f"Archive CRC failure: {bad}")
        temporary.replace(output)
    with output.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    (output.parent / "SHA256SUMS.txt").write_text(f"{digest}  {output.name}\n", encoding="utf-8")
    print(f"Installable package: {output}\nSHA256: {digest}")

if __name__ == "__main__":
    main()
