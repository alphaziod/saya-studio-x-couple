"""The bridge refuses what could leak the Civitai token or open it to anyone."""
import importlib.util
import json
import os
from pathlib import Path
import stat
import tempfile
import threading
import urllib.error
import urllib.request

spec = importlib.util.spec_from_file_location("bridge", Path(__file__).with_name("civitai_bridge.py"))
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


def refused(function, *args):
    try:
        function(*args)
    except ValueError:
        return True
    return False


# Download addresses: Civitai over https only.
for url in ("https://civitai.com/api/download/models/1", "https://civitai.red/x", "https://cdn.civitai.com/x"):
    assert bridge.trusted_url(url), url
for url in (
    "http://civitai.com/x", "file:///etc/passwd", "ftp://civitai.com/x", "https://evilcivitai.com/x",
    "https://civitai.com.evil.example/x", "https://evil.example/civitai.com", "https://civitai.com" + chr(64) + "evil.example/x",
    "", "javascript:alert(1)", "http://127.0.0.1:47993/evil.bin",
):
    assert not bridge.trusted_url(url), url
saved = bridge.API_BASE
bridge.API_BASE = "http://127.0.0.1:9/api/v1"
assert bridge.trusted_url("http://127.0.0.1:9/file"), "a test API may serve its own files"
assert not bridge.trusted_url("http://127.0.0.1:10/file")
bridge.API_BASE = saved

# Ids.
assert bridge.positive_id(12, "x") == 12 and bridge.positive_id("34", "x") == 34
for value in ("../../evil", "1/2", "1?a=b", "", " 1", "-1", 0, "0", True, None, [1], "1" * 40, 1.5):
    assert refused(bridge.positive_id, value, "x"), value

# File choice: the requested file or an error, never another one in its place.
files = [{"id": 1}, {"id": 2, "primary": True}, {"id": 3}]
assert bridge.pick_file(files, None)["id"] == 2
assert bridge.pick_file(files, 3)["id"] == 3
assert bridge.pick_file(files, "3")["id"] == 3, "the extension sends digits"
assert refused(bridge.pick_file, files, 9)
assert bridge.pick_file([], None) is None

# Token file and authentication, through the real server.
with tempfile.TemporaryDirectory() as directory:
    path = Path(directory) / "token"
    bridge.TOKEN_FILE = str(path)
    first = bridge.bridge_token()
    assert first
    if os.name == "posix":
        assert stat.S_IMODE(path.stat().st_mode) == 0o600
    assert bridge.bridge_token() == first
    path.write_text("")
    second = bridge.bridge_token()
    assert second and second != first, "an empty file gets a new token"
    server = bridge.ThreadingHTTPServer(("127.0.0.1", 0), bridge.H)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    port = server.server_address[1]

    def call(path_, token=None, body=None):
        headers = {"Content-Type": "application/json"}
        if token is not None:
            headers["X-Saya-Token"] = token
        data = None if body is None else json.dumps(body).encode()
        request = urllib.request.Request(f"http://127.0.0.1:{port}{path_}", data, headers)
        try:
            return urllib.request.urlopen(request, timeout=5).status
        except urllib.error.HTTPError as error:
            return error.code

    assert call("/health") == 403 and call("/health", "") == 403 and call("/health", "wrong") == 403
    assert call("/health", second) == 200
    assert call("/download", second, {"versionId": "../../evil"}) == 400
    assert call("/download", second, {"versionId": 1, "fileId": "x"}) == 400
    assert call("/download", second, [1]) == 400
    path.write_text("")
    assert call("/health", "") == 403, "an empty token file never opens the bridge"
    server.shutdown()
print("Civitai bridge: addresses, ids, file choice, token file and authentication")
