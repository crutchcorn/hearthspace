"""Recover only the user-manager app scopes bound to this native session."""

import os
import pwd
import time
from pathlib import Path

from .common import run


def owner_identity(state):
    runtime = Path(state["runtime_dir"])
    expected = f"hearthspace-native-{state['id']}"
    if runtime.parent != Path(f"/run/user/{state['uid']}") or runtime.name != expected:
        raise RuntimeError("refusing unexpected app scope runtime")
    if (
        not expected
        or len(expected) > 100
        or any(
            not (char.isascii() and (char.isalnum() or char in "-_"))
            for char in expected
        )
    ):
        raise RuntimeError("invalid native app scope identity")
    return runtime, f"hearthspace-apps-{expected}.service"


def manager_command(uid, *args):
    account = pwd.getpwuid(uid)
    # Contact the user manager only. No imports into its shared environment.
    return [
        "/usr/bin/setpriv",
        f"--reuid={uid}",
        f"--regid={account.pw_gid}",
        "--init-groups",
        "/usr/bin/env",
        f"XDG_RUNTIME_DIR=/run/user/{uid}",
        f"DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{uid}/bus",
        "LC_ALL=C",
        "/usr/bin/systemctl",
        "--user",
        "--no-ask-password",
        "--no-pager",
        *args,
    ]


def properties(uid, unit, deadline):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise TimeoutError("owned app scope recovery deadline exceeded")
    result = run(
        manager_command(
            uid, "show", unit, "--property=LoadState,ActiveState,Description,BoundBy"
        ),
        check=False,
        timeout=min(remaining, 2),
    )
    if len(result.stdout) > 65536:
        raise RuntimeError("oversized app scope property response")
    values = dict(
        line.split("=", 1) for line in result.stdout.splitlines() if "=" in line
    )
    if result.returncode and values.get("LoadState") != "not-found":
        raise RuntimeError(f"cannot inspect owned app scope: {result.stderr}")
    return values


def stopped(values):
    return values.get("LoadState") == "not-found" or values.get("ActiveState") in {
        "inactive",
        "failed",
    }


def scope_records(runtime):
    directory = runtime / "app-scopes"
    if not directory.exists():
        return set()
    if directory.is_symlink() or not directory.is_dir():
        raise RuntimeError("invalid app scope record directory")
    names = set()
    for entry in directory.iterdir():
        if (
            len(names) >= 512
            or entry.is_symlink()
            or not entry.is_file()
            or not entry.name.startswith("snap.")
            or not entry.name.endswith(".scope")
            or any(
                not (char.isascii() and (char.isalnum() or char in "-_."))
                for char in entry.name
            )
        ):
            raise RuntimeError("invalid or excessive app scope records")
        names.add(entry.name)
    return names


def cleanup_app_scopes(state, deadline):
    runtime, unit = owner_identity(state)
    if runtime.exists():
        if runtime.is_symlink():
            raise RuntimeError("refusing symlink app scope runtime")
        fd = os.open(
            runtime / "app-scopes-closing",
            os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW,
            0o600,
        )
        os.close(fd)
    uid = state["uid"]
    records = scope_records(runtime)
    values = properties(uid, unit, deadline)
    owner_missing = values.get("LoadState") == "not-found"
    if (
        not owner_missing
        and values.get("Description") != f"Hearthspace app scopes for {runtime}"
    ):
        raise RuntimeError("owned app scope service identity changed; refusing stop")
    dependent_units = values.get("BoundBy", "").split()
    if len(dependent_units) > 128 or any(
        not name.startswith("snap.")
        or not name.endswith(".scope")
        or any(
            not (char.isascii() and (char.isalnum() or char in "-_.")) for char in name
        )
        for name in dependent_units
    ):
        raise RuntimeError("unexpected bound app scope identity")
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise TimeoutError("owned app scope recovery deadline exceeded")
    if not owner_missing:
        run(manager_command(uid, "stop", unit), timeout=min(remaining, 3))
    candidates = sorted(records | set(dependent_units))
    if not owner_missing:
        candidates.insert(0, unit)
    while True:
        if all(
            stopped(properties(uid, candidate, deadline)) for candidate in candidates
        ):
            return True
        if time.monotonic() >= deadline:
            raise TimeoutError("owned app scope is still active at recovery deadline")
        time.sleep(min(0.05, max(0, deadline - time.monotonic())))
