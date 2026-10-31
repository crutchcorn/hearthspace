"""Fake external commands/kernel files; never creates a real service or compiles."""

import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[2]
VALUES = {
    "cpu.max": "50000 100000",
    "cpu.weight": "25",
    "memory.high": "12884901888",
    "memory.max": "17179869184",
    "memory.swap.max": "0",
    "memory.oom.group": "1",
    "pids.max": "256",
    "memory.current": "1024",
    "memory.peak": "2048",
    "memory.events": "oom 0\noom_kill 0",
    "memory.swap.current": "0",
    "memory.swap.events": "max 0\nfail 0",
    "cpu.stat": "usage_usec 100\nnr_throttled 1\nthrottled_usec 50",
    "pids.current": "1",
    "cgroup.events": "populated 1",
    "cgroup.procs": "123",
}
PROPERTIES = {
    "Type": "exec",
    "ExitType": "cgroup",
    "Slice": "app.slice",
    "CPUQuotaPerSecUSec": "500ms",
    "CPUWeight": "25",
    "Nice": "10",
    "MemoryHigh": "12884901888",
    "MemoryMax": "17179869184",
    "MemorySwapMax": "0",
    "TasksMax": "256",
    "OOMPolicy": "kill",
    "KillMode": "control-group",
    "TimeoutStopUSec": "10s",
    "TimeoutStartUSec": "20s",
    "RuntimeMaxUSec": "45min",
    "NotifyAccess": "all",
    "WatchdogUSec": "10s",
    "WatchdogSignal": "9",
    "Restart": "no",
    "SendSIGKILL": "yes",
    "RemainAfterExit": "yes",
    "Delegate": "no",
    "LoadState": "loaded",
    "ActiveState": "active",
    "SubState": "running",
    "ExecMainCode": "0",
    "ExecMainStatus": "0",
    "Result": "success",
    "ControlPID": "0",
    "Job": "",
}
REQUESTED = {
    key: value
    for key, value in PROPERTIES.items()
    if key
    not in {
        "CPUQuotaPerSecUSec",
        "MemoryHigh",
        "MemoryMax",
        "TimeoutStopUSec",
        "TimeoutStartUSec",
        "RuntimeMaxUSec",
        "WatchdogUSec",
        "WatchdogSignal",
        "LoadState",
        "ActiveState",
        "SubState",
        "ExecMainCode",
        "ExecMainStatus",
        "Result",
        "ControlPID",
        "Job",
    }
} | {
    "CPUQuota": "50%",
    "MemoryHigh": "12G",
    "MemoryMax": "16G",
    "TimeoutStopSec": "10",
    "TimeoutStartSec": "20",
    "RuntimeMaxSec": "45min",
    "WatchdogSec": "10",
    "WatchdogSignal": "SIGKILL",
}


def write_json(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value))
    temporary.replace(path)


def boundaries(root, worker=False):
    sys.path.insert(0, str(SCRIPTS))
    import guarded_cargo_policy as policy

    policy.CGROUP = root / "cgroup"
    policy.PROC = root / ("proc-worker" if worker else "proc")
    policy.HWMON = root / "hwmon"
    policy.RUNTIME = root / "runtime"
    if worker:
        # External kernel syscall boundary; no host priority is changed.
        policy.os.getpriority = lambda *_: 10


def worker_driver(root):
    boundaries(root, worker=True)
    import guarded_cargo

    state = json.loads(Path(sys.argv[-1]).read_text())
    if not (root / "skip-affinity").exists():
        # Apply the submitted exec property to this fixture-owned process only.
        os.sched_setaffinity(0, {state["cpu_affinity"]})
    deadline = time.monotonic() + 3
    while not (root / "process.json").exists():
        if time.monotonic() >= deadline:
            raise RuntimeError("fixture process identity was not recorded")
        time.sleep(0.01)
    raise SystemExit(guarded_cargo.main(sys.argv[2:]))


