"""Independent recovery runs as systemd ExecStopPost, including after SIGKILL."""

import fcntl
import os
import shutil
import signal
import subprocess
import termios
import time
from pathlib import Path

from .common import STATE_ROOT, active_vt, load, run, save, unit_info
from .portal_cleanup import cleanup_documents
from .service import allocation_lock


def read_state(path):
    path = Path(path).resolve()
    if path.parent != STATE_ROOT or path.stat().st_uid != 0:
        raise RuntimeError("recovery requires a root-owned harness state file")
    return load(path)


def budget(deadline, maximum=1):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise TimeoutError("independent native recovery deadline exceeded")
    return min(remaining, maximum)


def owned_sessions(state, deadline=None):
    """PAM children may migrate to a session scope outside the service cgroup."""
    deadline = deadline or time.monotonic() + 3
    sessions = []
    try:
        identity = load(Path(state["session_dir"]) / "identity.json")
    except FileNotFoundError:
        identity = None
    # ExecMainPID remains available after main-process death, including failure
    # between PAM opening the session and our wrapper writing identity.json.
    expected_leader = (
        str(identity["leader"])
        if identity
        else unit_info(state["unit"], timeout=budget(deadline)).get("ExecMainPID", "0")
    )
    for line in run(
        ["loginctl", "list-sessions", "--no-legend"], timeout=budget(deadline)
    ).stdout.splitlines():
        session_id = line.split()[0]
        result = run(
            [
                "loginctl",
                "show-session",
                session_id,
                "--property=User",
                "--property=TTY",
                "--property=Leader",
            ],
            check=False,
            timeout=budget(deadline),
        )
        if result.returncode:
            scope = unit_info(f"session-{session_id}.scope", timeout=budget(deadline))
            if scope.get("ActiveState") not in ("inactive", "failed"):
                raise RuntimeError(
                    f"cannot verify logind session {session_id}: {result.stderr}"
                )
            continue
        props = dict(
            item.split("=", 1) for item in result.stdout.splitlines() if "=" in item
        )
        if not {"User", "TTY", "Leader"}.issubset(props):
            raise RuntimeError(
                f"logind omitted identity fields for session {session_id}"
            )
        same_identity = not identity or identity["session_id"] == session_id
        if (
            same_identity
            and expected_leader != "0"
            and props.get("Leader") == expected_leader
            and props.get("TTY") == f"tty{state['vt']}"
            and props.get("User") == str(state["uid"])
        ):
            sessions.append(session_id)
    return sessions


def detach_recovery_tty(state):
    # ExecStopPost inherits StandardInput=tty-fail. Its own fd/session would
    # otherwise keep the spare console busy during VT_DISALLOCATE.
    if not os.isatty(0) or os.ttyname(0) != f"/dev/tty{state['vt']}":
        return
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    with open("/dev/tty", "rb", buffering=0) as controlling_tty:
        fcntl.ioctl(controlling_tty, termios.TIOCNOTTY)
    with open("/dev/null", "rb", buffering=0) as null:
        os.dup2(null.fileno(), 0)


def recover(state_file, lock=True):
    if os.geteuid() != 0:
        raise RuntimeError("recovery requires root")
    if lock:
        with allocation_lock():
            return recover(state_file, lock=False)
    state = read_state(state_file)
    if state.get("cleanup", {}).get("complete"):
        return state
    errors = []
    deadline = time.monotonic() + 8
    try:
        detach_recovery_tty(state)
    except OSError as error:
        errors.append(str(error))
    cleanup = {"complete": False}
    try:
        run(["chvt", state["original_vt"]], timeout=budget(deadline, 3))
        cleanup["vt_restored"] = active_vt() == state["original_vt"]
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
        errors.append(str(error))
    try:
        for session_id in owned_sessions(state, deadline):
            run(
                ["loginctl", "terminate-session", session_id],
                timeout=budget(deadline),
                check=False,
            )
            # Force all descendants out even if an app ignores SIGTERM or setsid().
            run(
                [
                    "systemctl",
                    "kill",
                    "--kill-whom=all",
                    "--signal=KILL",
                    f"session-{session_id}.scope",
                ],
                timeout=budget(deadline),
                check=False,
            )
        while owned_sessions(state, deadline) and time.monotonic() < deadline:
            time.sleep(0.1)
        cleanup["session_scope_stopped"] = not owned_sessions(state, deadline)
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
        errors.append(str(error))
    runtime = Path(state["runtime_dir"])
    if cleanup.get("session_scope_stopped"):
        try:
            if (
                runtime.parent != Path(f"/run/user/{state['uid']}")
                or runtime.name != f"hearthspace-native-{state['id']}"
            ):
                raise RuntimeError("refusing unexpected runtime directory")
            cleanup_documents(runtime, timeout=budget(deadline, 2))
            shutil.rmtree(runtime, ignore_errors=False) if runtime.exists() else None
            cleanup["runtime_removed"] = not runtime.exists()
            with open("/dev/tty0", "rb", buffering=0) as tty:
                fcntl.ioctl(
                    tty, 0x5608, state["vt"]
                )  # VT_DISALLOCATE; never the original VT.
        except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
            errors.append(str(error))
    cleanup["complete"] = not errors and all(
        cleanup.get(key)
        for key in ("vt_restored", "session_scope_stopped", "runtime_removed")
    )
    cleanup["errors"] = errors
    state.update(
        status="stopped" if cleanup["complete"] else "cleanup_failed",
        cleanup=cleanup,
        stopped_at=time.time(),
    )
    manifest = Path(state["session_dir"]) / "session.json"
    try:
        retained = load(manifest)
    except (OSError, ValueError):
        retained = {}
    retained.update(state)
    save(manifest, retained, state["uid"])
    save(state_file, state)
    Path(state_file).chmod(0o644)
    save(STATE_ROOT / "current.json", state)
    return retained


def stop(state_file):
    state = read_state(state_file)
    uid = int(os.environ.get("SUDO_UID", "0"))
    if uid not in (0, state["uid"]):
        raise RuntimeError("the native session belongs to another user")
    if unit_info(state["unit"]).get("ActiveState") in (
        "active",
        "activating",
        "deactivating",
    ):
        run(["systemctl", "stop", state["unit"]], timeout=30, check=False)
    result = recover(state_file)
    if not result["cleanup"]["complete"]:
        raise RuntimeError(f"native recovery is incomplete: {result['cleanup']}")
    return result
