"""Fixed build policy and checks of the kernel's actual enforcement."""

import fcntl
import hashlib
import json
import os
import re
import stat
import subprocess
from pathlib import Path

CGROUP = Path("/sys/fs/cgroup")
PROC = Path("/proc")
HWMON = Path("/sys/class/hwmon")
RUNTIME = Path("/run/user")
INTERVAL = 1.0
TEMPERATURE_CUTOFF = 85000  # Operational Tctl policy, not a hardware limit.
PROPERTIES = {
    "Type": "exec",
    # Keep the runtime timer active until descendants have exited too.
    "ExitType": "cgroup",
    "Slice": "app.slice",
    "CPUQuota": "50%",
    "CPUWeight": "25",
    "Nice": "10",
    "MemoryHigh": "12G",
    "MemoryMax": "16G",
    "MemorySwapMax": "0",
    "TasksMax": "256",
    "OOMPolicy": "kill",
    "KillMode": "control-group",
    "TimeoutStopSec": "10",
    "TimeoutStartSec": "20",
    "RuntimeMaxSec": "45min",
    "NotifyAccess": "all",
    "WatchdogSec": "10",
    "WatchdogSignal": "SIGKILL",
    "Restart": "no",
    "SendSIGKILL": "yes",
    "RemainAfterExit": "yes",
    "Delegate": "no",
}
EXPECTED = {
    key: value
    for key, value in PROPERTIES.items()
    if key not in {"CPUQuota", "MemoryHigh", "MemoryMax"} and not key.endswith("Sec")
} | {
    "CPUQuotaPerSecUSec": "500ms",
    "MemoryHigh": str(12 * 1024**3),
    "MemoryMax": str(16 * 1024**3),
    "TimeoutStopUSec": "10s",
    "TimeoutStartUSec": "20s",
    "RuntimeMaxUSec": "45min",
    "WatchdogUSec": "10s",
    "WatchdogSignal": "9",
}
# Cargo's empty wrapper variables override configuration, including CLI config.
# Also override build.* and [env] at the end of the CLI so forced [env] entries
# cannot restore wrappers in children. Cargo owns config discovery and merging;
# incompatible config types fail closed. No daemon is queried or stopped.
# https://doc.rust-lang.org/cargo/reference/environment-variables.html
# https://doc.rust-lang.org/cargo/reference/config.html#command-line-overrides
WRAPPER_ENVIRONMENT = dict.fromkeys(
    (
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
    ),
    "",
)
WRAPPER_CONFIG = {
    "build.rustc-wrapper": "",
    "build.rustc-workspace-wrapper": "",
} | {
    f"env.{name}.{field}": value
    for name in WRAPPER_ENVIRONMENT
    # force=false preserves cargo-clippy's own local driver in its child Cargo.
    # relative=false prevents an empty path becoming a nonempty absolute path.
    for field, value in (("value", ""), ("force", False), ("relative", False))
}
WRAPPER_ARGUMENTS = [
    f"--config={key}={json.dumps(value)}" for key, value in WRAPPER_CONFIG.items()
]
ENVIRONMENT = {
    "HOME",
    "PATH",
    "USER",
    "LOGNAME",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TZ",
    "TMPDIR",
    "TMP",
    "TEMP",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "RUSTUP_TOOLCHAIN",
    "RUSTFLAGS",
    "RUSTDOCFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_ENCODED_RUSTDOCFLAGS",
    "CARGO_TARGET_DIR",
    "CARGO_BUILD_TARGET",
    "CARGO_BUILD_TARGET_DIR",
    "CARGO_INCREMENTAL",
    "RUSTC",
    "RUSTDOC",
    "CARGO_BUILD_RUSTC",
    "CARGO_BUILD_RUSTFLAGS",
    "CARGO_BUILD_RUSTDOCFLAGS",
    "CARGO_BUILD_INCREMENTAL",
    "CC",
    "CXX",
    "AR",
    "CFLAGS",
    "CXXFLAGS",
    "CPPFLAGS",
    "LDFLAGS",
    "PKG_CONFIG_PATH",
    "PKG_CONFIG_LIBDIR",
    "PKG_CONFIG_SYSROOT_DIR",
    "LLVM_CONFIG_PATH",
    "LIBCLANG_PATH",
    "CLANG_PATH",
}


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def build_environment(environ):
    if not environ.get("HOME") or not environ.get("PATH"):
        raise ValueError("HOME and PATH must be explicitly present")
    if environ.get("CARGO_BUILD_JOBS", "1") != "1":
        raise ValueError("CARGO_BUILD_JOBS contradicts the fixed -j1 policy")
    return {
        key: value
        for key, value in environ.items()
        if key in ENVIRONMENT
        or re.fullmatch(r"CARGO_PROFILE_[A-Z0-9_]+", key)
        or re.fullmatch(
            r"CARGO_TARGET_[A-Z0-9_]+_(RUSTFLAGS|RUSTDOCFLAGS|LINKER|RUNNER)", key
        )
    } | WRAPPER_ENVIRONMENT


