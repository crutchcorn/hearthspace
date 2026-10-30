#!/usr/bin/python3
"""Never contacts systemd; requires the owned backend before portal refresh."""
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).parent
owner = subprocess.run([
    "busctl", "--address", os.environ["DBUS_SESSION_BUS_ADDRESS"],
    "--timeout=0.25", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus",
    "org.freedesktop.DBus", "GetConnectionUnixProcessID", "s",
    "org.freedesktop.impl.portal.desktop.hearthspace_wlr",
], capture_output=True, text=True, check=True)
expected = json.loads(root.joinpath("capture.json").read_text())["pid"]
assert owner.stdout.strip() == f"u {expected}"
with root.joinpath("commands.jsonl").open("a") as log:
    log.write(json.dumps({"args": sys.argv[1:], "capture_pid": expected}) + "\n")
if root.joinpath("fail-manager").exists():
    sys.exit(7)
