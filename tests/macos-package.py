#!/usr/bin/env python3
"""Smoke-test a packaged Mac launcher without opening the browser or touching papers."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import signal
import socket
import subprocess
import tempfile
import time
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', type=Path, help='extracted GitLeaf.app')
    args = parser.parse_args()
    app = args.app.resolve(strict=True)
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    assert info['CFBundleExecutable'] == 'GitLeaf'
    assert info['CFBundlePackageType'] == 'APPL'
    assert (app / 'Contents/Resources/GitLeaf.icns').stat().st_size > 0
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
    binary = app / 'Contents/MacOS/gitleaf-server'
    assert subprocess.check_output([str(binary), '--version'], text=True, timeout=10).strip() == 'gitleaf ' + info['CFBundleShortVersionString']
    with tempfile.TemporaryDirectory(prefix='gitleaf-mac-smoke-') as directory:
        with socket.socket() as reserve:
            reserve.bind(('127.0.0.1', 0))
            port = reserve.getsockname()[1]
        url = f'http://127.0.0.1:{port}'
        env = dict(os.environ, GITLEAF_NO_OPEN='1', GITLEAF_NO_UPDATE_CHECK='1', PATH='/usr/bin:/bin')
        argv = [str(app / 'Contents/MacOS/GitLeaf'), '--root', directory + '/projects',
                '--cache', directory + '/cache', '--port', str(port), '--no-open']
        launcher = subprocess.Popen(argv, env=env)
        try:
            deadline = time.monotonic() + 20
            health = None
            while time.monotonic() < deadline:
                assert launcher.poll() is None, 'launcher exited before startup'
                try:
                    with urllib.request.urlopen(url + '/api/health', timeout=1) as response:
                        health = json.load(response)
                    break
                except OSError:
                    time.sleep(0.1)
            assert health and health['version'] == info['CFBundleShortVersionString'], health
            with urllib.request.urlopen(url + '/api/update/check', timeout=2) as response:
                update = json.load(response)
            assert update['can_self_update'] is False, update
            assert 'replace GitLeaf.app' in update['manual_update_instructions'], update
            with urllib.request.urlopen(url + '/assets/app.js', timeout=2) as response:
                assert response.status == 200 and len(response.read()) > 10000
            # Launching a second matching copy reuses the server, rather than taking it over.
            duplicate = subprocess.run([str(binary), *argv[1:]], env=env, timeout=15, capture_output=True, text=True)
            assert duplicate.returncode == 0
            assert launcher.poll() is None
            launcher.send_signal(signal.SIGTERM)
            assert launcher.wait(timeout=15) == 0
            with socket.socket() as check:
                assert check.connect_ex(('127.0.0.1', port)) != 0, 'backend remained alive after Quit'
        finally:
            if launcher.poll() is None:
                launcher.send_signal(signal.SIGTERM)
                try:
                    launcher.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    launcher.kill()
                    launcher.wait()
    print('PASS: bundle signature/version/icon, native startup, offline assets, safe update instructions, existing-instance reuse and graceful Quit.')


if __name__ == '__main__':
    main()
