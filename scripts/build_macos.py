#!/usr/bin/env python3
"""Build the local macOS app; never downloads, launches, or accepts installers."""
import utils.transport
import utils.session
import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def run(*args, **kwargs):
    subprocess.run([str(arg) for arg in args], check=True, **kwargs)


def copy_file(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.is_symlink():
        destination.unlink()
    shutil.copy2(source, destination, follow_symlinks=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cargo', default=os.environ.get('CARGO_BIN', 'cargo'))
    parser.add_argument('--runtime-root', type=Path)
    parser.add_argument('--state-dir', type=Path)
    args = parser.parse_args()
    local_file = ROOT / '.local.json'
    config = json.loads(local_file.read_text()) if local_file.exists() else {}
    runtime_root = args.runtime_root or (Path(config['runtime_root']) if config.get('runtime_root') else None)
    state_dir = args.state_dir or (Path(config['state_dir']) if config.get('state_dir') else None)
    if runtime_root:
        runtime_root = runtime_root.expanduser().resolve()
        if not (runtime_root / 'WineSikarugir10.bundle/bin/wine').is_file():
            parser.error('runtime-root must contain the prepared WineSikarugir10.bundle/bin/wine')
    run(args.cargo, 'build', '--locked', '--release', '--manifest-path', ROOT / 'Cargo.toml')
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')).resolve()
    app = ROOT / 'dist/PB Runner.app'
    if app.exists():
        # Only replaces this script's fixed generated output, never a prefix/state directory.
        shutil.rmtree(app)
    macos = app / 'Contents/MacOS'
    resources = app / 'Contents/Resources'
    macos.mkdir(parents=True)
    resources.mkdir(parents=True)
    copy_file(target / 'release/pb-runner', resources / 'pb-runner')
    for name in ('D3D9Probe.cs', 'D3D11Probe.cs'):
        copy_file(ROOT / 'diagnostics' / name, resources / name)
    run('xcrun', 'clang', '-arch', 'x86_64', ROOT / 'diagnostics/runtime-probe.c', '-o', resources / 'runtime-probe')
    run('xcrun', 'swiftc', '-parse-as-library', '-swift-version', '5', '-O',
        '-target', 'arm64-apple-macos13.0', '-module-cache-path', ROOT / 'build/swift-module-cache',
        '-framework', 'SwiftUI', '-framework', 'AppKit', '-framework', 'Metal',
        ROOT / 'macos/PBRunnerApp.swift', '-o', macos / 'PBRunnerUI')
    info = {
        'CFBundleDevelopmentRegion': 'en', 'CFBundleExecutable': 'PBRunnerUI',
        'CFBundleIdentifier': 'dev.pbrunner.prototype', 'CFBundleName': 'PB Runner',
        'CFBundleDisplayName': 'PB Runner', 'CFBundlePackageType': 'APPL',
        'CFBundleShortVersionString': '0.1.0', 'CFBundleVersion': '1',
        'LSMinimumSystemVersion': '13.0', 'NSHighResolutionCapable': True,
        'PBWineRelativePath': 'runtime/WineSikarugir10.bundle/bin/wine',
        'PBWineLabel': 'Wine Sikarugir 10.0 revision 8 · eksperimen',
        'PBMonoProbeDirectory': 'sikarugir10-mono-probe-state',
    }
    if state_dir:
        info['PBStateDirectory'] = str(state_dir.expanduser().resolve())
    (app / 'Contents/Info.plist').write_bytes(plistlib.dumps(info))
    if runtime_root:
        destination = resources / 'runtime'
        destination.mkdir()
        shutil.copytree(runtime_root / 'WineSikarugir10.bundle', destination / 'WineSikarugir10.bundle', symlinks=True)
        for source in runtime_root.glob('*.dylib'):
            copy_file(source, destination / source.name)
        licenses = runtime_root.parent / 'licenses'
        if licenses.is_dir():
            shutil.copytree(licenses, resources / 'licenses', symlinks=True)
    for key, folder in (('installer_file', 'installer'), ('tools_dir', 'tools')):
        if not config.get(key):
            continue
        source = Path(config[key]).expanduser().resolve()
        if key == 'installer_file':
            copy_file(source, resources / folder / 'Install_POINTBLANK_20251111.exe')
        else:
            shutil.copytree(source, resources / folder, symlinks=True)
    run('codesign', '--force', '--sign', '-', app)
    run('codesign', '--verify', app)
    print('Built:', app)
    print('Existing PB/Wine sessions were not launched or stopped.')
    if not runtime_root:
        print('No runtime bundled. Add a prepared runtime-root before running Wine.')


if __name__ == '__main__':
    main()
