"""Durable evidence and host telemetry, sampled outside the build cgroup."""

import hashlib
import json
import os
import re
import time
from pathlib import Path

import guarded_cargo_policy as policy


def sync_directory(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def save(path, value, *, replace=False):
    temporary = path.with_name(f".{path.name}.tmp")
    with temporary.open("x") as output:
        json.dump(value, output, sort_keys=True, indent=2)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    # Publish only complete records; the worker polls readiness concurrently.
    if replace:
        os.replace(temporary, path)
    else:
        os.link(temporary, path)
        temporary.unlink()
    sync_directory(path.parent)


def file_hash(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def fresh_directory(value):
    requested = Path(value).absolute()
    parent = requested.parent.resolve(strict=True)
    temporary_roots = [Path("/tmp"), Path("/var/tmp")]
    temporary_roots.extend(
        Path(os.environ[key])
        for key in ("TMPDIR", "TMP", "TEMP")
        if os.environ.get(key)
    )
    if any(
        (parent / requested.name).is_relative_to(root.resolve())
        for root in temporary_roots
    ):
        raise RuntimeError(
            "evidence must be outside temporary directories, even on disk-backed storage"
        )
    filesystem = None
    depth = -1
    for line in (policy.PROC / "self/mountinfo").read_text().splitlines():
        fields, kind = line.split(" - ", 1)
        mount = Path(
            re.sub(
                r"\\([0-7]{3})", lambda match: chr(int(match[1], 8)), fields.split()[4]
            )
        )
        if parent.is_relative_to(mount) and len(mount.parts) >= depth:
            filesystem, depth = kind.split()[0], len(mount.parts)
    if filesystem not in {
        "ext2",
        "ext3",
        "ext4",
        "btrfs",
        "xfs",
        "f2fs",
        "zfs",
        "bcachefs",
    }:
        raise RuntimeError(
            f"evidence needs a persistent local filesystem; found {filesystem}"
        )
    path = parent / requested.name
    path.mkdir(
        mode=0o700
    )  # Existing directories, including failed runs, are never reused.
    sync_directory(parent)
    return path


def source_record(git, cwd):
    root = Path(
        policy.command(
            [git, "-C", str(cwd), "rev-parse", "--show-toplevel"]
        ).stdout.strip()
    )
    head = policy.command([git, "-C", str(root), "rev-parse", "HEAD"]).stdout.strip()
    names = policy.command(
        [
            git,
            "-C",
            str(root),
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
        timeout=30,
    ).stdout.split("\0")
    files = []
    for name in sorted(set(names) - {""}):
        path = root / name
        if path.is_symlink():
            content = os.readlink(path).encode()
            files.append(
                {"path": name, "symlink_sha256": hashlib.sha256(content).hexdigest()}
            )
        elif path.is_file():
            files.append(
                {
                    "path": name,
                    "sha256": file_hash(path),
                    "mode": path.stat().st_mode & 0o777,
                }
            )
        elif not path.exists():
            files.append({"path": name, "deleted": True})
        else:
            raise RuntimeError("source manifest contains an unsupported non-file entry")
    return {"root": str(root), "head": head, "files": files}


def temperatures():
    values = {}
    for name in sorted(policy.HWMON.glob("*/name")):
        if name.read_text().strip() != "k10temp":
            continue
        for label in sorted(name.parent.glob("temp*_label")):
            if label.read_text().strip() == "Tctl":
                sensor = label.with_name(label.name.replace("_label", "_input"))
                values[str(sensor)] = int(sensor.read_text().strip())
    if not values or any(value <= 0 or value > 150000 for value in values.values()):
        raise RuntimeError("readable k10temp Tctl CPU sensors are required")
    return values


def check_temperature(sample):
    if any(
        value >= policy.TEMPERATURE_CUTOFF
        for value in sample["cpu_tctl_millidegrees"].values()
    ):
        raise RuntimeError(
            "CPU Tctl reached the operational 85 C cutoff (not a hardware limit)"
        )


def host_sample():
    started_ns = time.monotonic_ns()
    meminfo = {}
    for line in (policy.PROC / "meminfo").read_text().splitlines():
        key, value = line.split(":", 1)
        if key in {"MemAvailable", "SwapTotal", "SwapFree"}:
            meminfo[key] = int(value.split()[0]) * 1024
    if len(meminfo) != 3 or any(value < 0 for value in meminfo.values()):
        raise RuntimeError("host memory telemetry is unavailable")
    psi = {
        kind: (policy.PROC / "pressure" / kind).read_text().strip()
        for kind in ("cpu", "memory", "io")
    }
    for value in psi.values():
        if not value or any(
            not re.fullmatch(
                r"(some|full) avg10=\d+\.\d+ avg60=\d+\.\d+ avg300=\d+\.\d+ total=\d+",
                line,
            )
            for line in value.splitlines()
        ):
            raise RuntimeError("host PSI telemetry is invalid")
    return {
        "time_ns": time.time_ns(),
        "monotonic_ns": started_ns,
        "host_bytes": meminfo,
        "host_psi": psi,
        "cpu_tctl_millidegrees": temperatures(),
    }


def cgroup_sample(group):
    values = {
        name: (group / name).read_text().strip()
        for name in (
            "memory.current",
            "memory.peak",
            "memory.events",
            "memory.swap.current",
            "memory.swap.events",
            "cpu.stat",
            "pids.current",
        )
    }
    for key, value in values.items():
        if key in {"memory.events", "memory.swap.events", "cpu.stat"}:
            required = {
                "cpu.stat": {"usage_usec", "nr_throttled", "throttled_usec"},
                "memory.events": {"oom", "oom_kill"},
                "memory.swap.events": {"max", "fail"},
            }[key]
            counters = dict(line.split() for line in value.splitlines())
            if not required <= counters.keys() or any(
                not count.isdigit() for count in counters.values()
            ):
                raise RuntimeError(f"cgroup telemetry {key} is invalid")
        elif not value.isdigit():
            raise RuntimeError(f"cgroup telemetry {key} is invalid")
    return values


def append_sample(path, sample, log):
    with path.open("a") as output:
        output.write(json.dumps(sample, sort_keys=True) + "\n")
        output.flush()
        os.fsync(output.fileno())
    # systemd writes directly to this file; no supervisor pipe can block Cargo.
    os.fsync(log.fileno())


def finish(directory, result):
    result["finished_ns"] = time.time_ns()
    result["sha256"] = {
        path.name: file_hash(path)
        for path in sorted(directory.iterdir())
        if path.is_file() and path.name not in {"result.json", "result.sha256"}
    }
    save(directory / "result.json", result)
    with (directory / "result.sha256").open("x") as output:
        output.write(file_hash(directory / "result.json") + "  result.json\n")
        output.flush()
        os.fsync(output.fileno())
    sync_directory(directory)