def systemd_run(root):
    state = json.loads(Path(sys.argv[-1]).read_text())
    submitted = dict(
        argument.removeprefix("--property=").split("=", 1)
        for argument in sys.argv
        if argument.startswith("--property=")
    )
    expected = REQUESTED | {
        "CPUAffinity": str(state["cpu_affinity"]),
        "Description": state["owner"],
        "WorkingDirectory": state["cwd"],
        "StandardInput": "null",
        "StandardOutput": "append:" + str(Path(state["evidence"]) / "build.log"),
        "StandardError": "inherit",
    }
    if submitted != expected:
        raise RuntimeError(
            "fixture requires the exact resource and lifecycle launch properties"
        )
    write_json(root / "launch.json", sys.argv[1:])
    write_json(root / "state.json", state)
    (root / "supervisor.pid").write_text(str(os.getppid()))
    group = root / "cgroup/app.slice" / state["unit"]
    shutil.copytree(root / "template", group)
    (root / "proc-worker/self/cgroup").write_text(
        "0::/app.slice/" + state["unit"] + "\n"
    )
    environment = {"UNRELATED_MANAGER_SECRET": "must-not-reach-cargo"}
    for argument in sys.argv:
        if argument.startswith("--setenv="):
            key, value = argument[len("--setenv=") :].split("=", 1)
            environment[key] = value
    with (Path(state["evidence"]) / "build.log").open("ab") as log:
        subprocess.Popen(
            [sys.executable, "-I", __file__, str(root), "--keeper", sys.argv[-1]],
            stdin=subprocess.DEVNULL,
            stdout=log,
            stderr=log,
            env=environment,
            start_new_session=True,
        )
    deadline = time.monotonic() + 3
    while not (root / "process.json").exists():
        if time.monotonic() >= deadline:
            raise RuntimeError("fixture worker did not start")
        time.sleep(0.01)
    if (root / "heat-on-launch").exists():
        (root / "hwmon/cpu/temp1_input").write_text("85000\n")
    if (root / "interrupt-on-launch").exists():
        os.kill(os.getppid(), signal.SIGINT)


def keeper(root):
    # This is only a wait()/notification sink, not a systemd/limit emulator.
    with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as notify:
        notify.bind(str(root / "notify.sock"))
        notify.settimeout(0.1)
        process = subprocess.Popen(
            [sys.executable, "-I", __file__, str(root), "--worker", sys.argv[-1]],
            env=os.environ
            | {"NOTIFY_SOCKET": str(root / "notify.sock"), "WATCHDOG_USEC": "10000000"},
        )
        start = (
            Path(f"/proc/{process.pid}/stat").read_text().rsplit(")", 1)[1].split()[19]
        )
        write_json(root / "process.json", {"pid": process.pid, "start": start})
        while process.poll() is None:
            try:
                notify.recv(256)
            except TimeoutError:
                pass
        write_json(root / "exit.json", {"returncode": process.wait()})


def systemctl(root):
    with (root / "control.log").open("a") as output:
        output.write(json.dumps(sys.argv[1:]) + "\n")
    if "--property=ServiceWatchdogs" in sys.argv:
        print("ServiceWatchdogs=yes")
        return
    if "app.slice" in sys.argv:
        print("Id=app.slice\nLoadState=loaded\nControlGroup=/app.slice")
        return
    if not (root / "state.json").exists():
        print("LoadState=not-found")
        return
    state = json.loads((root / "state.json").read_text())
    process = json.loads((root / "process.json").read_text())
    group = root / "cgroup/app.slice" / state["unit"]
    if (root / "absent-unit").exists() and (root / "exit.json").exists():
        shutil.rmtree(group, ignore_errors=True)
        print("LoadState=not-found")
        return
    if "stop" in sys.argv:
        try:
            fd = os.pidfd_open(process["pid"])
            try:
                current = (
                    Path(f"/proc/{process['pid']}/stat")
                    .read_text()
                    .rsplit(")", 1)[1]
                    .split()[19]
                )
                if current == process["start"]:
                    signal.pidfd_send_signal(fd, signal.SIGTERM)
            finally:
                os.close(fd)
        except (ProcessLookupError, FileNotFoundError):
            pass
        deadline = time.monotonic() + 2
        while not (root / "exit.json").exists():
            if time.monotonic() >= deadline:
                raise RuntimeError("fixture worker has not exited after stop")
            time.sleep(0.01)
        (root / "stopped").touch()
        if not (root / "keep-tasks").exists():
            (group / "cgroup.events").write_text("populated 0\n")
            (group / "cgroup.procs").write_text("")
        return
    if "reset-failed" in sys.argv:
        return
    properties = PROPERTIES | {
        "Id": state["unit"],
        "Description": state["owner"],
        "ControlGroup": "/app.slice/" + state["unit"],
        "MainPID": str(process["pid"]),
        "WorkingDirectory": state["cwd"],
        "CPUAffinity": str(state["cpu_affinity"]),
    }
    if (root / "properties.json").exists():
        properties.update(json.loads((root / "properties.json").read_text()))
    if (root / "exit.json").exists():
        code = json.loads((root / "exit.json").read_text())["returncode"]
        properties.update(
            ActiveState="failed" if code else "active",
            SubState="failed" if code else "exited",
            ExecMainCode="1" if code >= 0 else "2",
            ExecMainStatus=str(abs(code)),
            Result="exit-code" if code else "success",
            MainPID="0",
        )
    if (root / "stopped").exists():
        properties.update(ActiveState="inactive", SubState="dead", MainPID="0")
    print("\n".join(f"{key}={value}" for key, value in properties.items()))


