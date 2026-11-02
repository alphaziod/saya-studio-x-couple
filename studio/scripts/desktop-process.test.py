"""The bridge must observe its parent without terminating it, especially on Windows."""
import importlib.util
from pathlib import Path
import subprocess
import sys

spec = importlib.util.spec_from_file_location("bridge", Path(__file__).with_name("civitai_bridge.py"))
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)
child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])
try:
    assert bridge.parent_alive(child.pid)
    assert child.poll() is None, "Parent liveness probe killed the process"
finally:
    child.terminate()
    child.wait(timeout=10)
assert not bridge.parent_alive(child.pid)
print("Parent liveness: active process preserved, exited process detected")
