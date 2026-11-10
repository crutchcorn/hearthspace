"""Read-only verification of a prepared Xwayland and its exact build inputs."""

from pathlib import Path

from installlib import xwayland
from installlib.filesystem import no_symlinks

BUILD_CONFIGURATION = (
    "compile_commands.json",
    "build.ninja",
    "meson-private/coredata.dat",
    "meson-private/cmd_line.txt",
    "meson-info/intro-buildoptions.json",
)
SOURCE_INPUTS = (
    "hw/xwayland/xwayland-input.c",
    "hw/xwayland/xwayland-input.h",
    "hw/xwayland/xwayland-input-target.c",
    "hw/xwayland/xwayland-input-target.h",
    "dix/inpututils.c",
    "include/inpututils.h",
)


def source_inventory(source):
    no_symlinks(source)
    if not source.is_dir():
        raise RuntimeError(f"missing prepared source directory: {source}")
    files = {}
    for path in sorted(source.rglob("*")):
        if path.is_symlink():
            raise RuntimeError(f"symlink in prepared source: {path}")
        if not path.is_dir():
            files[str(path.relative_to(source))] = xwayland.regular_file(path)["sha256"]
    for name in (*SOURCE_INPUTS, "COPYING"):
        if name not in files:
            raise RuntimeError(f"prepared source is missing {name}")
    return files


def build_inventory(build):
    no_symlinks(build)
    files = {}
    paths = {build / name for name in BUILD_CONFIGURATION}
    paths.update(
        path for path in build.rglob("*") if path.suffix in (".h", ".c", ".inc")
    )
    for path in sorted(paths):
        no_symlinks(path)
        files[str(path.relative_to(build))] = xwayland.regular_file(path)["sha256"]
    return files


def compiler_inventory(dependencies, build):
    """Ninja's depfile database records headers actually read by the compiler."""
    files = {}
    for line in dependencies.splitlines():
        if not line.startswith("    "):
            continue
        path = Path(line[4:])
        if not path.is_absolute():
            path = build / path
        # Preserve the compiler's path, including symlink ancestors and '..'.
        # A canonical target alone cannot detect a retargeted include directory.
        path = path.absolute()
        if str(path) not in files:
            target = path.resolve(strict=True)
            files[str(path)] = {
                "resolved_path": str(target),
                "sha256": xwayland.regular_file(target)["sha256"],
            }
    if not files:
        raise RuntimeError(
            "Ninja reported no compiler inputs; cannot record build provenance"
        )
    return files


def build_receipt(binary, source, compile_commands, dependencies):
    """Called only after the pinned, freshly extracted build has succeeded."""
    return {
        **xwayland.requirements(),
        "sha256": xwayland.regular_file(binary, executable=True)["sha256"],
        "source_directory": str(source),
        "compile_commands": {
            "path": str(compile_commands),
            "sha256": xwayland.regular_file(compile_commands)["sha256"],
        },
        "source_files": source_inventory(source),
        "build_files": build_inventory(compile_commands.parent),
        "compiler_inputs": compiler_inventory(dependencies, compile_commands.parent),
    }


def verify_prepared(binary, source, compile_commands, *, receipt_path=None):
    """Reject mismatched source/build/binary inputs without launching any process.

    `binary` is a companion with its adjacent `.json` runtime receipt. The build
    receipt defaults to `compile_commands.parent.parent / 'build-receipt.json'`.
    Returns the verified build receipt. Raises RuntimeError on verification failure.
    """
    binary, source, compile_commands = map(Path, (binary, source, compile_commands))
    if not all(path.is_absolute() for path in (binary, source, compile_commands)):
        raise RuntimeError("prepared Xwayland paths must be absolute")
    verified = xwayland.verify_binary(binary)
    receipt_path = (
        Path(receipt_path)
        if receipt_path is not None
        else compile_commands.parent.parent / "build-receipt.json"
    )
    _, receipt = xwayland.json_file(receipt_path)
    xwayland.check_requirements(receipt)
    if receipt["sha256"] != verified["binary"]["sha256"]:
        raise RuntimeError(
            "prepared source/build receipt does not match Xwayland binary sha256"
        )
    if receipt.get("source_directory") != str(source):
        raise RuntimeError("prepared source directory does not match build receipt")
    expected_commands = {
        "path": str(compile_commands),
        "sha256": xwayland.regular_file(compile_commands)["sha256"],
    }
    if receipt.get("compile_commands") != expected_commands:
        raise RuntimeError("compile_commands path/sha256 does not match build receipt")
    if receipt.get("source_files") != source_inventory(source):
        raise RuntimeError("prepared source inventory does not match build receipt")
    if receipt.get("build_files") != build_inventory(compile_commands.parent):
        raise RuntimeError("prepared build inventory does not match build receipt")
    inputs = receipt.get("compiler_inputs")
    if not isinstance(inputs, dict) or not inputs:
        raise RuntimeError("build receipt has no compiler input inventory")
    for name, expected in inputs.items():
        path = Path(name)
        if (
            not path.is_absolute()
            or not isinstance(expected, dict)
            or set(expected) != {"resolved_path", "sha256"}
            or not isinstance(expected["resolved_path"], str)
            or not Path(expected["resolved_path"]).is_absolute()
            or not xwayland.is_digest(expected["sha256"])
        ):
            raise RuntimeError(
                f"invalid compiler input record in build receipt: {name}"
            )
        try:
            target = path.resolve(strict=True)
        except (OSError, RuntimeError) as error:
            raise RuntimeError(
                f"cannot resolve recorded compiler input: {name}"
            ) from error
        if str(target) != expected["resolved_path"]:
            raise RuntimeError(
                f"compiler input resolution does not match build receipt: {name}"
            )
        if xwayland.regular_file(target)["sha256"] != expected["sha256"]:
            raise RuntimeError(f"compiler input does not match build receipt: {name}")
    return receipt
