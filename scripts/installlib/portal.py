"""Check the selected ScreenCast backend without starting a desktop service."""

import hashlib
import json
import os
import re
import subprocess
from pathlib import Path

BACKENDS = (
    Path("/usr/local/libexec/xdg-desktop-portal-wlr"),
    Path("/usr/libexec/xdg-desktop-portal-wlr"),
)
MINIMUM = (0, 8, 4)


def compatible(version):
    match = re.fullmatch(
        r"(?:[0-9]+:)?([0-9]+)\.([0-9]+)\.([0-9]+)(?:[-+].*)?", version
    )
    return bool(match and tuple(map(int, match.groups())) >= MINIMUM)


def validate_backend(paths=BACKENDS):
    path = next((path for path in paths if os.access(path, os.X_OK)), None)
    version = None
    if path is not None:
        receipt = path.with_name(path.name + ".json")
        if receipt.is_file():
            try:
                data = json.loads(receipt.read_text())
                with path.open("rb") as binary:
                    digest = hashlib.file_digest(binary, "sha256").hexdigest()
                if data.get("sha256") == digest:
                    version = data.get("version")
            except (OSError, ValueError, AttributeError):
                pass
        elif path == BACKENDS[1]:
            # Upstream has no --version command. On the documented Debian/Ubuntu
            # baseline the package database supplies the system binary's version.
            try:
                result = subprocess.run(
                    [
                        "dpkg-query",
                        "-W",
                        "-f=${Status}\n${Version}",
                        "xdg-desktop-portal-wlr",
                    ],
                    capture_output=True,
                    text=True,
                    timeout=5,
                    check=False,
                )
                status, _, candidate = result.stdout.partition("\n")
                if result.returncode == 0 and status == "install ok installed":
                    version = candidate.strip()
            except (OSError, subprocess.TimeoutExpired):
                pass
    if not isinstance(version, str) or not compatible(version):
        raise RuntimeError(
            "screen sharing requires a verified xdg-desktop-portal-wlr >= 0.8.4; "
            "see docs/SCREEN_SHARING.md for package or source-build setup"
        )
    return path
