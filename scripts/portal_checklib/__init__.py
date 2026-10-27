"""Isolation and bounded cleanup for the disposable portal backend check."""

import logging
import os
import signal
import subprocess
import time
from pathlib import Path

LOGGER = logging.getLogger(__name__)


def fixture_environment(inherited, directory):
    env = {
        key: value
        for key, value in inherited.items()
        if not key.startswith(("HEARTHSPACE_", "PIPEWIRE_", "DBUS_"))
        and key not in ("DISPLAY", "WAYLAND_SOCKET", "AT_SPI_BUS_ADDRESS")
    }
    # PipeWire 1.6.2 src/modules/module-rt.c:pw_rtkit_bus_get checks this
    # before opening either bus; pipewire(1) also documents DISABLE_RTKIT.
    # A nonexistent socket also blocks every child's system-bus fallback.
    env.update(
        XDG_RUNTIME_DIR=str(directory),
        HOME=str(directory),
        XDG_CONFIG_HOME=str(directory / "config"),
        XDG_DATA_HOME=str(directory / "data"),
        XDG_CACHE_HOME=str(directory / "cache"),
        XDG_STATE_HOME=str(directory / "state"),
        DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(directory / "no-system-bus"),
        DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(directory / "no-session-bus"),
        DISABLE_RTKIT="1",
        PIPEWIRE_RUNTIME_DIR=str(directory),
        PIPEWIRE_REMOTE="pipewire-0",
        PIPEWIRE_CORE="pipewire-0",
        WAYLAND_DISPLAY="policy-wayland",
        HEARTHSPACE_WAYLAND_DISPLAY="policy-wayland",
        HEARTHSPACE_COMMAND_SOCKET=str(directory / "command.sock"),
        LIBGL_ALWAYS_SOFTWARE="1",
        VK_DRIVER_FILES="/usr/share/vulkan/icd.d/lvp_icd.json",
        XDPW_PERSIST_MODE="permanent",
    )
    return env


class OwnedProcess:
    """Keep the session leader unreaped until its process group is drained.

    WNOWAIT pins the leader PID even after exit, preventing PGID reuse while
    descendants still need signalling. Callers must not reap the leader.
    """

    def __init__(self, command, **kwargs):
        self._child = subprocess.Popen(command, start_new_session=True, **kwargs)
        self.pid = self._child.pid
        self.stdout = self._child.stdout
        self._stopped = False

    def poll(self):
        if self._stopped:
            return self._child.returncode
        status = os.waitid(os.P_PID, self.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
        if status is None:
            return None
        if status.si_code == os.CLD_EXITED:
            return status.si_status
        return -status.si_status

    def _live_members(self):
        members = []
        for entry in Path("/proc").iterdir():
            if not entry.name.isdigit():
                continue
            try:
                fields = (entry / "stat").read_text().rsplit(")", 1)[1].split()
            except (FileNotFoundError, ProcessLookupError):
                continue
            # Zombies cannot execute or retain sockets; their parent reaps them.
            if (
                fields[0] not in ("Z", "X")
                and int(fields[2]) == self.pid
                and int(fields[3]) == self.pid
            ):
                members.append(int(entry.name))
        return members

    def _wait_empty(self, timeout):
        deadline = time.monotonic() + timeout
        while self._live_members():
            if time.monotonic() >= deadline:
                return False
            time.sleep(0.025)
        return True

    def stop(self, term_timeout=3, kill_timeout=3):
        if self._stopped:
            return
        # Unlike Popen.poll(), this verifies ownership without reaping the PID.
        self.poll()
        for signum, timeout in (
            (signal.SIGTERM, term_timeout),
            (signal.SIGKILL, kill_timeout),
        ):
            try:
                os.killpg(self.pid, signum)
            except ProcessLookupError:
                pass
            if self._wait_empty(timeout):
                self._child.wait(timeout=kill_timeout)
                self._stopped = True
                return
        raise TimeoutError(
            f"owned process group {self.pid} still has live members: "
            f"{self._live_members()}"
        )


def cleanup(actions, report):
    """Attempt every cleanup action; incomplete cleanup must fail the check."""
    errors = []
    for label, action in actions:
        try:
            action()
        except Exception as error:
            LOGGER.exception("portal check cleanup failed: %s", label)
            errors.append(f"{label}: {type(error).__name__}: {error}")
    report["cleanup"] = not errors
    if errors:
        report["success"] = False
        report["cleanup_errors"] = errors
        raise RuntimeError("incomplete portal check cleanup: " + "; ".join(errors))
