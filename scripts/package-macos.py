#!/usr/bin/env python3
"""Build a Finder-launchable GitLeaf.app, ZIP and drag-to-Applications DMG on macOS."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def run(*args, **kwargs):
    print('+', ' '.join(str(arg) for arg in args), flush=True)
    return subprocess.run([str(arg) for arg in args], check=True, **kwargs)


def sign(path, identity):
    args = ['codesign', '--force', '--sign', identity]
    if identity != '-':
        args += ['--timestamp', '--options', 'runtime']
    run(*args, path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/gitleaf')
    parser.add_argument('--arch', choices=['arm64', 'x86_64'], required=True)
    parser.add_argument('--output', type=Path, default=ROOT / 'dist')
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('macOS is required (Swift, iconutil, codesign and hdiutil are used)')
    binary = args.binary.resolve(strict=True)
    version = json.loads((ROOT / 'package.json').read_text())['version']
    if subprocess.check_output([str(binary), '--version'], text=True).strip() != f'gitleaf {version}':
        parser.error('binary version must match package.json; rebuild the release binary')
    architectures = subprocess.check_output(['lipo', '-archs', str(binary)], text=True).split()
    if architectures != [args.arch]:
        parser.error(f'binary architectures {architectures} do not match --arch {args.arch}')
    identity = os.environ.get('MACOS_SIGNING_IDENTITY') or '-'
    notary_profile = os.environ.get('MACOS_NOTARY_PROFILE')
    if notary_profile and identity == '-':
        parser.error('notarization requires MACOS_SIGNING_IDENTITY (Developer ID Application)')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    stem = f'gitleaf-macos-{args.arch}'
    with tempfile.TemporaryDirectory(prefix='.gitleaf-package-', dir=output) as scratch:
        scratch = Path(scratch)
        module_cache = Path(os.environ.get('SWIFT_MODULE_CACHE_PATH', str(scratch / 'swift-cache')))
        volume = scratch / 'volume'
        app = volume / 'GitLeaf.app'
        executables = app / 'Contents/MacOS'
        resources = app / 'Contents/Resources'
        executables.mkdir(parents=True)
        resources.mkdir()
        shutil.copy2(binary, executables / 'gitleaf-server')
        (executables / 'gitleaf-server').chmod(0o755)
        run('xcrun', 'swiftc', '-module-cache-path', module_cache, '-swift-version', '5', '-O', '-target',
            f'{args.arch}-apple-macosx13.0', '-framework', 'AppKit',
            ROOT / 'packaging/macos/Launcher.swift', '-o', executables / 'GitLeaf')
        icon_tool = scratch / 'render-icon'
        run('xcrun', 'swiftc', '-module-cache-path', module_cache, '-swift-version', '5', '-O', '-framework', 'AppKit',
            ROOT / 'packaging/macos/Icon.swift', '-o', icon_tool)
        png = scratch / 'logo.png'
        run(icon_tool, png)
        iconset = scratch / 'GitLeaf.iconset'
        iconset.mkdir()
        for points in (16, 32, 128, 256, 512):
            for scale in (1, 2):
                suffix = '@2x' if scale == 2 else ''
                run('sips', '-z', points * scale, points * scale, png, '--out',
                    iconset / f'icon_{points}x{points}{suffix}.png', stdout=subprocess.DEVNULL)
        run('iconutil', '-c', 'icns', iconset, '-o', resources / 'GitLeaf.icns')
        shutil.copy2(ROOT / 'LICENSE', resources / 'LICENSE.txt')
        shutil.copy2(ROOT / 'static/assets/THIRD_PARTY_NOTICES.txt', resources / 'THIRD_PARTY_NOTICES.txt')
        info = {
            'CFBundleName': 'GitLeaf', 'CFBundleDisplayName': 'GitLeaf',
            'CFBundleIdentifier': 'com.kmarkiv.gitleaf', 'CFBundleExecutable': 'GitLeaf',
            'CFBundlePackageType': 'APPL', 'CFBundleIconFile': 'GitLeaf.icns',
            'CFBundleShortVersionString': version, 'CFBundleVersion': version,
            'LSMinimumSystemVersion': '13.0', 'NSHighResolutionCapable': True,
            'LSApplicationCategoryType': 'public.app-category.productivity',
            'NSHumanReadableCopyright': 'Copyright © 2026 Vikram Cannanure. MIT License.',
        }
        with (app / 'Contents/Info.plist').open('wb') as stream:
            plistlib.dump(info, stream)
        (app / 'Contents/PkgInfo').write_bytes(b'APPL????')
        # Sign inner executables before sealing the bundle. '-' provides an ad-hoc local signature.
        if subprocess.check_output([str(executables / 'gitleaf-server'), '--version'], text=True, timeout=10).strip() != f'gitleaf {version}':
            raise RuntimeError('packaged server is not the expected GitLeaf binary')
        sign(executables / 'gitleaf-server', identity)
        sign(executables / 'GitLeaf', identity)
        sign(app, identity)
        run('codesign', '--verify', '--deep', '--strict', '--verbose=2', app)
        zipped = scratch / f'{stem}.app.zip'
        run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', app, zipped)
        if notary_profile:
            run('xcrun', 'notarytool', 'submit', zipped, '--keychain-profile', notary_profile, '--wait')
            run('xcrun', 'stapler', 'staple', app)
            run('xcrun', 'stapler', 'validate', app)
            run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', app, zipped)
        (volume / 'Applications').symlink_to('/Applications')
        (volume / 'Read Me.txt').write_text(
            f'GitLeaf {version}\n\n'
            '1. Drag GitLeaf.app to Applications.\n'
            '2. Eject this disk image.\n'
            '3. Open GitLeaf from Applications. The editor opens in your browser.\n'
            'Use the leaf menu in the menu bar to reopen the editor or quit GitLeaf.\n\n'
            'Git is required; set your Git author name and email before creating projects.\n'
            'LaTeX is optional: Compile offers to download Tectonic when no engine is installed.\n'
            'MacTeX is also supported.\n'
            'Setup guide: https://github.com/kmarkiv/gitleaf#get-started\n\n'
            'Projects stay in your Documents folder when you replace the app to update it.\n'
            'This app uses the browser editor; closing the browser does not quit the app.\n'
            + ('\nThis build is not notarized. macOS may require approval in Privacy & Security.\n'
               if not notary_profile else '\nThis build is signed and notarized.\n')
        )
        dmg = scratch / f'{stem}.dmg'
        run('hdiutil', 'create', '-volname', 'GitLeaf', '-srcfolder', volume,
            '-fs', 'HFS+', '-format', 'UDZO', '-ov', dmg)
        if identity != '-':
            sign(dmg, identity)
        if notary_profile:
            run('xcrun', 'notarytool', 'submit', dmg, '--keychain-profile', notary_profile, '--wait')
            run('xcrun', 'stapler', 'staple', dmg)
            run('xcrun', 'stapler', 'validate', dmg)
        run('hdiutil', 'verify', dmg)
        os.replace(zipped, output / zipped.name)
        os.replace(dmg, output / dmg.name)
    print(f'Created {output / (stem + ".dmg")} and {output / (stem + ".app.zip")}')
    if not notary_profile:
        print('Not notarized: this package still needs first-launch Gatekeeper approval.')


if __name__ == '__main__':
    main()
