# Depth Map & Depth Blur plugin

Read `README.md`, `authoring.json`, `src/lib.rs`, and `build.py` before editing.
This independent project owns the panel, native worker, models, runtime dependencies,
tests, and packaging. Keep all local dependencies inside this project. Do not add
editor, sibling plugin, companion, or global inference-runtime dependencies.

## Contracts and ownership

Preserve `org.photocraft.depth-studio` across releases. Edit `authoring.json` as the
single source of descriptor and package metadata; Cargo writes generated manifests
and bindings to `OUT_DIR`. The typed Plugin API uses internal wire discriminator 1
in the current host. Package format 2 and native worker protocol 1 are independent.
Keep the private
API/SDK snapshot, its licenses, locks, and `vendor/SDK-RELEASE.json` hashes intact.

The WASM SDK owns bounded import-free ABI buffers. Do not add unsafe pointer access,
WASI, networking, or filesystem imports. Preserve request state and return no effects
for observers and unknown events. Only `generate` starts work, gated on an open
document and validated settings. It returns exactly one `plugin.processImage` command
with `processor: "native"` and `preview: true`; no installation ID, process path,
launch arguments, or output dimensions belong in the panel request.

Worker options are `output` (`blur`/`map`), `focus` (0–1), `range` (0–1), `radius`
(integer 0–64), `side` (`background`/`both`), and `invert` (boolean). Relative depth
uses near = 1 and far = 0; inversion changes map output only. Update panel validation,
worker validation, controls, and tests together when changing options. Model outputs
are relative estimates, not metric depth or precise object masks.

Native input is the visible flattened 8-bit RGBA sRGB composite. The host does not
supply active-layer pixels or a separate selection mask. Preserve dimensions and alpha;
do not claim high-precision, editable depth-channel, or selection-aware native transport.
Native workers have normal OS access, unlike the bounded WASM panel. Validate protocol,
options, sizes, assets, and cancellation. Read cancellation/EOF concurrently, stop and
reap owned descendants, and never publish a cancelled result. Do not share another
plugin's process, model directory, or mutable runtime.

## Build and acceptance

Follow `README.md` and `build.py` for checks and packaging. Run panel/native tests,
formatting, Clippy, actual release WASM/native builds, model integrity checks, and
package checks. Keep both Cargo projects and vendored SDK locks reproducible. Do not
format private SDK sources independently of their hash index. For the panel, use
`cargo fmt --package photocraft-depth-studio -- --check`; `cargo fmt --all` also
traverses vendored path dependencies and changes their pinned bytes. Retain source, runtime,
and model license notices; verify checkpoint provenance before changing models.

Exercise the exact artifact in a compatible host with disposable documents and an
isolated store: map and blur pixels, alpha, previews, Apply/Discard, Undo/Redo,
stale-source refusal, cancellation, and removal. Restricted automation refusal must
not be bypassed. Report which host checks were actually performed. Keep generated
`target/`, `dist/`, downloaded runtime/model payloads, machine logs, and screenshots
out of source commits. Update `CHANGELOG.md` and source instructions when behavior
changes. Building does not itself authorize publishing or replacing installations.
