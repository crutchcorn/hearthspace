#!/usr/bin/python3 -I
"""Cargo/display boundary only; the supervisor, broker and D-Bus clients are real."""

import ctypes
import json
import os
from pathlib import Path
import signal
import socket
import sys
import time

root = Path(os.environ["HEARTHSPACE_TEST_DEV_BUS_ROOT"])
parent = os.getppid()
assert ctypes.CDLL(None).prctl(1, signal.SIGKILL, 0, 0, 0) == 0
if os.getppid() != parent:
    sys.exit(98)
signal.alarm(30)

assert "--locked" in sys.argv
binary = sys.argv[sys.argv.index("--bin") + 1]
if sys.argv[1] == "build":
    assert binary == "hearthspace-lock"
    artifact = root / "locker-fixture"
    artifact.write_text("#!/bin/sh\nexit 0\n")
    artifact.chmod(0o700)
    print(json.dumps({
        "reason": "compiler-artifact",
        "target": {"name": binary, "kind": ["bin"]},
        "profile": {"test": False},
        "executable": str(artifact),
    }))
    sys.exit(0)

assert sys.argv[1] == "run"
(root / (binary + ".started")).write_text(str(os.getpid()))
if binary == "hearthspace-settingsd":
    if os.environ["HEARTHSPACE_TEST_DEV_BUS_MODE"] == "stalled-settings":
        time.sleep(25)
    with socket.socket(socket.AF_UNIX) as server:
        server.bind(os.environ["HEARTHSPACE_SETTINGS_SOCKET"])
        server.listen()
        while True:
            peer, _ = server.accept()
            with peer:
                assert peer.recv(1024) == b"get\n"
                peer.sendall(b"ok {}\n")
elif binary == "hearthspace-compositor":
    assert "--tty" in sys.argv
    wayland = socket.socket(socket.AF_UNIX)
    wayland.bind(str(Path(os.environ["XDG_RUNTIME_DIR"]) / os.environ["HEARTHSPACE_WAYLAND_DISPLAY"]))
    wayland.listen()
    with socket.socket(socket.AF_UNIX) as server:
        server.bind(os.environ["HEARTHSPACE_COMMAND_SOCKET"])
        server.listen()
        while True:
            peer, _ = server.accept()
            with peer:
                request = peer.recv(1024)
                if request == b"status\n":
                    payload = json.dumps({
                        "backend": "native", "active": True, "locked": False,
                        "shell_ready": (root / "shell.json").exists(),
                        "recovery_ready": True,
                        "outputs": [{"name": "fixture", "frame_id": 1}],
                    }).encode()
                else:
                    assert request == b"capture fixture\n"
                    payload = json.dumps({
                        "output": "fixture", "presentation": "page-flip", "frame_id": 2,
                    }).encode() + b"\n\x89PNG\r\n\x1a\n"
                peer.sendall(("ok %d\n" % len(payload)).encode() + payload)
elif binary == "hearthspace-shell":
    os.environ["HEARTHSPACE_TEST_DEV_BUS_CHILD"] = "1"
    executable = os.environ["HEARTHSPACE_TEST_DEV_BUS_EXE"]
    os.execv(executable, [executable, "--exact", "development_shell_probe", "--nocapture"])
else:
    raise AssertionError("unexpected Cargo binary: " + binary)
