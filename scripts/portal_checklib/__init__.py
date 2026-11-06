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
    """Pin the original leader while draining its inherited process group.

    WNOWAIT retains the PGID anchor; a pidfd and the birth identity reject a
    replacement child after an external reap. Callers must not reap the leader.
    Descendants that create another group or session are outside this boundary.
    Reaping after failed verification retires signalling, not the failure.
    """

    def __init__(self, command, **kwargs):
        # Reject unavailable pidfd support before spawning anything. Popen also
        # releases its exec-error pipe descriptors before we acquire the child fd.
        probe = os.pidfd_open(os.getpid())
        try:
            self._child = subprocess.Popen(command, start_new_session=True, **kwargs)
            # Only pidfd waits may reap this child, including after setup or
            # teardown fails. Disable Popen's implicit numeric waitpid from
            # birth; poll() ignores this private placeholder until a real reap.
            self._child.returncode = 0
            self.pid = self._child.pid
            self.stdout = self._child.stdout
            self._pidfd = None
            self._reaped = False
            self._last_status = None
            self._ownership_error = None
            self._cleanup_error = None
            try:
                self._pidfd = os.pidfd_open(self.pid)
                self._identity = self._leader_identity()
                self._require_owner()
            except (OSError, ValueError, IndexError) as error:
                self._abort_spawn(error)
                raise
        finally:
            os.close(probe)

    def _close_pidfd(self):
        if self._pidfd is not None:
            os.close(self._pidfd)
            self._pidfd = None

    @staticmethod
    def _returncode(status):
        return (
            status.si_status if status.si_code == os.CLD_EXITED else -status.si_status
        )

    def _waitid(self, *, reap=False):
        flags = os.WEXITED | os.WNOHANG | (0 if reap else os.WNOWAIT)
        try:
            return os.waitid(os.P_PIDFD, self._pidfd, flags)
        except ChildProcessError as error:
            self._ownership_error = error
            self._reaped = True
            # Prevent Popen.__del__ from doing a numeric waitpid on a replacement.
            # Unknown status is bookkeeping only: poll/stop raise ownership loss.
            self._child.returncode = self._last_status or 0
            self._close_pidfd()
            raise

    def poll(self):
        if self._ownership_error is not None:
            raise self._ownership_error
        if self._reaped:
            return self._child.returncode
        status = self._waitid()
        if status is None:
            return None
        self._last_status = self._returncode(status)
        return self._last_status

    def _leader_identity(self):
        fields = Path(f"/proc/{self.pid}/stat").read_bytes().rsplit(b")", 1)[1].split()
        return int(fields[19]), int(fields[2]), int(fields[3])

    def _require_owner(self):
        self.poll()  # Stable original child, not merely any child with this PID.
        identity = self._leader_identity()
        if identity != self._identity or identity[1:] != (self.pid, self.pid):
            raise ChildProcessError(f"owned leader {self.pid} identity changed")

    def _reap(self, deadline):
        while True:
            status = self._waitid(reap=True)
            if status is not None:
                self._child.returncode = self._returncode(status)
                self._reaped = True
                self._close_pidfd()
                return
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(
                    f"owned leader {self.pid} did not exit before deadline"
                )
            time.sleep(min(0.025, remaining))

    def _abort_spawn(self, error):
        if self._pidfd is None:
            # No safe PID fallback if even acquiring the birth handle failed.
            error.add_note(
                f"no stable handle for spawned leader {self.pid}; recovery required"
            )
            return
        deadline = time.monotonic() + 3
        try:
            self.poll()  # Pins the original child even if its stat was unreadable.
            try:
                os.killpg(self.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        except OSError as cleanup_error:
            error.add_note(f"spawn cleanup signal: {cleanup_error}")
        finally:
            if not self._reaped:
                try:
                    self._reap(deadline)
                except OSError as cleanup_error:
                    error.add_note(
                        f"spawn cleanup reap: {cleanup_error}; recovery required"
                    )
            self._close_pidfd()

    def _live_members(self, deadline):
        members = []
        for entry in Path("/proc").iterdir():
            if time.monotonic() >= deadline:
                return None  # Incomplete scan; never an empty census.
            if not entry.name.isdigit():
                continue
            try:
                fields = (entry / "stat").read_bytes().rsplit(b")", 1)[1].split()
            except (FileNotFoundError, ProcessLookupError):
                continue
            # stat describes the main thread. pthread_exit can leave it a
            # zombie while other threads still execute and retain resources.
            if (
                int(fields[2]) == self.pid
                and int(fields[3]) == self.pid
                and (fields[0] not in (b"Z", b"X") or int(fields[17]) > 1)
            ):
                members.append(int(entry.name))
        if time.monotonic() >= deadline:
            return None
        return members

    def _wait_empty(self, deadline):
        empty_once = False
        while time.monotonic() < deadline:
            exited = self.poll() is not None
            members = self._live_members(deadline)
            if members is None:
                return False
            empty = exited and not members
            if empty and empty_once:
                return True
            empty_once = empty
            time.sleep(min(0.025, max(0, deadline - time.monotonic())))
        return False

    def stop(self, term_timeout=3, kill_timeout=3):
        if self._reaped:
            if self._cleanup_error is not None or self._ownership_error is not None:
                raise self._cleanup_error or self._ownership_error
            return
        errors = []
        verified = False
        kill_attempted = False
        for signum, timeout in (
            (signal.SIGTERM, term_timeout),
            (signal.SIGKILL, kill_timeout),
        ):
            deadline = time.monotonic() + timeout
            # Revalidate before *each* signal. ECHILD is lost ownership, not
            # evidence that a numeric PGID is still safe to signal.
            try:
                self._require_owner()
            except (OSError, ValueError, IndexError) as error:
                errors.append(error)
                break
            kill_attempted = signum == signal.SIGKILL
            try:
                try:
                    os.killpg(self.pid, signum)
                except ProcessLookupError:
                    pass
                verified = self._wait_empty(deadline)
            except (OSError, ValueError, IndexError) as error:
                # Actual I/O/parsing errors stay failures even if KILL drains
                # the group. Our own phase deadline simply advances escalation.
                errors.append(error)
            if verified:
                break
        if kill_attempted and not verified and not errors:
            errors.append(
                TimeoutError(
                    f"owned group {self.pid} quiescence not verified after KILL"
                )
            )
        if not self._reaped:
            # The original pidfd remains safe to reap even when leader stat
            # cannot authorize a group signal. Keep the failure and spend only
            # the remaining phase budget; reaping does not verify quiescence.
            try:
                self._reap(deadline)
            except OSError as error:
                errors.append(error)
        if errors:
            for error in errors[1:]:
                errors[0].add_note(
                    f"additional cleanup error: {type(error).__name__}: {error}"
                )
            if not self._reaped:
                errors[0].add_note(
                    f"leader {self.pid} remains unreaped; recovery required"
                )
            if self._reaped and not verified:
                self._cleanup_error = errors[0]
            raise errors[0]


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
