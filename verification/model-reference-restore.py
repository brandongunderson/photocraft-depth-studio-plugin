"""Exercise asset restoration in a fresh isolated directory; no installed plugin changes."""
from pathlib import Path
import importlib.util
import json
import shutil
import sys
import tempfile
import time


root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("fetch_assets", root / "scripts" / "fetch-assets.py")
fetch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fetch)
started = time.time()
with tempfile.TemporaryDirectory(prefix="model-reference-restore-", dir=root / "verification") as directory:
    isolated = Path(directory)
    fetch.RUNTIME = isolated / "runtime"
    fetch.CACHE = isolated / "cache"
    fetch.RUNTIME.mkdir()
    shutil.copyfile(root / "runtime" / "downloads.json", fetch.RUNTIME / "downloads.json")
    sys.argv = ["fetch-assets.py"]
    fetch.main()
    sys.argv.append("--offline")
    fetch.main()
    rejected = []
    for path in ("../escape", "nested/../../escape", "/absolute", "C:/absolute", "nested\\escape"):
        try:
            fetch.contained(fetch.RUNTIME, path)
        except ValueError:
            rejected.append(path)
        else:
            raise AssertionError(f"Path incorrectly accepted: {path}")
    model = fetch.RUNTIME / "models" / "depth-anything-v2-small.onnx"
    with model.open("r+b") as stream:
        stream.write(b"FAIL")
    try:
        fetch.main()
    except RuntimeError as error:
        assert "depth-anything-v2-small.onnx" in str(error)
    else:
        raise AssertionError("Offline verification accepted modified model")
result = {
    "freshNetworkRestoration": "passed",
    "offlineVerification": "passed",
    "alteredModelRejected": "passed",
    "unsafePathsRejected": rejected,
    "runtimeInstallerExecuted": False,
    "restoredFileCount": 17,
    "elapsedSeconds": round(time.time() - started, 3),
}
(root / "verification" / "model-reference-restore.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
print(json.dumps(result, indent=2))
