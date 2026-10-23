#!/usr/bin/python3
"""Command-boundary stand-in; never connects to a real systemd manager."""
import json
import os
from pathlib import Path
import sys
import time

root = Path(__file__).parent
args = sys.argv[1:]
with root.joinpath("commands.jsonl").open("a") as log:
    log.write(json.dumps({
        "args": args,
        "runtime": os.environ["XDG_RUNTIME_DIR"],
        "bus": os.environ["DBUS_SESSION_BUS_ADDRESS"],
    }) + "\n")
unit = args[-1]
if root.joinpath("fail").exists() and root.joinpath("fail").read_text() == unit:
    sys.exit(7)
if root.joinpath("stall").exists():
    root.joinpath("stalled-pid").write_text(f"{os.getpid()}\n")
    time.sleep(60)
if not root.joinpath("no-publication").exists():
    root.joinpath(unit).touch()
