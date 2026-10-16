"""Independent CPU ONNX reference on an original synthetic RGB fixture.

Validation only: numpy, Pillow, onnx==1.18.0, onnxruntime==1.22.0.
The 518px square avoids resampling so tensor/channel/normalization mistakes can be
distinguished from intentionally different interpolation kernels in the worker.
No Python libraries or generated reference data are required by the plugin.
"""
from pathlib import Path
import hashlib
import json
import sys
import time

import numpy as np
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / "verification"
sys.path.insert(0, str(DIRECTORY / "model-reference-environment"))
import onnx
import onnxruntime as ort


def tensor_info(value):
    tensor = value.type.tensor_type
    return {
        "name": value.name,
        "elementType": tensor.elem_type,
        "shape": [d.dim_value if d.HasField("dim_value") else d.dim_param for d in tensor.shape.dim],
    }


def fixture():
    y, x = np.mgrid[0:518, 0:518]
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


model_path = ROOT / "runtime" / "models" / "depth-anything-v2-small.onnx"
model = onnx.load(model_path)
onnx.checker.check_model(model)
options = ort.SessionOptions()
options.intra_op_num_threads = 2
session = ort.InferenceSession(str(model_path), sess_options=options, providers=["CPUExecutionProvider"])
image = fixture()
image.save(DIRECTORY / "model-reference-input.png")
rgb = np.asarray(image, dtype=np.float32) / np.float32(255)
mean = np.array([0.485, 0.456, 0.406], dtype=np.float32)
std = np.array([0.229, 0.224, 0.225], dtype=np.float32)
tensor = np.ascontiguousarray(((rgb - mean) / std).transpose(2, 0, 1)[None, ...])
started = time.perf_counter()
prediction = session.run(["predicted_depth"], {"pixel_values": tensor})[0]
elapsed = time.perf_counter() - started
assert prediction.shape == (1, 518, 518)
assert np.isfinite(prediction).all()
assert prediction.max() - prediction.min() > 0.01
depth = prediction[0]
normalized = (depth - depth.min()) / (depth.max() - depth.min())
visualization = np.floor(np.clip(normalized, 0, 1) * 255 + 0.5).astype(np.uint8)
Image.fromarray(visualization).save(DIRECTORY / "model-reference-depth.png")
np.save(DIRECTORY / "model-reference-depth.npy", depth)
dynamic_tensor = np.ascontiguousarray(tensor[:, :, :224, :336])
dynamic_output = session.run(["predicted_depth"], {"pixel_values": dynamic_tensor})[0]
assert dynamic_output.shape == (1, 224, 336) and np.isfinite(dynamic_output).all()
samples = [(0, 0), (120, 380), (259, 259), (400, 150), (517, 517)]
report = {
    "onnxChecker": "passed",
    "onnxRuntimeVersion": ort.__version__,
    "provider": session.get_providers(),
    "modelSha256": hashlib.sha256(model_path.read_bytes()).hexdigest(),
    "modelProducer": {"name": model.producer_name, "version": model.producer_version},
    "irVersion": model.ir_version,
    "opset": [{"domain": v.domain, "version": v.version} for v in model.opset_import],
    "inputs": [tensor_info(v) for v in model.graph.input],
    "outputs": [tensor_info(v) for v in model.graph.output],
    "fixture": "Original procedural scene, 518x518 opaque RGB, no resize",
    "inputSha256": hashlib.sha256((DIRECTORY / "model-reference-input.png").read_bytes()).hexdigest(),
    "preprocessing": "RGB uint8 -> float32 /255 -> subtract [0.485,0.456,0.406] -> divide [0.229,0.224,0.225] -> NCHW",
    "outputShape": list(prediction.shape),
    "finite": True,
    "min": float(depth.min()),
    "max": float(depth.max()),
    "mean": float(depth.mean()),
    "inferenceSeconds": round(elapsed, 4),
    "samples": [{"y": y, "x": x, "rgb": image.getpixel((x, y)), "input": tensor[0, :, y, x].tolist(), "depth": float(depth[y, x]), "normalized8bit": int(visualization[y, x])} for y, x in samples],
    "dynamicShapeProbe": {"input": [1, 3, 224, 336], "output": list(dynamic_output.shape), "finite": True},
    "meaning": "Relative inverse depth: larger values indicate nearer regions. Not meters or calibrated distances.",
    "limitations": "Checks numerical execution and contract, not depth accuracy or portrait boundary quality.",
}
(DIRECTORY / "model-reference.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print(json.dumps(report, indent=2))
