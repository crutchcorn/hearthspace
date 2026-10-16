"""Build and record native artifacts before acquiring a physical VT."""

import hashlib
import os
import subprocess
import time

from .common import BINARIES, ROOT, save


def build(args, session):
    required = BINARIES
    if getattr(args, "login_session", False):
        helper = args.binary_dir.resolve() / "hearthspace-login-guard"
        if not helper.is_file() or not os.access(helper, os.X_OK):
            raise RuntimeError(
                "--login-session requires an executable installed cleanup helper: "
                f"{helper}; use --no-build --binary-dir with an installed release"
            )
        share = helper.parent.parent / "share"
        for relative in (
            "applications/dev.hearthspace.Settings.desktop",
            "icons/hicolor/scalable/apps/hearthspace.svg",
        ):
            resource = share / relative
            if not resource.is_file() or not os.access(resource, os.R_OK):
                raise RuntimeError(
                    f"--login-session requires installed shared data: {resource}"
                )
        required += ("hearthspace-login-guard",)
    if not args.no_build:
        with (session / "build.log").open("w") as log:
            command = [
                "cargo",
                "build",
                "--locked",
                "--target-dir",
                str(ROOT / "target"),
                "-p",
                "hearthspace-session",
                "-p",
                "hearthspace-compositor",
                "-p",
                "hearthspace-shell",
                "-p",
                "hearthspace-settings",
                "-p",
                "hearthspace-settingsd",
                "-p",
                "waydriver-hearthspace",
                "-p",
                "hearthspace-notifications",
                "--features",
                "hearthspace-compositor/udev",
            ]
            # Pin the artifact path even if the caller sets CARGO_TARGET_DIR.
            if args.binary_dir.resolve() != ROOT / "target/debug":
                raise ValueError("a custom --binary-dir requires --no-build")
            commands = (
                development_commands() if getattr(args, "dev", False) else [command]
            )
            deadline = time.monotonic() + 1800
            for command in commands:
                result = subprocess.run(
                    command,
                    check=False,
                    cwd=ROOT,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    timeout=max(0.01, deadline - time.monotonic()),
                )
                if result.returncode:
                    raise RuntimeError(
                        f"native build failed; see {session / 'build.log'}"
                    )
    binaries = {}
    for name in required:
        path = args.binary_dir.resolve() / name
        if not path.is_file() or not os.access(path, os.X_OK):
            raise RuntimeError(f"missing executable {path}; build before switching VTs")
        # Retain exact binary provenance even when the caller explicitly skips building.
        with path.open("rb") as binary_file:
            digest = hashlib.file_digest(binary_file, "sha256").hexdigest()
        binaries[name] = {
            "path": str(path),
            "sha256": digest,
            "mtime": path.stat().st_mtime,
        }
    save(session / "binaries.json", binaries)


def development_commands():
    # Each cargo-run child resolves features independently. A single workspace
    # build unifies features and leaves expensive recompilation until after the
    # VT switch. Warm the exact child graphs before recording binary provenance.
    prefix = ["cargo", "build", "--locked", "--target-dir", str(ROOT / "target")]
    commands = [prefix + ["-p", "hearthspace-session", "-p", "waydriver-hearthspace"]]
    for package in (
        "hearthspace-compositor",
        "hearthspace-settingsd",
        "hearthspace-shell",
        "hearthspace-notifications",
        "hearthspace-settings",
    ):
        command = prefix + ["-p", package]
        if package == "hearthspace-compositor":
            command.extend(["--features", "udev"])
        commands.append(command)
    return commands