def cargo_arguments(arguments):
    if not arguments or arguments[0] not in {"build", "check", "clippy"}:
        raise ValueError(
            "choose build, check, or clippy; execution/test commands are excluded"
        )
    output = [arguments[0], "-j1"]
    index = 1
    while index < len(arguments):
        arg = arguments[index]
        if arg == "--":
            break
        if arg in {"-j", "--jobs"}:
            index += 1
            if index == len(arguments) or arguments[index] != "1":
                raise ValueError("Cargo jobs must be 1")
        elif arg.startswith(("--jobs=", "-j")):
            if arg not in {"--jobs=1", "-j1"}:
                raise ValueError("Cargo jobs must be 1")
        elif arg.startswith("-") and not arg.startswith("--"):
            for option in arg[1:]:
                # These options consume the rest as their value, including j.
                # Cargo still validates option availability and placement.
                if option in "pFZC":
                    break
                if option == "j":
                    raise ValueError(
                        "use a separate -j1; combined jobs options are rejected"
                    )
            output.append(arg)
        else:
            output.append(arg)
        index += 1
    return output + WRAPPER_ARGUMENTS + arguments[index:]


def normal_user():
    if os.getuid() == 0 or os.geteuid() == 0 or os.getuid() != os.geteuid():
        raise RuntimeError("guarded-cargo requires an unprivileged normal user")


