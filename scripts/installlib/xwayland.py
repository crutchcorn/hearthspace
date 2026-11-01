"""Verify the private Xwayland's local-build provenance without executing it.

Receipts detect stale or mixed artifacts; they are not signed attestations.
The repository requirements are authoritative, never the version in a receipt.
"""

import hashlib
import json
import os
import re
import stat
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
BINARY = "hearthspace-Xwayland"
RECEIPT = BINARY + ".json"
NOTICE = Path("share/doc/hearthspace/Xwayland-COPYING")
FIELDS = (
    "format_version",
    "version",
    "source_archive_sha256",
    "surface_input_targets_sha256",
)
RELEASE_MODES = {f"bin/{BINARY}": 0o755, f"bin/{RECEIPT}": 0o644, str(NOTICE): 0o644}
PREPARE_HINT = (
    "run scripts/prepare-xwayland as your normal user with --archive, "
    "--build-dir and --compositor before starting or installing Hearthspace"
)


def regular_file(path, *, executable=False, contents=False):
    """Hash an opened nonsymlink regular file without blocking on a FIFO."""
    path = Path(path)
    try:
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(descriptor, "rb") as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode):
                raise RuntimeError(f"not a regular nonsymlink file: {path}")
            if executable and (
                not before.st_mode & 0o111 or not os.access(path, os.X_OK)
            ):
                raise RuntimeError(f"not executable: {path}")
            data = stream.read() if contents else None
            digest = (
                hashlib.sha256(data).hexdigest()
                if contents
                else hashlib.file_digest(stream, "sha256").hexdigest()
            )
            after = os.fstat(stream.fileno())
            if (before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (
                after.st_size,
                after.st_mtime_ns,
                after.st_ctime_ns,
            ):
                raise RuntimeError(f"file changed while verifying: {path}")
    except OSError as error:
        raise RuntimeError(
            f"cannot read regular nonsymlink file {path}: {error}"
        ) from error
    record = {
        "path": str(path),
        "sha256": digest,
        "mtime": after.st_mtime,
        "mode": stat.S_IMODE(after.st_mode),
    }
    return (record, data) if contents else record


def _object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON field: {key}")
        result[key] = value
    return result


def json_file(path):
    record, content = regular_file(path, contents=True)
    try:
        value = json.loads(content, object_pairs_hook=_object)
        if not isinstance(value, dict):
            raise TypeError("expected a JSON object")
    except (ValueError, TypeError, UnicodeError) as error:
        raise RuntimeError(f"malformed JSON in {path}: {error}") from error
    return record, value


def is_digest(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def requirements():
    _, values = json_file(REPOSITORY / "data/xwayland-requirements.json")
    if (
        type(values.get("format_version")) is not int
        or values["format_version"] != 1
        or not isinstance(values.get("version"), str)
        or not re.fullmatch(r"\d+\.\d+\.\d+", values["version"])
        or not all(is_digest(values.get(key)) for key in FIELDS[2:])
    ):
        raise RuntimeError("invalid trusted Xwayland requirements")
    return {key: values[key] for key in FIELDS}


def check_requirements(receipt):
    for key, expected in requirements().items():
        if type(receipt.get(key)) is not type(expected) or receipt[key] != expected:
            raise RuntimeError(
                f"Xwayland receipt {key} does not match trusted requirements"
            )
    if not is_digest(receipt.get("sha256")):
        raise RuntimeError("Xwayland receipt has no valid binary sha256")


def verify_binary(binary):
    """Return exact binary/receipt file records and receipt values; never launch it."""
    binary = Path(binary)
    try:
        record = regular_file(binary, executable=True)
        receipt_record, receipt = json_file(binary.with_name(binary.name + ".json"))
        check_requirements(receipt)
        if record["sha256"] != receipt["sha256"]:
            raise RuntimeError(
                f"Xwayland binary sha256 does not match receipt: {binary}"
            )
        return {"binary": record, "receipt": receipt_record, "provenance": receipt}
    except RuntimeError as error:
        raise RuntimeError(f"{error}; {PREPARE_HINT}") from error


def verify_companions(compositor):
    """Find companions beside the actual selected compositor, including overrides."""
    try:
        compositor = Path(compositor).resolve(strict=True)
    except OSError as error:
        raise RuntimeError(
            f"compositor is unavailable: {compositor}; {PREPARE_HINT}"
        ) from error
    regular_file(compositor, executable=True)
    result = verify_binary(compositor.with_name(BINARY))
    records = {BINARY: result["binary"], RECEIPT: result["receipt"]}
    for name, record in records.items():
        expected_mode = RELEASE_MODES[f"bin/{name}"]
        if record["mode"] != expected_mode:
            raise RuntimeError(
                f"published Xwayland companion {record['path']} must have mode "
                f"{expected_mode:04o}, found {record['mode']:04o}; {PREPARE_HINT}"
            )
    return records
