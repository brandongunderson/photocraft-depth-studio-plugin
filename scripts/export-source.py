"""Export a fresh standalone source tree or ZIP without generated/binary assets."""
from pathlib import Path
import argparse
import os
import shutil
import zipfile

ROOT = Path(__file__).resolve().parents[1]
EXCLUDED = {"target", "dist", ".git", "__pycache__", "model-reference-environment", "stores", "extracted", "source-export"}

def files():
    for folder, directories, names in os.walk(ROOT):
        relative = Path(folder).relative_to(ROOT)
        directories[:] = [name for name in directories if name not in EXCLUDED
                          and not name.startswith("worker-smoke-files-")
                          and not (relative == Path("runtime") and name in {"bin", "models"})]
        for name in sorted(names):
            path = Path(folder) / name
            if relative.parts and relative.parts[0] == "verification" and path.suffix not in {".py", ".md"}:
                continue
            if path.suffix.lower() in {".pcplugin", ".exe", ".dll", ".onnx", ".wasm", ".pyc", ".partial"}:
                continue
            if not path.is_symlink():
                yield path, path.relative_to(ROOT)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    destination = args.destination.resolve()
    if destination.exists():
        raise RuntimeError("Choose a fresh export destination")
    source = list(files())
    if destination.suffix.lower() == ".zip":
        destination.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(destination, "x", compression=zipfile.ZIP_DEFLATED) as archive:
            for path, relative in source:
                archive.write(path, "photocraft-depth-studio-plugin/" + relative.as_posix())
    else:
        destination.mkdir(parents=True)
        for path, relative in source:
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, target)
    print(f"Exported {len(source)} source files to {destination}")

if __name__ == "__main__":
    main()
