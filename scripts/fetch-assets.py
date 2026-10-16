#!/usr/bin/env python3
"""Restore the independently owned, checksum-pinned Depth Studio runtime.

Python 3.11+ standard library only. Never runs a downloaded installer or model code.
The final installed plugin has no Python or network requirement.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tempfile
import urllib.parse
import urllib.request
import zipfile


ROOT = Path(__file__).resolve().parents[1]
RUNTIME = ROOT / "runtime"
CACHE = ROOT / "target" / "download-cache"
MAX_ASSET_BYTES = 512 * 1024 * 1024


def contained(root: Path, relative: str) -> Path:
    if not isinstance(relative, str) or "\\" in relative or ":" in relative:
        raise ValueError("Asset paths must be relative, forward-slash paths")
    parts = PurePosixPath(relative)
    if parts.is_absolute() or not parts.parts or any(x in (".", "..") for x in parts.parts):
        raise ValueError(f"Unsafe asset path: {relative!r}")
    target = root.joinpath(*parts.parts).resolve()
    if not target.is_relative_to(root.resolve()) or target == root.resolve():
        raise ValueError(f"Asset path escapes its directory: {relative!r}")
    return target


def validate_identity(asset: dict) -> None:
    if not re.fullmatch(r"[0-9a-f]{64}", asset.get("sha256", "")):
        raise ValueError("Every asset requires a lowercase SHA-256 digest")
    if type(asset.get("size")) is not int or not 0 < asset["size"] <= MAX_ASSET_BYTES:
        raise ValueError("Asset size is missing or exceeds the bounded download limit")


def matches(path: Path, asset: dict) -> bool:
    if not path.is_file() or path.stat().st_size != asset["size"]:
        return False
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest() == asset["sha256"]


def safe_url(url: str) -> None:
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password:
        raise ValueError("Only unauthenticated HTTPS asset URLs are permitted")


class HttpsRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, msg, headers, newurl):
        safe_url(newurl)
        return super().redirect_request(request, fp, code, msg, headers, newurl)


def download(asset: dict, destination: Path) -> None:
    safe_url(asset["url"])
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(destination.name + ".part")
    digest = hashlib.sha256()
    count = 0
    try:
        opener = urllib.request.build_opener(HttpsRedirects)
        request = urllib.request.Request(asset["url"], headers={"User-Agent": "PhotoCraft-Depth-Studio/1.0"})
        with opener.open(request, timeout=120) as response, temporary.open("wb") as output:
            safe_url(response.url)
            while chunk := response.read(1024 * 1024):
                count += len(chunk)
                if count > asset["size"]:
                    raise ValueError("Download exceeds its pinned byte length")
                digest.update(chunk)
                output.write(chunk)
        if count != asset["size"] or digest.hexdigest() != asset["sha256"]:
            raise ValueError(f"Checksum or byte-length mismatch: {destination.name}")
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def copy_verified(source: Path, destination: Path, asset: dict) -> None:
    if not matches(source, asset):
        raise ValueError(f"Extracted asset has unexpected bytes: {asset['path']}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(destination.name + ".part")
    try:
        shutil.copyfile(source, temporary)
        if not matches(temporary, asset):
            raise ValueError("Copied asset failed verification")
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def extract_zip(archive: Path, members: list[dict], staging: Path) -> None:
    with zipfile.ZipFile(archive) as source:
        for asset in members:
            member = source.getinfo(asset["member"])
            if member.is_dir() or member.file_size != asset["size"]:
                raise ValueError("Archive member size does not match the pinned identity")
            destination = contained(staging, asset["path"])
            destination.parent.mkdir(parents=True, exist_ok=True)
            with source.open(member) as incoming, destination.open("wb") as output:
                shutil.copyfileobj(incoming, output, 1024 * 1024)


def extract_vc(archive: Path, spec: dict, members: list[dict], staging: Path) -> None:
    if os.name != "nt":
        raise RuntimeError("Restoring this Windows runtime requires Windows system tar.exe")
    tar = Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32" / "tar.exe"
    if not tar.is_file():
        raise RuntimeError("Windows system tar.exe is required for CAB extraction")
    cabinet_root = staging / "cabinets"
    cabinet_root.mkdir()
    data = archive.read_bytes()
    minimum_members = [a["member"] for a in members if a.get("container") != "ux"]
    for container in ("ux", "payload"):
        selected = [a["member"] for a in members if a.get("container") == "ux"] if container == "ux" else ([spec["minimumMember"]] if minimum_members else [])
        if not selected:
            continue
        start, length = spec[container + "Offset"], spec[container + "Size"]
        if type(start) is not int or type(length) is not int or start < 0 or length < 36 or start + length > len(data):
            raise ValueError("Invalid pinned VC runtime cabinet range")
        cabinet = cabinet_root / (container + ".cab")
        cabinet.write_bytes(data[start:start + length])
        if cabinet.read_bytes()[:4] != b"MSCF":
            raise ValueError("Expected a Microsoft cabinet")
        for member in selected:
            if not re.fullmatch(r"[A-Za-z0-9_.-]+", member):
                raise ValueError("Unsafe CAB member name")
        subprocess.run([str(tar), "-xf", str(cabinet), "-C", str(cabinet_root), *selected], check=True, capture_output=True)
    minimum = cabinet_root / spec["minimumMember"]
    selected = minimum_members
    if any(not re.fullmatch(r"[A-Za-z0-9_.-]+", name) for name in selected):
        raise ValueError("Unsafe CAB member name")
    if selected:
        subprocess.run([str(tar), "-xf", str(minimum), "-C", str(cabinet_root), *selected], check=True, capture_output=True)
    for asset in members:
        destination = contained(staging, asset["path"])
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(cabinet_root / asset["member"], destination)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true", help="Verify all required files without network access or modification")
    args = parser.parse_args()
    manifest = json.loads((RUNTIME / "downloads.json").read_text(encoding="utf-8"))
    if manifest.get("formatVersion") != 1:
        raise ValueError("Unsupported runtime asset manifest")
    files = manifest["files"]
    archives = {a["id"]: a for a in manifest["archives"]}
    if len(archives) != len(manifest["archives"]):
        raise ValueError("Duplicate archive ids")
    seen = set()
    missing = []
    for asset in files:
        validate_identity(asset)
        destination = contained(RUNTIME, asset["path"])
        if str(destination).casefold() in seen:
            raise ValueError("Duplicate asset destinations")
        seen.add(str(destination).casefold())
        if not matches(destination, asset):
            missing.append(asset)
    for archive in archives.values():
        validate_identity(archive)
        safe_url(archive["url"])
    if args.offline and missing:
        raise RuntimeError("Missing or altered assets: " + ", ".join(a["path"] for a in missing))
    if missing:
        CACHE.mkdir(parents=True, exist_ok=True)
    restored_archives = set()
    for asset in missing:
        if "archive" not in asset:
            download(asset, contained(RUNTIME, asset["path"]))
            print("Restored " + asset["path"])
            continue
        archive_id = asset["archive"]
        if archive_id in restored_archives:
            continue
        spec = archives[archive_id]
        archive_path = contained(CACHE, spec["sha256"] + ".archive")
        if not matches(archive_path, spec):
            download(spec, archive_path)
        members = [a for a in missing if a.get("archive") == archive_id]
        with tempfile.TemporaryDirectory(prefix="extract-", dir=CACHE) as directory:
            staging = Path(directory)
            if spec["format"] == "zip":
                extract_zip(archive_path, members, staging)
            elif spec["format"] == "vc-redist-cab":
                extract_vc(archive_path, spec, members, staging)
            else:
                raise ValueError("Unsupported archive format")
            for member in members:
                copy_verified(contained(staging, member["path"]), contained(RUNTIME, member["path"]), member)
                print("Restored " + member["path"])
        restored_archives.add(archive_id)
    for asset in files:
        if not matches(contained(RUNTIME, asset["path"]), asset):
            raise ValueError("Final verification failed: " + asset["path"])
    print(f"Verified {len(files)} pinned runtime assets ({sum(a['size'] for a in files):,} bytes).")


if __name__ == "__main__":
    main()
