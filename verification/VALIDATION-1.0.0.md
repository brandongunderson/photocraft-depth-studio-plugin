# Depth Studio validation

Validated on Windows x64 on 2026-10-16 against the current local PhotoCraft typed
Plugin API (wire discriminator 1), package format 2, and native worker protocol 1.
This plugin required no PhotoCraft core source changes. The existing editor was
rebuilt for contract and acceptance checks; these results do not qualify an entire
PhotoCraft release.

## Exact installable artifact

- Version: 1.0.0; ID: `org.photocraft.depth-studio`.
- File: `dist/PhotoCraft-Depth-Studio.pcplugin`.
- Size: 97,521,322 bytes.
- SHA-256: `a4915632f24d5265e3057691bbebf07febba2efbf49f21d0f53c620929ab6081`.
- Host identity (BLAKE3, a different hash):
  `f278c272feaf14352c7a4cb5325b93ab77fc0c8e11acb018501c565b7aef4f7f`.

Generated JSON reports, logs, screenshots, installed test copies and downloaded
validation libraries are local evidence, excluded from source exports. This summary
and the portable validation scripts are included in source exports.

## Completed checks

| Check | Result and scope |
|---|---|
| Unit tests | 8 typed-panel tests and 17 native tests passed. Covers strict event/settings validation, bounded processing, alpha, focus behavior, constant depth, invalid sizes, and cancellation. |
| Build quality | Formatting and Clippy with warnings denied passed for both projects. Actual release WASM and Windows native builds passed with locked offline dependencies. |
| SDK ownership | All 35 pinned SDK source files matched their release hashes. The host developer tool accepted private dependency ownership and locked offline resolution for every exported Cargo project. |
| Standalone build | A clean source export built its own WASM, worker and package without editor or sibling plugin sources. Cached Cargo dependencies and restored assets were supplied. |
| Asset restoration | All 17 pinned assets restored from their recorded network sources, then passed offline size/hash verification. Altered model bytes and unsafe manifest paths were rejected. No runtime installer was executed. |
| Model reference | ONNX validation and independent Python ONNX Runtime 1.22 CPU inference passed. Checked tensor contract, finite nonconstant output, and dynamic 224 x 336 input. |
| Package structure | Current `photocraft-plugin check` accepted archive paths, CRC, payload, native header, WASM ABI, manifest and grants. |
| Packaged worker | All 13 checks passed using the exact extracted package, a minimal system PATH, a 368-character executable path and a 388-character model path. |
| Numerical parity | Full-frame 518 x 518 output matched independent ONNX inference within 1 grayscale byte; mean absolute difference 0.0000186 byte. This fixture deliberately avoids resampling. |
| Image processing | Inversion differed by at most 1 byte from the complementary map; nonzero blur changed 97,235 textured RGB pixels while preserving alpha; zero-radius blur preserved every byte. |
| Failure handling | Invalid dimensions, radius, focus and mode, missing model, and 36 MP inputs were rejected without publishing output. Closing stdin during inference cancelled in 66 ms in the measured run. |
| Current engine acceptance | Isolated installation/restart, actual map and nonzero blur, document gating, undeclared-action refusal, 16-bit source retention, alpha within 8-bit quantization, Apply to new layer, Undo/Redo, Discard, inverted map, Apply to new tab with 300 DPI, stale-source refusal, cancellation/reaping and removal all passed. The original source was retained during previews. |
| Actual panel rendering | A freshly rebuilt current host UI rendered the installed panel offscreen. Clicking Create result produced a real blur comparison and enabled Apply. Screenshots were inspected at 1152 x 768 and 640 x 480; Apply and Done remained visible, with scrolling for the small window's controls and preview. |

The exact engine libraries and acceptance fixture are fingerprinted in the local
`host-acceptance-build.json`. Engine acceptance exercises real host code with
disposable documents and an isolated installation store; it does not replace or
install into the user's normal plugin store.
The rendering results and exact UI library fingerprints are recorded locally in
`host-acceptance-ui.json` and `host-acceptance-ui-build.json`. This is an offscreen
test of the real UI, not a manual session in the user's normal desktop installation.

## Measured large-image run

The packaged CPU worker processed a 6,000 x 4,000 synthetic RGBA image (24 MP),
radius 64, blurring foreground and background, in **6.26 seconds**. Recorded peaks
were **671 MiB working set** and **813 MiB private commit**. This is one local run,
including worker startup, model loading and PNG I/O, not a speed guarantee or a
cross-hardware benchmark. It does not include PhotoCraft's own memory use.

## Reproduce portable checks

Build instructions are in the project README. Tests require a developer Python
environment with Pillow and NumPy; the installed plugin does not require Python.

```powershell
python scripts/fetch-assets.py --offline
python build.py --check --offline
python scripts/test-worker.py --sampled-golden
```

The worker test extracts a fresh disposable copy of the package and generates its
own original synthetic fixtures. The portable mode verifies five independently
recorded depth samples. For a full-frame independent numerical reference, install
`onnx==1.18.0` and `onnxruntime==1.22.0` in a developer environment, then run:

```powershell
python verification/model-reference.py
python scripts/test-worker.py
```

`--skip-performance` omits the 24 MP case and its two oversized-input cases.
Reports are saved under `verification/`, with per-run evidence inside
`worker-smoke-files-*`. Use `python scripts/export-source.py FRESH_DESTINATION`
to produce a clean standalone directory or `.zip`; downloaded models, runtimes,
generated artifacts and these test directories are excluded.

## Remaining limitations

The checks establish contract compatibility, numerical execution and processing
behavior. They do not establish photographic model accuracy across portraits,
hair, glass, mirrors, or challenging occlusion boundaries. Broader real-photo
quality evaluation remains useful before presenting this as a polished lens-blur
replacement. There is no GPU path, metric depth, click-to-focus canvas tool,
editable depth channel, selection-mask transport, or high-precision native output.
The current host supplies a flattened 8-bit RGBA sRGB composite, even when the
source document retains higher precision. Only Windows x64 was built and tested.
