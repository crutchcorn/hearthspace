"""Build as the caller; elevate only the file installation transaction."""

import argparse
import os
import subprocess
import sys
import tempfile
from pathlib import Path

from . import layout, transaction


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("action", choices=("install", "uninstall", "_apply"))
    result.add_argument("--prefix", type=layout.prefix_path, default=Path("/usr/local"))
    result.add_argument(
        "--destdir", type=Path, help="stage under this directory without sudo"
    )
    result.add_argument("--profile", choices=("release", "debug"), default="release")
    result.add_argument(
        "--binary-dir",
        type=Path,
        help="prebuilt binary directory (requires --no-build)",
    )
    result.add_argument(
        "--no-build", action="store_true", help="use an already-built native set"
    )
    result.add_argument("--bundle", type=Path, help=argparse.SUPPRESS)
    return result


def root_path(args):
    if args.destdir is None:
        return args.prefix
    if not args.destdir.is_absolute() or ".." in args.destdir.parts:
        raise RuntimeError("--destdir must be absolute without parent components")
    return args.destdir / args.prefix.relative_to("/")


def privileged(args, action, bundle=None):
    if args.destdir is not None or os.geteuid() == 0:
        if action == "uninstall":
            transaction.uninstall(root_path(args), args.prefix)
            return None
        return transaction.install(bundle, root_path(args), args.prefix)
    command = [
        "sudo",
        "--",
        sys.executable,
        str(layout.REPOSITORY / "scripts/install-session"),
        action,
        "--prefix",
        str(args.prefix),
    ]
    if bundle is not None:
        command.extend(["--bundle", str(bundle)])
    subprocess.run(command, check=True)
    return None


def run(args):
    if args.action == "uninstall":
        privileged(args, "uninstall")
        print(
            "Removed verified Hearthspace files; other desktops and configuration were preserved."
        )
        return
    if args.action == "_apply":
        if args.bundle is None:
            raise RuntimeError("internal apply requires a prepared bundle")
        transaction.install(args.bundle, root_path(args), args.prefix)
        return
    if args.binary_dir is not None and not args.no_build:
        raise RuntimeError("--binary-dir requires --no-build")
    if args.destdir is None:
        layout.dependencies()
    if not args.no_build:
        layout.build(args.profile)
    binary_dir = args.binary_dir or layout.REPOSITORY / "target" / args.profile
    with tempfile.TemporaryDirectory(prefix="hearthspace-install-") as temporary:
        prepared = Path(temporary)
        spec = layout.bundle(prepared, binary_dir.resolve(), args.prefix)
        privileged(args, "_apply", prepared)
    print(
        f"Installed Hearthspace release {spec['release'][:12]} under {root_path(args)}."
    )
    print("Log out and choose Hearthspace at the graphical login screen.")


def main():
    try:
        run(parser().parse_args())
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"install-session: {error}", file=sys.stderr)
        raise SystemExit(1) from None
