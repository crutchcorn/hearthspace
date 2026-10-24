#!/usr/bin/python3 -I
"""Owned readiness stubs and a narrowly allowlisted disposable activation probe."""
import ctypes
import json
import os
from pathlib import Path
import signal
import socket
import sys


def write_report(path, values):
    temporary = str(path) + ".pending"
    with open(temporary, "x", encoding="utf-8") as output:
        json.dump(values, output)
    os.replace(temporary, path)


if sys.argv[1:2] == ["probe"]:
    signal.alarm(2)
    # This process inherits only the disposable broker's synthetic environment.
    # Never enumerate it, and never write ordinary user profile files.
    keys = ["XDG_RUNTIME_DIR", "WAYLAND_DISPLAY", "HEARTHSPACE_SESSION_DIR",
            "HEARTHSPACE_ACTIVATION_OWNER", "PATH"]
    write_report(sys.argv[2], {key: os.environ.get(key) for key in keys})
    # Deliberately exit without acquiring a name: activation reports an error.
    sys.exit(0)

root = Path(os.environ["HEARTHSPACE_ACTIVATION_TEST_ROOT"])
role = Path(sys.argv[0]).name.removeprefix("hearthspace-")
if role not in ("settingsd", "compositor"):
    (root / "unexpected-helper").write_text(role, encoding="utf-8")
    sys.exit(97)

# Each child has its own session/process group. Parent death and the alarm are
# emergency fixture cleanup only; the test separately requires SIGTERM receipts.
parent = os.getppid()
assert ctypes.CDLL(None).prctl(1, signal.SIGKILL, 0, 0, 0) == 0
if os.getppid() != parent:
    sys.exit(98)
signal.alarm(12)
start_time = Path("/proc/self/stat").read_text().rsplit(")", 1)[1].split()[19]
identity = {"pid": os.getpid(), "start_time": start_time}


def terminated(_signal, _frame):
    write_report(root / (role + "-stopped.json"), identity)
    sys.exit(0)


signal.signal(signal.SIGTERM, terminated)
write_report(root / (role + "-started.json"), identity)
if role == "compositor":
    assert "--tty" in sys.argv
    endpoint = Path(os.environ["XDG_RUNTIME_DIR"]) / os.environ["HEARTHSPACE_WAYLAND_DISPLAY"]
else:
    endpoint = os.environ["HEARTHSPACE_SETTINGS_SOCKET"]
with socket.socket(socket.AF_UNIX) as server:
    server.bind(str(endpoint))
    server.listen()
    while True:
        conn, _ = server.accept()
        with conn:
            if role == "settingsd":
                assert conn.recv(1024) == b"get\n"
                conn.sendall(b"ok {}\n")
