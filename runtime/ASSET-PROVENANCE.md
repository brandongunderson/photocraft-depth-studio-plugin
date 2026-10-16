# Depth Studio runtime assets

The package owns its model and native runtime. It does not find or depend on an
editor checkout, another plugin, a system Python environment, or a shared model
installation. Inference is local and uses ONNX Runtime's CPU execution provider.

## Model

Depth Anything V2 Small is the 24.8 million parameter relative-depth model by
Lihe Yang, Bingyi Kang, Zilong Huang, Zhen Zhao, Xiaogang Xu, Jiashi Feng, and
Hengshuang Zhao. It estimates relative inverse depth within an image. Larger
predictions indicate nearer regions; the output does not measure meters or give
calibrated distances that can be compared between unrelated images.

The bundled artifact is the ONNX Community float32 export, **not** an official
author-produced ONNX binary. Its model card identifies the authors' Small model
as its base model and declares Apache-2.0. No model quantization or graph changes
were made in this plugin. The author repository also explicitly licenses Small
under Apache-2.0. Base, Large, and Giant variants are not bundled.

| Source | Immutable revision |
|---|---|
| [ONNX Community export](https://huggingface.co/onnx-community/depth-anything-v2-small/tree/4472b7362082ad9968fee890ca0f1e5aca36b93d) | `4472b7362082ad9968fee890ca0f1e5aca36b93d` |
| [Authors' Small model](https://huggingface.co/depth-anything/Depth-Anything-V2-Small/tree/03876f8651c73a60fe4c2c48294e09fcb6838fcf) | `03876f8651c73a60fe4c2c48294e09fcb6838fcf` |
| [Author source and Apache license](https://github.com/DepthAnything/Depth-Anything-V2/tree/a561b849ebae10a6f5ef49e26c83cbbcd36c71bf) | `a561b849ebae10a6f5ef49e26c83cbbcd36c71bf` |

`models/depth-anything-v2-small.onnx` is 99,060,839 bytes. SHA-256:
`afb6a5c28f3b6bf1618c6e43f02073ef9dfdc70e937502d51603e57b0a1df10c`.
The digest was obtained from Hugging Face's LFS object metadata and verified
against the downloaded bytes. The graph has ONNX IR 9, opset 14, and identifies
its producer as PyTorch 2.3.0. `model-contract.json` records the inspected tensor
contract. The complete upstream cards, README, and Apache license are under
`licenses/`; the upstream processor configuration is retained under `models/`.

The original ONNX export's preprocessing configuration specifies RGB, float
rescaling by 1/255, ImageNet channel means/stds, bicubic resampling, preserved
aspect ratio, dimensions that are multiples of 14, a nominal size of 518, and no
padding. Hugging Face's DPT processor chooses the resize scale closest to one;
the author's original processor uses a lower bound on the short side. The
plugin's bounded resolution/interpolation policy is described in its README.
Neither a different interpolation implementation nor a resolution cap should be
represented as byte-identical to the original author's OpenCV predictions.

For reference, the [DPT processor source](https://github.com/huggingface/transformers/blob/174890280b340b89c5bfa092f6b4fb0e2dc2d7fc/src/transformers/models/dpt/image_processing_dpt.py)
was inspected at Transformers 4.44.2 commit
`174890280b340b89c5bfa092f6b4fb0e2dc2d7fc`. Transformers is not included or required
at runtime. Its Apache license is retained as a reference notice.

## Native dependencies

- [ONNX Runtime 1.22.0 Windows x64](https://github.com/microsoft/onnxruntime/releases/tag/v1.22.0),
  MIT license plus its upstream third-party notices. Both DLLs and both notice
  files are restored from the release ZIP. The archive and every member are
  pinned by SHA-256 and byte length in `downloads.json`.
- Microsoft Visual C++ Runtime 14.44.35211.0, app-local redistributable DLLs. The
  Microsoft license is preserved in `licenses/MICROSOFT-VC-RUNTIME-LICENSE.rtf`.
  The build places these files beside the worker executable as required for DLL
  discovery. The fetcher reads the pinned installer's embedded CAB data using
  Windows' system `tar.exe`; it never executes the installer or changes the
  machine's installed runtime. The manifest pins the installer bytes, CAB ranges,
  DLL members, individual file sizes, and individual hashes.

## Reproducibility and checks

Run `python scripts/fetch-assets.py` with Python 3.11+ to restore missing assets.
`--offline` performs verification only and reports every missing or changed file.
Downloads use HTTPS and bounded expected lengths; every final file must match
its pinned SHA-256 before it is moved into place. ZIP/CAB extraction names only
the required members. No mirror URLs or runtime downloads are used.

A fresh isolated restoration of all 17 files was tested directly against the
public upstream sources, followed by offline verification, rejection of an
altered model, and rejection of escaping paths. This confirms the source project
can restore its own files independently of the development machine's other
plugins. `verification/model-reference-restore.py` is the repeatable check.

`verification/model-reference.py` independently checked the ONNX graph and ran it
with ONNX Runtime 1.22.0 CPU on an original synthetic RGB image. It verified
finite, nonconstant results and dynamic spatial shapes. The square 518px fixture
avoids interpolation differences and supplies a reference for channel ordering
and normalization. These numerical checks do not certify scene-depth accuracy,
fine hair/transparent-edge quality, or photographic blur realism.

Validation-only Python packages and generated reference arrays are not package
dependencies and must not be included in the distributable `.pcplugin`.
