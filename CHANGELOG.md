# Changelog

## 1.0.1 — 2026-10-16

- First public GitHub release, following the initial local 1.0.0 build.
- Added this plugin's repository and exact release asset to its embedded descriptor
  so PhotoCraft can find available updates through GitHub Releases.
- Added download, source and update instructions. Processing behavior is unchanged.

## 1.0.0 — 2026-10-16 (local build)

- New independent Depth Map & Depth Blur plugin with privately pinned typed SDK.
- Bundled Depth Anything V2 Small ONNX model and CPU runtime for local inference.
- Near-white or inverted grayscale map and adjustable background/foreground depth blur.
- Same-size, alpha-preserving output through host previews, Apply/Discard and history.
- Bounded image/model resolutions, cancellation and atomic output publication.
- Targets the current Plugin API (internal wire 1), package 2 and worker protocol 1.
