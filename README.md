# Depth Map & Depth Blur for PhotoCraft

An independent local PhotoCraft plugin using **Depth Anything V2 Small**. Create a
grayscale depth map or soften the background and optional foreground around a chosen
focus depth. The package includes the model, Windows x64 processing engine, and native
panel. It needs no Python installation, companion application, network connection,
or GPU at runtime. CPU inference uses up to four threads.

[Download the latest release](https://github.com/brandongunderson/photocraft-depth-studio-plugin/releases/latest)
or [browse the source](https://github.com/brandongunderson/photocraft-depth-studio-plugin).

## Install and use

1. Use the current PhotoCraft build with the typed Plugin API, `host.commands.invoke`
   and `plugin.processImage`, native package installation, and comparison previews.
2. Choose **Plugins > Manage Plugins > Browse** and select
   `PhotoCraft-Depth-Studio.pcplugin`. Review the bundled native-code permission.
3. Open an image and choose **Plugins > Depth Map & Depth Blur**.
4. Choose **Depth map**, then **Create result** to inspect the inferred depth.
   White is near and black is far; the invert control reverses the map only.
5. Choose **Depth blur**. Set **Focus depth** to the gray level of the subject you
   want sharp: 0 is far, 1 is near. **Sharp depth range** is the full width of the
   in-focus interval around that value. **Maximum blur radius** controls softness
   in original-image pixels. Choose background-only or foreground-and-background blur.
6. Compare the preview, then **Apply** to a new tab or same-size new layer, or **Discard**.
   Previewing leaves the source untouched. New-layer Apply hides the previous stack
   in the same undo step so the composite matches the preview; Undo restores it.

Start with focus 0.65, sharp range 0.1, radius 16, and background-only blur.
All controls remain visible with this API; focus/range/radius/areas affect blur only,
and inversion affects depth-map output only. Radius zero returns the original pixels
without loading the model. Focus is relative scene depth, not meters and not an
object selection. There is currently no click-to-focus canvas API.

## Plugin updates

Install version 1.0.1 or later once to register this plugin's GitHub update source.
Earlier local packages do not contain that information. PhotoCraft can then check
the public releases of this repository for `PhotoCraft-Depth-Studio.pcplugin`.
Use **Check for updates** in Manage Plugins whenever you want to check manually.
Hosts with automatic plugin update checks can also show available updates without
a manual check. Updates still require your review and installation; checking does
not replace the installed plugin. You can always install a downloaded package
locally, and image processing continues to work offline.

## Image behavior and limits

- Input/output keep the same dimensions and source alpha, up to **24 megapixels**
  and 32,768 pixels per side. Larger images require resizing a copy.
- The host supplies the visible flattened **8-bit RGBA sRGB** composite. Supported
  16-bit source documents are converted by the host; inference does not preserve
  their original precision. Active-layer pixels and selection masks are not supplied.
- The map is an 8-bit grayscale raster with original transparency, not a metric
  depth measurement, editable depth channel, or perfect segmentation mask.
- Inference preserves aspect ratio approximately, rounds dimensions to multiples
  of 14, targets a 518-pixel short edge, and caps the long edge at 1,036 pixels.
  Depth is resized in float and normalized to visible pixels before rendering.
  This bounded resize policy differs from upstream's unbounded short-edge policy.
- Transparent source pixels are composited over neutral gray only for model input.
  Output alpha is unchanged. Blur uses normalized separate near/far support to limit
  color bleeding across the focus boundary. It is a depth-guided soft blur, not a
  physical lens/bokeh simulation or reconstruction of hidden background content.
- Thin structures, hair, glass, mirrors, and ambiguous scenes can produce inaccurate
  depth boundaries. Inspect the map and preview before applying. Large processing
  jobs can use substantial RAM; CPU timing depends on image size and hardware.
- Cancellation/host disconnect stops processing; incomplete output is discarded.
  Native processing runs with normal user privileges; the WASM panel is sandboxed.

## Host compatibility

This plugin uses the **PhotoCraft Plugin API** through its privately included typed
SDK. It requires native package installation, `host.commands.invoke`,
`plugin.processImage`, and comparison previews. For developers, the package format
is 2 and the native worker uses protocol 1; these are separate transport formats.

This is an independent project: metadata, controls, algorithms, model weights and
runtime dependencies live here. No PhotoCraft core changes are required.

## Build

Build on Windows x64 with Rust 1.95+, a compatible native linker, the
`wasm32-unknown-unknown` target, and Python 3.11+ for packaging. The model and runtime
are downloaded build assets, excluded from source control.

```powershell
rustup target add wasm32-unknown-unknown
python scripts/fetch-assets.py
python build.py --check
```

Downloads use pinned revisions, sizes and SHA-256 hashes in `runtime/downloads.json`.
The fetcher extracts runtime files without executing downloaded installers. Once
assets and Cargo dependencies are cached, use `python build.py --check --offline`.
`build.ps1 -Check -Offline -Python C:\path\python.exe` is the PowerShell wrapper.

The output is `dist/PhotoCraft-Depth-Studio.pcplugin`, with `dist/SHA256SUMS.txt`.
The root panel and `native/` are independent Cargo workspaces with private lockfiles.
`authoring.json` is the single metadata source; generated bindings stay in Cargo
`OUT_DIR`. Keep the privately included SDK's byte-pinned source files intact.

See `AGENTS.md` for development rules, `THIRD-PARTY-NOTICES.md` for provenance and
licenses, and `verification/README.md` for the checks actually performed.
