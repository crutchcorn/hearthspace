"""Check the selected ScreenCast backend without starting a desktop service."""

import hashlib
import json
import os
from pathlib import Path

BACKENDS = (
    Path("/usr/local/libexec/xdg-desktop-portal-wlr"),
    Path("/usr/libexec/xdg-desktop-portal-wlr"),
)
VERSION = "0.8.4"
REVISION = "34153094662acd713241ca6cbbb20003ef67da5f"
POLICY_PATCH = (
    Path(__file__).resolve().parents[2]
    / "patches/xdg-desktop-portal-wlr-session-policy.patch"
)
START_PATCH = POLICY_PATCH.with_name("xdg-desktop-portal-wlr-start-session.patch")
LIFECYCLE_PATCH = POLICY_PATCH.with_name(
    "xdg-desktop-portal-wlr-capture-lifecycle.patch"
)


def patch_digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_backend(paths=BACKENDS):
    path = next((path for path in paths if os.access(path, os.X_OK)), None)
    verified = False
    if path is not None:
        receipt = path.with_name(path.name + ".json")
        if receipt.is_file():
            try:
                data = json.loads(receipt.read_text())
                with path.open("rb") as binary:
                    digest = hashlib.file_digest(binary, "sha256").hexdigest()
                verified = (
                    data.get("sha256") == digest
                    and data.get("version") == VERSION
                    and data.get("source_revision") == REVISION
                    and data.get("session_policy_sha256") == patch_digest(POLICY_PATCH)
                    and data.get("start_session_sha256") == patch_digest(START_PATCH)
                    and data.get("capture_lifecycle_sha256")
                    == patch_digest(LIFECYCLE_PATCH)
                )
            except (OSError, ValueError, AttributeError):
                pass
    if not verified:
        raise RuntimeError(
            "screen sharing requires a verified xdg-desktop-portal-wlr 0.8.4 "
            "build with Hearthspace's session-policy patch, Start-session fix, "
            "and capture-lifecycle repair; "
            "see docs/SCREEN_SHARING.md for source-build setup"
        )
    return path
