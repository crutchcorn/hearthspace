#!/usr/bin/python3
"""External backend fixture: records its launch, then owns names on the test bus."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(os.environ["HEARTHSPACE_TEST_CAST_ROOT"])
label = os.environ["HEARTHSPACE_TEST_CAST_LABEL"]
keys = ["DBUS_SESSION_BUS_ADDRESS", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR",
        "PIPEWIRE_RUNTIME_DIR", "PIPEWIRE_REMOTE", "PIPEWIRE_CORE",
        "GTK_USE_PORTAL", "GDK_BACKEND", "XDPW_PERSIST_MODE"]
record = {"pid": os.getpid(), "args": sys.argv[1:],
          "env": {key: os.environ.get(key) for key in keys}}
if label == "capture":
    assert len(sys.argv) == 5 and sys.argv[1] == "-c"
    assert sys.argv[3].startswith("--bus-name=") and sys.argv[4] == "--no-restore"
    record["config"] = Path(sys.argv[2]).read_text()
    os.environ["HEARTHSPACE_TEST_CAST_NAMES"] = sys.argv[3].split("=", 1)[1]
elif label in ("portal-gtk", "desktop-portal"):
    owner = subprocess.run([
        "busctl", "--address", os.environ["DBUS_SESSION_BUS_ADDRESS"],
        "--timeout=0.25", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus",
        "org.freedesktop.DBus", "GetConnectionUnixProcessID", "s",
        "org.freedesktop.impl.portal.desktop.hearthspace_wlr",
    ], capture_output=True, text=True, check=True)
    record["capture_pid"] = int(owner.stdout.split()[1])
temporary = root.joinpath(f"{label}.json.tmp")
temporary.write_text(json.dumps(record))
temporary.replace(root.joinpath(f"{label}.json"))
if label == "capture" and root.joinpath("unpatched-backend").exists():
    parser = argparse.ArgumentParser()
    parser.add_argument("-c")
    parser.parse_args()  # An unpatched backend rejects the required long options.
if root.joinpath(f"exit-{label}").exists():
    sys.exit(7)
executable = os.environ["HEARTHSPACE_TEST_CAST_EXE"]
os.execv(executable, [executable, "--exact",
    "desktop_services::startup::tests::support::portal_helper",
    "--ignored", "--nocapture"])
