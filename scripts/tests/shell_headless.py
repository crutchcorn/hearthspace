"""Owned headless shell fixture with private accessibility and notifications."""

import contextlib
import io
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from types import SimpleNamespace
from xml.sax.saxutils import escape

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib.control import Control, ControlError, Deadline
from agentlib.daily_checks import a11y
from agentlib.smoke import Smoke
from test_clipboard_headless import private_bus_command, stop_process_group

ROOT = Path(__file__).resolve().parents[2]


def failure_evidence(smoke, label):
    original = smoke.control
    try:
        smoke.control = Control(original.path, Deadline(5))
        smoke.record("failure-status", status=smoke.control.status())
        smoke.capture(label)
        (smoke.artifacts / f"{label}.xml").write_text(
            a11y(smoke, "snapshot")["snapshot"]
        )
    except (
        ControlError,
        OSError,
        ValueError,
        KeyError,
        subprocess.SubprocessError,
    ) as error:
        smoke.record("failure-evidence-unavailable", error=str(error))
    finally:
        smoke.control = original


def shell_bus_command(directory, address):
    # Only accessibility may activate. Excluding desktop portals avoids any
    # host mounts or unrelated desktop services in this owned test session.
    command = private_bus_command(directory, address)
    services = directory / "services"
    services.mkdir()
    source = Path("/usr/share/dbus-1/services/org.a11y.Bus.service")
    service = source.read_text()
    (services / source.name).write_text(
        "\n".join(
            line
            for line in service.splitlines()
            if not line.startswith("SystemdService=")
        )
        + "\n"
    )
    config = directory / "bus.conf"
    config.write_text(
        config.read_text().replace(
            "</busconfig>",
            f"<servicedir>{escape(str(services))}</servicedir></busconfig>",
        )
    )
    return command


def run_shell_session(
    test,
    directory,
    evidence,
    workflow,
    marker,
    *,
    size=(2560, 1600),
    scale=2,
    services=None,
    timeout=120,
):
    test.assertFalse(
        (evidence / "session.json").exists(), "use fresh evidence directory"
    )
    binary = Path(
        os.environ.get("HEARTHSPACE_BINARY", ROOT / "target/debug/hearthspace")
    )
    socket = directory / "control.sock"
    address = f"unix:path={directory / 'bus'}"
    config = directory / "shell-config"
    config.mkdir(exist_ok=True)
    env = {
        **os.environ,
        "XDG_RUNTIME_DIR": str(directory),
        "XDG_CONFIG_HOME": str(config),
        "XDG_DATA_HOME": str(directory / "shell-data"),
        "XDG_STATE_HOME": str(directory / "shell-state"),
        "HEARTHSPACE_COMMAND_SOCKET": str(socket),
        "HEARTHSPACE_WAYLAND_DISPLAY": "wayland-map",
        "WAYLAND_DISPLAY": "wayland-map",
        "DBUS_SESSION_BUS_ADDRESS": address,
        # Battery and other system-service reads must also stay on the owned
        # bus. Tests never observe or invoke the host's UPower/logind services.
        "DBUS_SYSTEM_BUS_ADDRESS": address,
        "HEARTHSPACE_SESSION_BUS_ADDRESS": address,
        "GSETTINGS_BACKEND": "memory",
        "XDG_CURRENT_DESKTOP": "Hearthspace",
        "XDG_DATA_DIRS": f"{ROOT / 'data'}:{os.environ.get('XDG_DATA_DIRS', '/usr/local/share:/usr/share')}",
        "PATH": f"{binary.parent}:{os.environ.get('PATH', '/usr/bin:/bin')}",
    }
    for key in (
        "AT_SPI_BUS_ADDRESS",
        "DISPLAY",
        "DBUS_SESSION_BUS_PID",
        "GTK_A11Y",
    ):
        env.pop(key, None)
    (evidence / "session.json").write_text(
        json.dumps(
            {
                "dbus_session_bus_address": address,
                "command_socket": str(socket),
                "binary_dir": str(binary.parent),
                "runtime_dir": str(directory),
                "wayland_display": env["WAYLAND_DISPLAY"],
            }
        )
    )
    with contextlib.ExitStack() as cleanup:
        bus_log = cleanup.enter_context((evidence / "dbus.log").open("w"))
        bus = subprocess.Popen(
            shell_bus_command(directory, address),
            env=env,
            stdout=bus_log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        cleanup.callback(stop_process_group, bus)
        deadline = Deadline(timeout)
        while not (directory / "bus").exists():
            test.assertIsNone(bus.poll(), "private bus exited")
            time.sleep(min(0.02, deadline.remaining()))
        service_fixture = (
            cleanup.enter_context(services(directory, evidence, env, deadline))
            if services
            else None
        )
        log = cleanup.enter_context((evidence / "compositor.log").open("w"))
        compositor = subprocess.Popen(
            [
                str(binary),
                "--headless",
                "--no-shell",
                "--headless-size",
                f"{size[0]}x{size[1]}",
                "--headless-scale",
                str(scale),
            ],
            env=env,
            stdout=log,
            stderr=log,
            start_new_session=True,
        )
        cleanup.callback(stop_process_group, compositor)
        smoke = Smoke(
            SimpleNamespace(session=evidence, timeout=timeout), presentation="offscreen"
        )
        smoke.control = Control(socket, deadline)
        smoke.marker = marker
        smoke.manifest = json.loads((evidence / "session.json").read_text())
        smoke.service_fixture = service_fixture
        smoke.failure_evidence = lambda label: failure_evidence(smoke, label)
        # Ubuntu confines notify-send to standard runtime bus paths. Exercise
        # the same Notify protocol directly on this owned /tmp test bus, without
        # broadening host policy. Native acceptance still uses notify-send.
        smoke.notification_via_dbus = True
        while not socket.exists():
            test.assertIsNone(compositor.poll(), "headless compositor exited")
            time.sleep(min(0.05, deadline.remaining()))
        notifications_log = cleanup.enter_context(
            (evidence / "notifications.log").open("w")
        )
        notifications = subprocess.Popen(
            [str(binary.with_name("hearthspace-notifications"))],
            env=env,
            stdout=notifications_log,
            stderr=notifications_log,
            start_new_session=True,
        )
        cleanup.callback(stop_process_group, notifications)
        while not (directory / "hearthspace-notifications.sock").exists():
            test.assertIsNone(
                notifications.poll(), "private notification service exited"
            )
            time.sleep(min(0.02, deadline.remaining()))
        shell_log = cleanup.enter_context((evidence / "shell.log").open("w"))
        shell = subprocess.Popen(
            [str(binary.with_name("hearthspace-shell"))],
            env=env,
            stdout=shell_log,
            stderr=shell_log,
            start_new_session=True,
        )
        cleanup.callback(stop_process_group, shell)
        while True:
            try:
                if smoke.control.status().get("shell_ready"):
                    break
            except ControlError:
                pass
            test.assertIsNone(compositor.poll(), "headless compositor exited")
            test.assertIsNone(shell.poll(), "headless shell exited")
            time.sleep(min(0.05, deadline.remaining()))
        try:
            with contextlib.redirect_stdout(io.StringIO()):
                workflow(smoke)
        except (
            ControlError,
            OSError,
            ValueError,
            KeyError,
            subprocess.SubprocessError,
        ) as error:
            failure_evidence(smoke, "shell-workflow-failed")
            log.flush()
            test.fail(f"{error}\n{(evidence / 'compositor.log').read_text()[-16000:]}")
