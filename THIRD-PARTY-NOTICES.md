# Third-party notices

This plugin is independently developed and is not affiliated with the Depth Anything
authors, ONNX Community, Microsoft, Adobe, or the PhotoCraft upstream project.

| Component | Source and license |
|---|---|
| Depth Anything V2 Small | [Official repository](https://github.com/DepthAnything/Depth-Anything-V2) and [Small model](https://huggingface.co/depth-anything/Depth-Anything-V2-Small), Apache-2.0. Larger model variants are not bundled. |
| ONNX conversion of Small | [ONNX Community](https://huggingface.co/onnx-community/depth-anything-v2-small), Apache-2.0 model card. Exact conversion revision, tensor contract and checksum are pinned in `runtime/downloads.json`. |
| Transformers preprocessing reference | [Transformers](https://github.com/huggingface/transformers), Apache-2.0. This plugin implements bounded aspect-preserving preprocessing in Rust, with Catmull-Rom resampling and ImageNet normalization; it does not execute model repository Python code. |
| ONNX Runtime 1.22.0 | [Microsoft ONNX Runtime](https://github.com/microsoft/onnxruntime), MIT and included third-party notices. CPU runtime DLLs are privately bundled. |
| Microsoft Visual C++ runtime | Redistributable runtime components under Microsoft's accompanying terms. Included app-locally; the installer is not run. |
| PhotoCraft public API/SDK 0.1 | MIT OR Apache-2.0; privately included source and `vendor/SDK-RELEASE.json` record exact hashes. |
| Rust image, PNG, serialization and supporting crates | Exact locked dependency licenses collected into `licenses/rust-dependencies.txt`. |

Full model/runtime license texts and model cards are in `runtime/licenses/`; packaging
retains those texts and the private SDK notices. Binary assets are fetched from fixed
sources and verified before packaging. See `runtime/downloads.json` for every asset's
immutable provenance, size and SHA-256. No model or runtime is loaded from another plugin.

The independently written blur algorithm uses separate depth support masks and
normalized separable filtering. It contains no Adobe code, shaders, models or assets.
