"""Require evidence that native startup reached the injected compositor failure."""

import json
from pathlib import Path

from .control import ControlError


def verify_startup_failure(directory, compositor):
    try:
        manifest = json.loads((directory / "session.json").read_text())
        identity = json.loads((directory / "identity.json").read_text())
        log = (directory / "session.log").read_text()
    except (OSError, ValueError) as error:
        raise ControlError(
            "startup failure did not record an allocated native session"
        ) from error

    try:
        valid = (
            Path(manifest["session_dir"]).resolve() == directory.resolve()
            and manifest["unit"] == f"hearthspace-native-{manifest['id']}.service"
            and 7 <= manifest["vt"] <= 15
            and manifest["vt"] != manifest["original_vt"]
            and identity["vt"] == manifest["vt"]
            and identity["uid"] == manifest["uid"]
            and identity["leader"] > 0
            and bool(identity["session_id"])
            and Path(manifest["compositor"]).resolve() == Path(compositor).resolve()
        )
    except (KeyError, TypeError, ValueError):
        valid = False
    if not valid:
        raise ControlError("startup failure has no matching injected native session")
    if "compositor exited before readiness: exit status: 1" not in log:
        raise ControlError("startup failed before the injected compositor failure")
    return {
        "unit": manifest["unit"],
        "vt": manifest["vt"],
        "session_id": identity["session_id"],
        "compositor": manifest["compositor"],
    }
