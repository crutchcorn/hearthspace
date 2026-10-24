#!/usr/bin/python3 -I
"""Disposable Cargo/manager boundary; never builds or contacts a real manager."""
import ctypes
import json
import os
from pathlib import Path
import signal
import socket
import sys
import time

root = Path(os.environ["HEARTHSPACE_ARTIFACT_TEST_ROOT"])
role = Path(sys.argv[0]).name
parent = os.getppid()
assert ctypes.CDLL(None).prctl(1, signal.SIGKILL, 0, 0, 0) == 0
if os.getppid() != parent:
    sys.exit(98)
signal.alarm(12)

if role == "cargo":
    assert "--locked" in sys.argv
    binary = sys.argv[sys.argv.index("--bin") + 1]
    if sys.argv[1] == "build":
        if binary == "hearthspace-lock":
            package = "hearthspace-lock"
            prefix = "locker-"
            artifact = Path(os.environ["LOCKER_ARTIFACT_PATH"])
            mode = os.environ["LOCKER_ARTIFACT_MODE"]
        else:
            assert binary == "hearthspace-app-scope-guard"
            package = "hearthspace-session"
            prefix = ""
            artifact = Path(os.environ["ARTIFACT_PATH"])
            mode = os.environ["ARTIFACT_MODE"]
        assert sys.argv[sys.argv.index("-p") + 1] == package
        assert "--features" not in sys.argv
        assert "--message-format=json-render-diagnostics" in sys.argv
        (root / f"{prefix}build-called").write_text(json.dumps(sys.argv[1:]))
        if mode == "hang":
            (root / f"{prefix}build-pid").write_text(str(os.getpid()))
            time.sleep(30)
        if mode == "fresh":
            artifact.parent.mkdir(parents=True, exist_ok=True)
            artifact.write_text("#!/bin/sh\n# fresh helper\nexit 0\n")
            artifact.chmod(0o700)
        if mode != "no-message":
            print(json.dumps({
                "reason": "compiler-artifact",
                "target": {"name": binary, "kind": ["bin"]},
                "profile": {"test": False},
                "executable": str(artifact),
                "fresh": mode != "fresh",
            }))
        sys.exit(42 if mode == "fail" else 0)
    assert sys.argv[1] == "run"
    assert binary == "hearthspace-settingsd", "compositor must not acquire native resources"
    role = binary

if role == "hearthspace-settingsd":
    with socket.socket(socket.AF_UNIX) as server:
        server.bind(os.environ["HEARTHSPACE_SETTINGS_SOCKET"])
        server.listen()
        while True:
            peer, _ = server.accept()
            with peer:
                assert peer.recv(1024) == b"get\n"
                peer.sendall(b"ok {}\n")
elif role == "systemd-run":
    assert os.environ["XDG_RUNTIME_DIR"] == str(root / "host")
    artifact = Path(sys.argv[-4])
    (root / "owner-submitted").write_text(json.dumps({
        "helper": str(artifact), "contents": artifact.read_text()
    }))
    # End startup at the first manager boundary; no user scope, bus or VT exists.
    print("fixture intentionally ends owner submission", file=sys.stderr)
    sys.exit(19)
elif role == "systemctl":
    assert sys.argv[1:4] == ["--user", "--no-ask-password", "show"]
    print("LoadState=not-found\nActiveState=inactive")
    sys.exit(1)
else:
    raise AssertionError(f"unexpected fixture executable: {role}")
