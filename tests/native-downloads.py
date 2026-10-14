"""Optional live check of Rust downloads without curl/tar on Unix PATH."""
import io
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.request import Request, urlopen
import zipfile

binary = Path(os.environ.get("GITLEAF_BIN", "target/release/gitleaf")).resolve()
with tempfile.TemporaryDirectory(prefix="gitleaf-native-downloads-") as temporary:
    scratch = Path(temporary)
    tools = scratch / "tools"
    tools.mkdir()
    env = dict(os.environ, GITLEAF_NO_UPDATE_CHECK="1", GITLEAF_NO_OPEN="1",
               GIT_AUTHOR_NAME="GitLeaf Test", GIT_AUTHOR_EMAIL="test@example.invalid",
               GIT_COMMITTER_NAME="GitLeaf Test", GIT_COMMITTER_EMAIL="test@example.invalid")
    if os.name == "posix":
        git = shutil.which("git")
        if not git:
            raise RuntimeError("Git is required for this test")
        (tools / "git").symlink_to(git)
        env["PATH"] = str(tools)
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    log = scratch / "server.log"
    with log.open("wb") as output:
        process = subprocess.Popen([str(binary), "--no-open", "--root", str(scratch / "projects"),
                                    "--cache", str(scratch / "cache"), "--port", str(port)],
                                   env=env, stdout=output, stderr=output)
        base = f"http://127.0.0.1:{port}"
        def api(path, payload=None, timeout=30):
            request = Request(base + path, data=json.dumps(payload).encode() if payload is not None else None,
                              headers={"Content-Type": "application/json"})
            with urlopen(request, timeout=timeout) as response:
                return json.load(response)
        http = None
        try:
            for _ in range(100):
                if process.poll() is not None:
                    raise RuntimeError(log.read_text())
                try:
                    api("/api/health")
                    break
                except OSError:
                    time.sleep(0.05)
            else:
                raise RuntimeError("Server did not start")
            # These are genuine public HTTPS requests, not a mocked downloader.
            release = api("/api/update/check?force=1")
            assert release["error"] is None, release
            print("PASS: native HTTPS update check without curl", flush=True)
            installed = api("/api/engine/install", {}, timeout=300)
            assert installed["version"].lower().startswith("tectonic "), installed
            assert api("/api/health")["tectonic"], "Cached compiler was not detected"
            print("PASS: native Tectonic download, checksum, extraction and version check without curl/tar", flush=True)
            archive = io.BytesIO()
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as z:
                z.writestr("paper.tex", "\\documentclass{article}\n\\begin{document}\nNative import.\n\\end{document}\n")
            content = archive.getvalue()
            class Handler(BaseHTTPRequestHandler):
                def do_GET(self):
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(content)))
                    self.end_headers()
                    self.wfile.write(content)
                def log_message(self, *args):
                    pass
            http = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            threading.Thread(target=http.serve_forever, daemon=True).start()
            imported = api("/api/projects/import-url", {"name": "native", "url": f"http://127.0.0.1:{http.server_port}/paper.zip"})
            assert imported["main"] == "paper.tex", imported
            assert (scratch / "projects/native/paper.tex").is_file()
            print("PASS: HTTP ZIP import, Git initialization and Rust main-file detection without curl", flush=True)
        finally:
            if http:
                http.shutdown()
                http.server_close()
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
