"""Privileged VT allocation and transient PAM/logind service definition."""

import array
import fcntl
import os
import pwd
import shlex
import time
import uuid
from contextlib import contextmanager
from pathlib import Path

from .common import ENTRY, ROOT, STATE_ROOT, active_vt, load, run, save, unit_info


@contextmanager
def allocation_lock():
    STATE_ROOT.mkdir(mode=0o755, exist_ok=True)
    with (STATE_ROOT / "allocation.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield


def choose_vt():
    """Only unoccupied kernel VTs beyond logind's usual automatic gettys."""
    occupied = set()
    sessions = run(["loginctl", "list-sessions", "--no-legend"]).stdout.splitlines()
    for line in sessions:
        for item in line.split():
            if item.startswith("tty") and item[3:].isdigit():
                occupied.add(int(item[3:]))
    # VT_GETSTATE identifies open VTs, even if they have no logind session.
    # The kernel ABI has a 16-bit occupancy mask; stay inside that range.
    with open("/dev/tty0", "rb", buffering=0) as tty:
        state = array.array("H", [0, 0, 0])
        fcntl.ioctl(tty, 0x5603, state, True)
    occupied.add(state[0])
    for vt in range(7, 16):
        if vt not in occupied and not state[2] & (1 << vt):
            return vt
    raise RuntimeError("no unoccupied VT is available in tty7–tty15")


def property_command(args):
    # systemd performs its own specifier/environment expansion after tokenizing.
    return " ".join(
        shlex.quote(str(arg).replace("%", "%%").replace("$", "$$")) for arg in args
    )


def service_command(state):
    properties = {
        "Type": "exec",
        "User": str(state["uid"]),
        "PAMName": "login",
        "StandardInput": "tty-fail",
        "TTYPath": f"/dev/tty{state['vt']}",
        "TTYReset": "yes",
        "KillMode": "control-group",
        "SendSIGKILL": "yes",
        "TimeoutStopSec": "12",
        "TimeoutStartSec": "15",
        "RuntimeMaxSec": str(state["watchdog"]),
        "Restart": "no",
        "StandardOutput": f"append:{state['session_dir']}/session.log",
        "StandardError": f"append:{state['session_dir']}/session.log",
        "WorkingDirectory": str(ROOT),
        "UnsetEnvironment": "DISPLAY WAYLAND_DISPLAY WAYLAND_SOCKET DBUS_SESSION_BUS_ADDRESS DBUS_SESSION_BUS_PID",
        "ExecStartPre": "+" + property_command(["/usr/bin/chvt", state["vt"]]),
        "ExecStopPost": "+"
        + property_command(
            ["/usr/bin/python3", ENTRY, "_recover", "--state", state["state_file"]]
        ),
    }
    args = ["systemd-run", "--quiet", "--no-block", f"--unit={state['unit']}"]
    args.extend(f"--property={key}={value}" for key, value in properties.items())
    for key, value in {
        "XDG_SESSION_TYPE": "wayland",
        "XDG_SESSION_DESKTOP": "Hearthspace",
        "XDG_SEAT": "seat0",
        "XDG_VTNR": state["vt"],
        "LIBSEAT_BACKEND": "logind",
        "PATH": f"{state['binary_dir']}:/usr/local/bin:/usr/bin:/bin",
        "HOME": state["home"],
        "LANG": "C.UTF-8",
    }.items():
        args.append(f"--setenv={key}={value}")
    args.extend(["/usr/bin/python3", ENTRY, "_exec", "--state", state["state_file"]])
    return args


def launch(args):
    if os.geteuid() != 0:
        raise RuntimeError("native service setup requires sudo")
    uid = int(os.environ.get("SUDO_UID", "0"))
    if uid == 0:
        raise RuntimeError("start the harness as the desktop user, not root")
    user = pwd.getpwuid(uid)
    session = Path(args.session).resolve()
    if session.stat().st_uid != uid or session.is_symlink():
        raise RuntimeError("artifact directory must belong to the desktop user")
    with allocation_lock():
        current = STATE_ROOT / "current.json"
        if current.exists():
            previous = load(current)
            state = unit_info(previous["unit"]).get("ActiveState")
            recently_submitted = (
                previous.get("status") == "starting"
                and time.time() - previous["started_at"] < 30
            )
            if state in ("active", "activating", "deactivating") or recently_submitted:
                raise RuntimeError(
                    f"a native harness already owns the seat: {previous['session_dir']}"
                )
            if not previous.get("cleanup", {}).get("complete"):
                from .recovery import recover

                recovered = recover(previous["state_file"], lock=False)
                if not recovered.get("cleanup", {}).get("complete"):
                    raise RuntimeError(
                        "previous native session recovery is incomplete; refusing to claim another VT"
                    )
        identity = uuid.uuid4().hex[:12]
        runtime = Path(f"/run/user/{uid}/hearthspace-native-{identity}")
        runtime.mkdir(mode=0o700)
        os.chown(runtime, uid, user.pw_gid)
        state_file = STATE_ROOT / f"{identity}.json"
        state = {
            "version": 1,
            "id": identity,
            "status": "starting",
            "uid": uid,
            "home": user.pw_dir,
            "unit": f"hearthspace-native-{identity}.service",
            "session_dir": str(session),
            "runtime_dir": str(runtime),
            "command_socket": str(runtime / "shell-command.sock"),
            "wayland_display": "wayland-99",
            "settings_socket": str(runtime / "settings.sock"),
            "state_file": str(state_file),
            "original_vt": active_vt(),
            "vt": choose_vt(),
            "watchdog": args.watchdog,
            "startup_timeout": args.timeout,
            "binary_dir": str(Path(args.binary_dir).resolve()),
            "started_at": time.time(),
            "compositor": str(args.compositor.resolve()) if args.compositor else None,
        }
        save(state_file, state)
        # User wrapper needs only read access; privileged recovery always reads this copy.
        state_file.chmod(0o644)
        save(current, state)
        save(session / "session.json", state, uid)
        (session / "session.log").touch(mode=0o600)
        os.chown(session / "session.log", uid, user.pw_gid)
        try:
            run(service_command(state))
        except BaseException:
            from .recovery import recover

            recover(state_file, lock=False)
            raise
    return state


def exec_session(state_file):
    state = load(state_file)
    session_id = os.environ.get("XDG_SESSION_ID")
    if not session_id:
        raise RuntimeError(
            "PAM did not create a logind session; check pam_systemd in common-session"
        )
    save(
        Path(state["session_dir"]) / "identity.json",
        {
            "session_id": session_id,
            "leader": os.getpid(),
            "uid": os.getuid(),
            "vt": state["vt"],
        },
    )
    env = dict(os.environ)
    for key in (
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "WAYLAND_SOCKET",
        "DBUS_SESSION_BUS_ADDRESS",
        "DBUS_SESSION_BUS_PID",
    ):
        env.pop(key, None)
    # D-Bus-activated portals/accessibility services inherit the daemon's env,
    # so isolate it before dbus-run-session, not only in supervisor children.
    env["HEARTHSPACE_HOST_XDG_RUNTIME_DIR"] = f"/run/user/{state['uid']}"
    env["XDG_RUNTIME_DIR"] = state["runtime_dir"]
    env["WAYLAND_DISPLAY"] = state["wayland_display"]
    env["XDG_CURRENT_DESKTOP"] = "Hearthspace"
    supervisor = str(Path(state["binary_dir"]) / "hearthspace-session")
    args = [
        "/usr/bin/dbus-run-session",
        "--",
        supervisor,
        "--installed",
        "--backend",
        "native",
        "--session-dir",
        state["runtime_dir"],
        "--binary-dir",
        state["binary_dir"],
        "--startup-timeout",
        str(max(1, int(state["startup_timeout"]))),
        "--no-watch-apps",
        "--private-session-bus",
    ]
    if state.get("compositor"):
        args.extend(["--compositor-binary", state["compositor"]])
    os.execve(args[0], args, env)
