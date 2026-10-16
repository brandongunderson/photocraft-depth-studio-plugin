"""Collect exact resolved Cargo dependency license texts for both private projects."""
from pathlib import Path
import json
import os
import subprocess

ROOT = Path(__file__).resolve().parents[1]
packages = {}
for manifest, target in [(ROOT / 'Cargo.toml', 'wasm32-unknown-unknown'), (ROOT / 'native/Cargo.toml', 'x86_64-pc-windows-gnu')]:
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1', '--manifest-path', str(manifest), '--filter-platform', target], text=True, encoding='utf-8'))
    nodes = {node['id']: node for node in metadata['resolve']['nodes']}
    included = set()
    pending = [metadata['resolve']['root']]
    while pending:
        current = pending.pop()
        if current not in included:
            included.add(current)
            pending.extend(nodes[current]['dependencies'])
    for package in metadata['packages']:
        if package['id'] in included:
            packages[package['id']] = package
parts = ['Resolved Rust dependency licenses, including private PhotoCraft API/SDK.\n']
for package in sorted(packages.values(), key=lambda p: (p['name'], p['version'])):
    folder = Path(package['manifest_path']).parent
    files = []
    for directory, subdirs, names in os.walk(folder):
        subdirs[:] = [name for name in subdirs if name not in {'target', 'dist', '.git', 'vendor', 'runtime', 'verification', 'licenses', 'native'}]
        files.extend(Path(directory) / name for name in names if name.lower().startswith(('license', 'licence', 'copying', 'copyright')))
    if not files and folder == ROOT / 'native':
        files = [ROOT / 'LICENSE-MIT']
    if not files:
        raise RuntimeError(f'Missing license text for {package["name"]}')
    parts.append(f'\n{"="*72}\n{package["name"]} {package["version"]}\nLicense: {package.get("license")}\nSource: {package.get("repository")}\n')
    for path in sorted(files):
        parts.append(f'\n--- {path.name} ---\n{path.read_text(encoding="utf-8", errors="replace")}\n')
(ROOT / 'licenses').mkdir(exist_ok=True)
(ROOT / 'licenses/rust-dependencies.txt').write_text('\n'.join(parts), encoding='utf-8')
print(f'Collected license texts for {len(packages)} resolved Rust packages.')
