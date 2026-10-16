# Depth Studio validation

Version 1.0.1 validated on Windows x64 on 2026-10-16 against the current local
PhotoCraft Plugin API, package format 2, and native worker protocol 1.
This plugin required no PhotoCraft core source changes. The existing editor was
rebuilt for contract and acceptance checks; these results do not qualify an entire
PhotoCraft release.

## Exact installable artifact

- Version: 1.0.1; ID: `org.photocraft.depth-studio`.
- File: `dist/PhotoCraft-Depth-Studio.pcplugin`.
- Size: 97,521,843 bytes.
- SHA-256: `1978baf0e1d7c4f2d4c15c93fcc93663f32fc0ec4820fe11c78a3af29afa8848`.
- Host identity (BLAKE3, a different hash):
  `8287e0dda7866926d4af9215434942f8b6ab97a26c407e9f409b3871623c2366`.

The embedded descriptor was read from the compiled package by the conformance
tool. It records repository
`https://github.com/brandongunderson/photocraft-depth-studio-plugin`, GitHub as the
update provider, and the exact asset `PhotoCraft-Depth-Studio.pcplugin`.
The release tag must be `v1.0.1`. Structural metadata validation does not establish
a successful live GitHub update check, download or reviewed installation.

Generated JSON reports, logs, screenshots, installed test copies and downloaded
validation libraries are local evidence, excluded from source exports. This summary
and the portable validation scripts are included in source exports.

## Completed checks

| Check | Result and scope |
|---|---|
| Unit tests | 8 typed-panel tests and 17 native tests passed. Covers strict event/settings validation, bounded processing, alpha, focus behavior, constant depth, invalid sizes, and cancellation. |
| Build quality | Formatting and Clippy with warnings denied passed for both projects. Actual release WASM and Windows native builds passed with locked offline dependencies. |
| SDK ownership | All 35 pinned SDK source files matched their release hashes. The host developer tool accepted private dependency ownership and locked offline resolution for every exported Cargo project. |
| Standalone build | The initial 1.0.0 clean source export built its own WASM, worker and package without editor or sibling plugin sources. Version 1.0.1 changes release metadata and instructions only; dependencies, processing and private SDK sources are unchanged. |
| Asset restoration | All 17 pinned assets restored from their recorded network sources during initial validation, with corruption/path rejection checks. All 17 passed offline size/hash verification again for 1.0.1. No runtime installer was executed. |
| Model reference | Initial ONNX validation and independent Python ONNX Runtime 1.22 CPU inference checked the tensor contract, finite nonconstant output and dynamic 224 x 336 input. The unchanged model's full-frame reference was reused to verify the exact 1.0.1 packaged worker. |
| Package structure | Current `photocraft-plugin check` accepted archive paths, CRC, payload, native header, WASM ABI, manifest and grants. |
| Packaged worker | All 13 checks passed using the exact extracted package, a minimal system PATH, a 368-character executable path and a 388-character model path. |
| Numerical parity | Full-frame 518 x 518 output matched independent ONNX inference within 1 grayscale byte; mean absolute difference 0.0000186 byte. This fixture deliberately avoids resampling. |
| Image processing | Inversion differed by at most 1 byte from the complementary map; nonzero blur changed 97,235 textured RGB pixels while preserving alpha; zero-radius blur preserved every byte. |
| Failure handling | Invalid dimensions, radius, focus and mode, missing model, and 36 MP inputs were rejected without publishing output. Closing stdin during inference cancelled in 47 ms in the measured run. |
| Current engine acceptance | Isolated installation/restart, actual map and nonzero blur, document gating, undeclared-action refusal, 16-bit source retention, alpha within 8-bit quantization, Apply to new layer, Undo/Redo, Discard, inverted map, Apply to new tab with 300 DPI, stale-source refusal, cancellation/reaping and removal all passed. The original source was retained during previews. |
| Actual panel rendering (1.0.0 baseline) | The unchanged panel controls were rendered offscreen by the real host UI during initial validation. Clicking Create result produced a real blur comparison and enabled Apply. Screenshots were inspected at 1152 x 768 and 640 x 480; Apply and Done remained visible, with scrolling at the smaller size. Rendering was not repeated for the metadata-only 1.0.1 release. |

The exact engine libraries and acceptance fixture are fingerprinted in the local
`host-acceptance-build.json`. Engine acceptance exercises real host code with
disposable documents and an isolated installation store; it does not replace or
install into the user's normal plugin store.
The prior rendering results and exact UI library fingerprints are recorded locally in
`host-acceptance-ui.json` and `host-acceptance-ui-build.json`. This is an offscreen
test of the real UI, not a manual session in the user's normal desktop installation.

## Measured large-image run

The packaged CPU worker processed a 6,000 x 4,000 synthetic RGBA image (24 MP),
radius 64, blurring foreground and background, in **5.19 seconds**. Recorded peaks
were **670 MiB working set** and **813 MiB private commit**. This is one local run,
including worker startup, model loading and PNG I/O, not a speed guarantee or a
cross-hardware benchmark. It does not include PhotoCraft's own memory use.

The earlier 1.0.0 measurements and artifact identity are preserved in
`VALIDATION-1.0.0.md`; generated evidence from that run is retained locally under
`target/validation-1.0.0/`.

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