def command(argv, *, timeout=5, check=True):
    env = {
        key: os.environ[key]
        for key in ("HOME", "PATH", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS")
        if key in os.environ
    }
    try:
        result = subprocess.run(
            argv,
            env=env | {"LC_ALL": "C", "SYSTEMD_COLORS": "0"},
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired:
        raise RuntimeError(
            f"{Path(argv[0]).name} exceeded its {timeout}s deadline"
        ) from None
    if check and result.returncode:
        # Commands may contain deliberately forwarded environment values.
        raise RuntimeError(f"{Path(argv[0]).name} failed (exit {result.returncode})")
    return result


def show(systemctl, unit, *, timeout=5):
    keys = list(EXPECTED) + [
        "Id",
        "Description",
        "ControlGroup",
        "LoadState",
        "ActiveState",
        "SubState",
        "WorkingDirectory",
        "CPUAffinity",
        "MainPID",
        "ControlPID",
        "Job",
        "ExecMainCode",
        "ExecMainStatus",
        "Result",
        "MemoryPeak",
        "CPUUsageNSec",
    ]
    result = command(
        [systemctl, "--user", "show", "--all", unit, "--property=" + ",".join(keys)],
        timeout=timeout,
        check=False,
    )
    properties = dict(
        line.split("=", 1) for line in result.stdout.splitlines() if "=" in line
    )
    if result.returncode and properties.get("LoadState") != "not-found":
        raise RuntimeError("cannot inspect the user service")
    return properties


def cgroup_path(value):
    if not value.startswith("/") or ".." in Path(value).parts or value == "/":
        raise RuntimeError("missing or invalid cgroup v2 path")
    path = CGROUP / value.lstrip("/")
    if not path.resolve().is_relative_to(CGROUP.resolve()):
        raise RuntimeError("cgroup path escapes the unified hierarchy")
    return path


def self_cgroup():
    lines = (PROC / "self/cgroup").read_text().splitlines()
    if len(lines) != 1 or not lines[0].startswith("0::"):
        raise RuntimeError("a unified cgroup v2 hierarchy is required")
    return cgroup_path(lines[0][3:])


def require_controllers(path):
    available = set((path / "cgroup.controllers").read_text().split())
    if not {"cpu", "memory", "pids"} <= available:
        raise RuntimeError("user cgroup cpu, memory, and pids controllers are required")


def own_unit(state, properties):
    if (
        properties.get("Id") != state["unit"]
        or properties.get("Description") != state["owner"]
    ):
        raise RuntimeError("unit identity does not match this invocation")


def verify_worker(state):
    normal_user()
    manager = command(
        [state["systemctl"], "--user", "show", "--property=ServiceWatchdogs"]
    ).stdout.strip()
    if manager != "ServiceWatchdogs=yes":
        raise RuntimeError("the user service manager has disabled watchdog enforcement")
    properties = show(state["systemctl"], state["unit"])
    own_unit(state, properties)
    for key, expected in EXPECTED.items():
        if properties.get(key) != expected:
            raise RuntimeError(f"service property {key} is not enforced")
    group = self_cgroup()
    expected_group = cgroup_path(state["slice_cgroup"]) / state["unit"]
    if group != expected_group or group != cgroup_path(
        properties.get("ControlGroup", "")
    ):
        raise RuntimeError("worker is outside its dedicated service cgroup")
    if (
        properties.get("MainPID") != str(os.getpid())
        or os.getpriority(os.PRIO_PROCESS, 0) != 10
    ):
        raise RuntimeError("worker PID or nice value is not enforced")
    if (
        properties.get("WorkingDirectory") != state["cwd"]
        or str(Path.cwd()) != state["cwd"]
    ):
        raise RuntimeError("worker working directory changed")
    affinity = sorted(os.sched_getaffinity(0))
    if properties.get("CPUAffinity") != str(state["cpu_affinity"]) or affinity != [
        state["cpu_affinity"]
    ]:
        raise RuntimeError("worker CPU affinity is not enforced")
    values = {}
    for name, expected in {
        "cpu.weight": "25",
        "memory.high": str(12 * 1024**3),
        "memory.max": str(16 * 1024**3),
        "memory.swap.max": "0",
        "memory.oom.group": "1",
        "pids.max": "256",
    }.items():
        values[name] = (group / name).read_text().strip()
        if values[name] != expected:
            raise RuntimeError(f"kernel cgroup value {name} is not enforced")
    values["cpu.max"] = (group / "cpu.max").read_text().strip()
    quota, period = values["cpu.max"].split()
    if quota == "max" or int(period) <= 0 or 2 * int(quota) != int(period):
        raise RuntimeError("kernel CPU quota is not 50%")
    return {
        "cgroup": str(group),
        "values": values,
        "properties": properties,
        "cpu_affinity": affinity,
    }


def acquire_lock():
    directory = RUNTIME / str(os.getuid())
    info = directory.stat()
    if info.st_uid != os.getuid() or stat.S_IMODE(info.st_mode) & 0o077:
        raise RuntimeError("the user's private runtime directory is required")
    fd = os.open(
        directory / "hearthspace-guarded-cargo.lock",
        os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW,
        0o600,
    )
    try:
        info = os.fstat(fd)
        if (
            info.st_uid != os.getuid()
            or not stat.S_ISREG(info.st_mode)
            or info.st_nlink != 1
        ):
            raise RuntimeError("unsafe guarded-cargo lock file")
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        # The lock survives supervisor death and exec; never unlink this inode.
        os.set_inheritable(fd, True)
        return fd
    except BaseException:
        os.close(fd)
        raise


def empty_cgroup(group):
    if not group.exists():
        return {"cgroup_removed": True}
    events = dict(
        line.split() for line in (group / "cgroup.events").read_text().splitlines()
    )
    remaining = {}
    checked = 0
    for path in group.rglob("cgroup.procs"):
        try:
            pids = path.read_text().split()
        except FileNotFoundError:
            continue  # A removed cgroup cannot retain tasks.
        checked += 1
        if pids:
            remaining[str(path.relative_to(group))] = pids
    return {
        "cgroup_populated": events.get("populated"),
        "remaining_tasks": remaining,
        "cgroups_checked": checked,
    }
