"""Notification acceptance with asynchronous panel readiness before capture."""

import json
import os
import subprocess
from pathlib import Path

from .control import ControlError
from .daily_checks import a11y, read_line


def run(smoke):
    socket_path = Path(smoke.manifest["runtime_dir"]) / "hearthspace-notifications.sock"
    title = f"Daily notification {smoke.marker}"
    env = {
        **os.environ,
        "DBUS_SESSION_BUS_ADDRESS": smoke.manifest["dbus_session_bus_address"],
        "XDG_RUNTIME_DIR": smoke.manifest["runtime_dir"],
    }
    env.pop("DBUS_SESSION_BUS_PID", None)
    private_dbus_client = getattr(smoke, "notification_via_dbus", False)
    argv = (
        [
            "gdbus",
            "call",
            "--address",
            env["DBUS_SESSION_BUS_ADDRESS"],
            "--dest",
            "org.freedesktop.Notifications",
            "--object-path",
            "/org/freedesktop/Notifications",
            "--method",
            "org.freedesktop.Notifications.Notify",
            "--",
            "Hearthspace Daily Smoke",
            "0",
            "",
            title,
            "Semantic dismissal acceptance",
            "[]",
            "{}",
            "0",
        ]
        if private_dbus_client
        else [
            "notify-send",
            "--app-name=Hearthspace Daily Smoke",
            "--expire-time=0",
            "--wait",
            title,
            "Semantic dismissal acceptance",
        ]
    )
    client_log = smoke.artifacts / (
        "notification-dbus.log" if private_dbus_client else "notify-send.log"
    )
    with client_log.open("w") as log:
        process = subprocess.Popen(
            argv,
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        try:

            def inbox():
                response = read_line(
                    socket_path, b'{"command":"list"}\n', smoke.control.deadline
                )
                value = json.loads(response)
                if not value.get("ok"):
                    raise ControlError(f"notification query failed: {value}")
                return value["notifications"]

            def posted():
                # A denied bus connection must fail immediately rather than
                # spending the entire workflow budget waiting for a missing ID.
                code = process.poll()
                if code is not None and (code != 0 or not private_dbus_client):
                    log.flush()
                    raise ControlError(
                        f"notification client exited {code} before delivery: {client_log.read_text().strip()}"
                    )
                return next(
                    (
                        item
                        for item in inbox()
                        if item["summary"] == title and item["active"]
                    ),
                    None,
                )

            item = smoke.until(
                "private notification",
                posted,
            )
            active = [entry["id"] for entry in inbox() if entry["active"]]
            if active != [item["id"]]:
                raise ControlError(
                    "notification dismissal test requires its own sole active notification"
                )
            target = "(//*[(@role='button' or @role='toggle button') and @name='Notifications'])[1]"
            smoke.until(
                "notification control ready",
                lambda: a11y(smoke, "find", target)["elements"],
            )
            a11y(
                smoke,
                "activate",
                target,
                capture=False,
            )
            # Mapping needs a separate anchor IPC round trip. Do not block it
            # with readback immediately after accessibility accepts the action.
            smoke.until(
                "notification panel mapped",
                lambda: any(
                    panel["kind"] == "notifications"
                    for panel in smoke.control.status().get("shell_panels", [])
                ),
            )
            smoke.until(
                "notification dismissal button",
                lambda: a11y(smoke, "find", "//*[@role='button' and @name='Dismiss']")[
                    "elements"
                ],
            )
            smoke.capture("notification-open")
            a11y(smoke, "activate", "//*[@role='button' and @name='Dismiss']")
            closed = smoke.until(
                "notification dismissed",
                lambda: next(
                    (
                        entry
                        for entry in inbox()
                        if entry["id"] == item["id"] and not entry["active"]
                    ),
                    None,
                ),
            )
            if closed.get("close_reason") != 2:
                raise ControlError("notification was not closed by user dismissal")
            if process.wait(timeout=smoke.control.deadline.remaining()) != 0:
                raise ControlError(
                    f"notification client failed after dismissal: {client_log.read_text().strip()}"
                )
            a11y(
                smoke, "activate", "//*[@role='button' and @name='Close notifications']"
            )
            smoke.capture("notification-dismissed")
            smoke.record("notification-dismissed", notification=closed)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=2)
