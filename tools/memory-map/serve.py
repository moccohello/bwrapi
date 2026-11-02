"""원본 JSON 장부의 변경을 지도에 자동 반영하는 로컬 서버."""

import argparse
import hashlib
import json
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import subprocess
import sys
import threading
from urllib.parse import quote, urlsplit
import webbrowser

HERE = Path(__file__).resolve().parent
LEDGER = HERE.parents[1] / "docs" / "memory" / "memory-addresses.json"
OUTPUT = HERE / "memory-map.json"


class MapData:
    def __init__(self):
        self.lock = threading.Lock()
        self.signature = None
        self.error = ""
        self.content = b""
        self.etag = ""
        self.sources = set()
        if OUTPUT.exists():
            self.accept_output()

    def accept_output(self):
        content = OUTPUT.read_bytes()
        data = json.loads(content)
        self.sources = {Path(source["path"]) for source in data.get("sources", [])}
        self.content = content
        self.etag = '"' + hashlib.sha256(content).hexdigest() + '"'

    def source_signature(self):
        files = self.sources | {LEDGER, HERE / "build-data.py", HERE / "display-names.json", HERE / "source-paths.json"}
        result = []
        for path in sorted(files, key=str):
            try:
                stat = path.stat()
                result.append((str(path), stat.st_mtime_ns, stat.st_size))
            except OSError:
                result.append((str(path), None, None))
        return tuple(result)

    def read(self):
        with self.lock:
            signature = self.source_signature()
            if signature != self.signature:
                self.signature = signature
                try:
                    result = subprocess.run(
                        [sys.executable, str(HERE / "build-data.py"), str(LEDGER)],
                        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=30,
                    )
                    if result.returncode:
                        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
                    self.accept_output()
                    self.error = ""
                except Exception as error:
                    self.error = str(error)[-1200:]
            return self.content, self.etag, self.error


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(HERE), **kwargs)

    def do_GET(self):
        if urlsplit(self.path).path != "/memory-map.json":
            return super().do_GET()
        content, etag, error = self.server.map_data.read()
        if not content:
            self.send_error(503, "Map data is not available")
            return
        unchanged = self.headers.get("If-None-Match") == etag
        self.send_response(304 if unchanged else 200)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("ETag", etag)
        self.send_header("X-Map-Sync", "stale" if error else "live")
        if error:
            self.send_header("X-Map-Error", quote(error))
        if not unchanged:
            self.send_header("Content-Length", str(len(content)))
        self.end_headers()
        if not unchanged:
            self.wfile.write(content)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="BWRAPI 원본 장부 자동 갱신 지도")
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--no-browser", action="store_true")
    args = parser.parse_args()
    with ThreadingHTTPServer(("127.0.0.1", args.port), Handler) as server:
        server.map_data = MapData()
        url = f"http://127.0.0.1:{server.server_port}/"
        print(f"메모리 지도: {url}\n원본 장부: {LEDGER}", flush=True)
        print("원본 변경은 열린 화면에 자동 반영됩니다. 종료: Ctrl+C", flush=True)
        if not args.no_browser:
            webbrowser.open(url)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass