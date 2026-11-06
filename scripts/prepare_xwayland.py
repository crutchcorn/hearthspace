"""Explicit normal-user preparation of the pinned private Xwayland dependency."""

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
from pathlib import Path

from installlib import xwayland
from installlib.filesystem import no_symlinks
from xwayland_build import available_tools, compile_source, extract
from xwayland_provenance import build_receipt, verify_prepared
from xwayland_stage import compositor_path, destinations, stage

REPOSITORY = Path(__file__).resolve().parents[1]


def absolute_path(value):
    path = Path(value)
    if not path.is_absolute() or ".." in path.parts:
        raise argparse.ArgumentTypeError("use an absolute path without '..'")
    return path


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument(
        "--archive",
        required=True,
        type=absolute_path,
        help="pinned upstream .tar.xz; never downloaded here",
    )
    result.add_argument(
        "--build-dir",
        required=True,
        type=absolute_path,
        help="fresh private build directory; must not exist",
    )
    result.add_argument(
        "--compositor",
        required=True,
        type=absolute_path,
        help="actual prebuilt compositor executable (including custom Cargo targets)",
    )
    return result


def verified_copy(source, target, expected):
    shutil.copyfile(source, target)
    target.chmod(0o600)
    if xwayland.regular_file(target)["sha256"] != expected:
        raise RuntimeError(f"input changed while copying: {source}")


def prepare(args):
    if os.geteuid() == 0:
        raise RuntimeError("prepare Xwayland as your normal user, never through sudo")
    compositor = compositor_path(args.compositor)
    destinations(compositor)
    root = args.build_dir
    no_symlinks(root)
    if root.exists() or not root.parent.is_dir():
        raise RuntimeError(
            "--build-dir must be fresh, with an existing nonsymlink parent"
        )
    expected = xwayland.requirements()
    patch = REPOSITORY / "patches/xwayland-surface-input-targets.patch"
    notice = REPOSITORY / "patches/xwayland-COPYING"
    inputs = (
        (args.archive, expected["source_archive_sha256"]),
        (patch, expected["surface_input_targets_sha256"]),
    )
    # Verify both inputs before creating a build tree or running any program.
    for path, digest in inputs:
        if xwayland.regular_file(path)["sha256"] != digest:
            raise RuntimeError(
                f"input sha256 does not match trusted requirements: {path}"
            )
    notice_digest = xwayland.regular_file(notice)["sha256"]
    tools = available_tools()
    root.mkdir(mode=0o700)
    archive = root / "source.tar.xz"
    local_patch = root / "surface-input-targets.patch"
    for (source, digest), destination in zip(
        inputs, (archive, local_patch), strict=True
    ):
        verified_copy(source, destination, digest)
    verified_copy(notice, root / "Xwayland-COPYING", notice_digest)
    source = extract(archive, root / "source", expected["version"])
    if xwayland.regular_file(source / "COPYING")["sha256"] != notice_digest:
        raise RuntimeError(
            "upstream COPYING does not match the preserved Xwayland notice"
        )
    binary = compile_source(root, source, local_patch, tools)
    if xwayland.requirements() != expected:
        raise RuntimeError("trusted Xwayland requirements changed during preparation")
    commands = root / "build/compile_commands.json"
    provenance = build_receipt(
        binary, source, commands, (root / "compiler-inputs.log").read_text()
    )
    receipt = {**expected, "sha256": provenance["sha256"]}
    # This receipt is separate from the stable runtime companion contract.
    (root / "build-receipt.json").write_text(
        json.dumps(provenance, sort_keys=True, indent=2) + "\n"
    )
    binary.with_name(binary.name + ".json").write_text(
        json.dumps(receipt, sort_keys=True, indent=2) + "\n"
    )
    verify_prepared(binary, source, commands)
    prepared = stage(binary, compositor, receipt)
    verify_prepared(prepared, source, commands)
    return {
        "binary": str(prepared),
        "receipt": str(prepared.with_name(xwayland.RECEIPT)),
        "build_receipt": str(root / "build-receipt.json"),
        "source": str(source),
        "compile_commands": str(commands),
        "sha256": receipt["sha256"],
    }


def main():
    def interrupted(signum, _frame):
        raise KeyboardInterrupt(f"interrupted by signal {signum}")

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGHUP, interrupted)
    try:
        print(json.dumps(prepare(parser().parse_args()), sort_keys=True, indent=2))
    except (
        OSError,
        ValueError,
        RuntimeError,
        subprocess.SubprocessError,
        KeyboardInterrupt,
    ) as error:
        print(f"prepare-xwayland: {error}", file=sys.stderr)
        raise SystemExit(1) from None


if __name__ == "__main__":
    main()
