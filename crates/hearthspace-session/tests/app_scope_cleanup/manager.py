#!/usr/bin/python3 -I
"""Disposable systemctl boundary; never forwards to a real service manager."""
import os
from pathlib import Path
import sys

root = Path(os.environ["HEARTHSPACE_SCOPE_TEST_ROOT"])
runtime = root / "session"
owner = "hearthspace-apps-session.service"
recorded = "snap.test.recorded.scope"
bound = "ptyxis-spawn-01234567-89ab-cdef-0123-456789abcdef.scope"

if Path(sys.argv[0]).name == "hearthspace-settingsd":
    # Model persisted app-scope startup intent whose in-memory handle was never
    # returned. The real supervisor still owns all shutdown/removal decisions.
    assert Path(os.environ["HEARTHSPACE_SESSION_DIR"]) == runtime
    (runtime / "app-scope-owner").write_text(owner)
    (runtime / "app-scopes").mkdir()
    (runtime / "app-scopes" / recorded).touch()
    sys.exit(17)

assert Path(sys.argv[0]).name == "systemctl"
assert os.environ["XDG_RUNTIME_DIR"] == str(root / "host")
assert os.environ["DBUS_SESSION_BUS_ADDRESS"] == f"unix:path={root}/host/bus"
assert sys.argv[1:3] == ["--user", "--no-ask-password"]
action, unit = sys.argv[3:5]
assert unit in (owner, recorded, bound)
assert action in ("show", "stop")
with (root / "commands").open("a") as log:
    log.write(f"{action} {unit}\n")

if action == "stop":
    assert unit == owner
    if (root / "stop-fails").exists():
        print("fixture stop failed", file=sys.stderr)
        sys.exit(1)
    (root / "owner-stopped").touch()
elif unit == owner:
    if (root / "owner-collected").exists():
        print("LoadState=not-found\nActiveState=inactive")
        sys.exit(1)
    description = (
        "foreign owner"
        if (root / "foreign-owner").exists()
        else f"Hearthspace app scopes for {runtime}"
    )
    state = "inactive" if (root / "owner-stopped").exists() else "active"
    print(
        f"LoadState=loaded\nActiveState={state}\n"
        f"Description={description}\nBoundBy={bound}"
    )
else:
    state = "inactive" if (root / f"release-{unit}").exists() else "deactivating"
    if state == "deactivating":
        (root / f"observed-{unit}").touch()
    print(f"LoadState=loaded\nActiveState={state}")
