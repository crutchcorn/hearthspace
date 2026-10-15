"""One marker-owned native Firefox profile; never inspect ordinary profiles."""

import json
import os
import shutil
from pathlib import Path

from .control import ControlError

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures/daily-browser"


def prepare(directory, marker):
    profile = directory / "firefox-profile"
    profile.mkdir(mode=0o700)
    ownership = {"run": str(directory), "uid": os.getuid(), "marker": marker}
    (profile / "hearthspace-probe-owner.json").write_text(json.dumps(ownership))
    with (directory / "firefox-profile-owner.json").open("x") as record:
        record.write(json.dumps(ownership))
    (profile / "user.js").write_text(
        'user_pref("browser.aboutwelcome.enabled", false);\n'
        'user_pref("browser.startup.homepage", "about:blank");\n'
        'user_pref("browser.newtabpage.enabled", false);\n'
    )
    shutil.copytree(FIXTURES, directory / "browser-fixtures")
    return profile


def verify_stopped(directory, marker):
    profile = directory / "firefox-profile"
    if profile.is_symlink():
        raise ControlError("probe profile became a symlink; preserving it")
    ownership = json.loads((profile / "hearthspace-probe-owner.json").read_text())
    if ownership != {"run": str(directory), "uid": os.getuid(), "marker": marker}:
        raise ControlError("probe profile ownership changed; preserving it")
    manifest = json.loads((directory / "session.json").read_text())
    if not manifest.get("cleanup", {}).get("complete"):
        raise ControlError("native cleanup is incomplete; preserving browser profile")
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            arguments = (entry / "cmdline").read_bytes().split(b"\0")
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            continue
        if os.fsencode(profile) in arguments:
            raise ControlError(
                f"owned Firefox PID {entry.name} remains; preserving profile"
            )
    return profile


def cleanup(directory, marker):
    profile = verify_stopped(directory, marker)
    shutil.rmtree(profile)
