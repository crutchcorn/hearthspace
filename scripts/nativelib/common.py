"""Bounded subprocesses, state files, and control-socket readiness."""

import json
import os
import socket
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ENTRY = ROOT / "scripts/hearthspace-native"
STATE_ROOT = Path("/run/hearthspace-native")
BINARIES = (
    "hearthspace-session",
    "hearthspace-app-scope-guard",
    "hearthspace-compositor",
    "hearthspace-shell",
    "hearthspace-settings",
    "hearthspace-settingsd",
    "hearthspace-a11y",
    "hearthspace-notifications",
)
MAX_REPLY = 128 * 1024 * 1024


def run(args, timeout=15, check=True, **kwargs):
    result = subprocess.run(
        [str(arg) for arg in args],
        check=False,
        capture_output=True,
        text=True,
        timeout=timeout,
        **kwargs,
    )
    if check and result.returncode:
        raise RuntimeError(
            f"{' '.join(map(str, args))}: {result.stderr.strip() or result.stdout.strip()}"
        )
    return result


def load(path):
    return json.loads(Path(path).read_text())


def save(path, value, uid=None):
    path = Path(path)
    temp = path.with_suffix(f".tmp-{os.getpid()}")
    temp.write_text(json.dumps(value, indent=2) + "\n")
    temp.chmod(0o600)
    if uid is not None:
        os.chown(temp, uid, -1)
    temp.replace(path)


def active_vt():
    return int(
        Path("/sys/class/tty/tty0/active").read_text().strip().removeprefix("tty")
    )


def unit_info(unit, timeout=15):
    result = run(
        [
            "systemctl",
            "show",
            unit,
            "--property=ActiveState,SubState,MainPID,ExecMainPID,Result",
        ],
        timeout=timeout,
    )
    return dict(
        line.split("=", 1) for line in result.stdout.splitlines() if "=" in line
    )


def control(path, command, timeout):
    """Read the length-framed reply with one absolute operation budget."""
    deadline = time.monotonic() + timeout
    with socket.socket(socket.AF_UNIX) as stream:

        def remaining():
            budget = deadline - time.monotonic()
            if budget <= 0:
                raise TimeoutError(f"{command}: control operation timed out")
            stream.settimeout(budget)

        remaining()
        stream.connect(str(path))
        remaining()
        stream.sendall(command.encode() + b"\n")
        data = bytearray()
        while b"\n" not in data:
            remaining()
            chunk = stream.recv(1)
            if not chunk:
                raise RuntimeError("control socket disconnected before reply")
            data.extend(chunk)
            if len(data) > 4096:
                raise RuntimeError("oversized control reply header")
        header = bytes(data[:-1]).decode()
        if not header.startswith("ok "):
            raise RuntimeError(header)
        size = int(header[3:])
        if not 0 < size <= MAX_REPLY:
            raise RuntimeError("invalid control reply size")
        payload = bytearray()
        while len(payload) < size:
            remaining()
            chunk = stream.recv(min(size - len(payload), 65536))
            if not chunk:
                raise RuntimeError("control socket disconnected during reply")
            payload.extend(chunk)
        return bytes(payload)


def settings_ready(path, timeout):
    deadline = time.monotonic() + timeout
    with socket.socket(socket.AF_UNIX) as stream:

        def remaining():
            budget = deadline - time.monotonic()
            if budget <= 0:
                raise TimeoutError("settings readiness timed out")
            stream.settimeout(budget)

        remaining()
        stream.connect(str(path))
        remaining()
        stream.sendall(b"get\n")
        stream.shutdown(socket.SHUT_WR)
        response = bytearray()
        while b"\n" not in response:
            remaining()
            chunk = stream.recv(1024)
            if not chunk:
                raise RuntimeError("settings service disconnected before its reply")
            response.extend(chunk)
            if len(response) > 8192:
                raise RuntimeError("oversized settings reply")
        return response.startswith(b"ok ")


def wait_ready(state, timeout):
    deadline = time.monotonic() + timeout
    last_error = "services have not responded"

    def remaining(limit):
        budget = deadline - time.monotonic()
        if budget <= 0:
            raise TimeoutError("absolute native startup deadline exceeded")
        return min(limit, budget)

    while time.monotonic() < deadline:
        info = unit_info(state["unit"], timeout=remaining(1))
        if info.get("ActiveState") in ("failed", "inactive"):
            raise RuntimeError(f"native session exited during startup: {info}")
        try:
            status = json.loads(
                control(state["command_socket"], "status", remaining(1))
            )
            if status.get("backend") not in ("native", "udev", "tty"):
                raise RuntimeError("compositor did not select the native backend")
            if not status.get("shell_ready"):
                raise RuntimeError("shell has not presented a surface")
            if not settings_ready(state["settings_socket"], remaining(1)):
                raise RuntimeError("settings service is not responsive")
            capture = control(state["command_socket"], "capture", remaining(5))
            metadata, png = capture.split(b"\n", 1)
            metadata = json.loads(metadata)
            if (
                not png.startswith(b"\x89PNG\r\n\x1a\n")
                or not metadata.get("frame_id")
                or metadata.get("presentation") != "page-flip"
            ):
                raise RuntimeError("native capture has no successfully presented frame")
            if state.get("vnc"):
                # WayVNC starts after shell presentation. Its failure is optional:
                # return its recorded error without tearing down the physical desktop.
                state["wayvnc"] = load(Path(state["runtime_dir"]) / "wayvnc.json")
            session = Path(state["session_dir"])
            (session / "ready.png").write_bytes(png)
            save(session / "ready.json", metadata)
            state.update(
                status="ready",
                outputs=status["outputs"],
                capture=metadata,
                processes=load(Path(state["runtime_dir"]) / "processes.json"),
            )
            state["compositor_pid"] = state["processes"]["compositor"]
            bus_file = Path(state["runtime_dir"]) / "session-bus.txt"
            if bus_file.exists():
                state["dbus_session_bus_address"] = bus_file.read_text().strip()
            return state
        except (OSError, ValueError, RuntimeError) as error:
            last_error = str(error)
        time.sleep(min(0.1, max(0, deadline - time.monotonic())))
    raise TimeoutError(f"absolute native startup deadline exceeded: {last_error}")