def cargo(root):
    import fcntl

    if (root / "wrapper-probe").exists():
        sys.path.insert(0, str(Path(__file__).parent))
        from cargo_wrapper_probe import probe

        write_json(root / "wrapper-probe.json", probe(root, sys.argv[1:], os.environ))
    with (root / "runtime" / str(os.getuid()) / "hearthspace-guarded-cargo.lock").open(
        "r"
    ) as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            held = False
        except BlockingIOError:
            held = True
    state = json.loads((root / "state.json").read_text())
    directory = Path(state["evidence"])
    execution = json.loads((directory / "executing.json").read_text())
    relay_fds = Path(f"/proc/{execution['watchdog_pid']}/fd")
    relay_has_lock = False
    for fd in relay_fds.iterdir():
        try:
            relay_has_lock |= "hearthspace-guarded-cargo.lock" in os.readlink(fd)
        except FileNotFoundError:
            pass
    write_json(
        root / "cargo.json",
        {
            "argv": sys.argv[1:],
            "cpu_affinity": sorted(os.sched_getaffinity(0)),
            "environment": dict(os.environ),
            "lock_held": held,
            "ready": (directory / "monitor-ready.json").exists(),
            "relay_has_lock": relay_has_lock,
            "initial_heartbeat": json.loads(
                (directory / "monitor-heartbeat.json").read_text()
            ),
            "samples_before_exec": len(
                (directory / "telemetry.jsonl").read_text().splitlines()
            ),
        },
    )
    action = (
        (root / "post-exec").read_text().strip()
        if (root / "post-exec").exists()
        else ""
    )
    if action == "heat":
        (root / "hwmon/cpu/temp1_input").write_text("85000\n")
    elif action == "sensor":
        (root / "hwmon/cpu/temp1_input").unlink()
    elif action == "telemetry":
        (directory / "telemetry.jsonl").rename(
            directory / "telemetry-before-failure.jsonl"
        )
        (directory / "telemetry.jsonl").mkdir()
    elif action in {"SIGINT", "SIGHUP"}:
        os.kill(int((root / "supervisor.pid").read_text()), getattr(signal, action))
    # Fault cases remain alive for the monitor to stop; the bound also protects
    # the test runner if its own assertion fails before requesting cleanup.
    time.sleep(5 if action else 0.35)
    if action:
        raise SystemExit(97)
    print("fixture cargo completed", flush=True)


def main():
    name = Path(sys.argv[0]).name
    if name == "guarded_cargo.py":
        if sys.argv[2] == "--keeper":
            keeper(Path(sys.argv[1]))
            return
        worker_driver(Path(sys.argv[1]))
    root = Path(sys.argv[0]).parent.parent
    if name == "systemd-run":
        systemd_run(root)
    elif name == "systemctl":
        systemctl(root)
    elif name == "cargo":
        cargo(root)
    elif name == "git":
        if "--show-toplevel" in sys.argv:
            print(root / "source")
        elif "HEAD" in sys.argv:
            print("aba99007-fixture")
        else:
            sys.stdout.write("Cargo.toml\0")


if __name__ == "__main__":
    main()
