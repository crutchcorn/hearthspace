"""Optional deployed Ayatana compatibility check on the owned display and bus."""

import ctypes.util
import json
import os
import re
import subprocess
import sys
from pathlib import Path

from . import shell_checks, tray_checks
from .control import ControlError
from .shell_presentation import Frame, park_pointer

TITLE = "Ayatana tray fixture"


def properties(smoke):
    def call(owner, path, method, *arguments):
        return subprocess.run(
            [
                "gdbus",
                "call",
                "--address",
                smoke.manifest["dbus_session_bus_address"],
                "--dest",
                owner,
                "--object-path",
                path,
                "--method",
                method,
                "--",
                *arguments,
            ],
            check=True,
            capture_output=True,
            text=True,
            timeout=smoke.control.deadline.remaining(),
        ).stdout

    registered = call(
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.freedesktop.DBus.Properties.Get",
        "org.kde.StatusNotifierWatcher",
        "RegisteredStatusNotifierItems",
    )
    for address in re.findall(r"'([^']+)'", registered):
        owner, slash, path = address.partition("/")
        if not slash:
            continue
        path = slash + path
        title = call(
            owner,
            path,
            "org.freedesktop.DBus.Properties.Get",
            "org.kde.StatusNotifierItem",
            "Title",
        )
        if TITLE not in title:
            continue
        value = call(
            owner,
            path,
            "org.freedesktop.DBus.Properties.GetAll",
            "org.kde.StatusNotifierItem",
        )
        target = smoke.artifacts / "tray-ayatana-properties.txt"
        target.write_text(f"Registered items: {registered}\n{owner}{path}\n{value}")
        smoke.record("tray-library-properties", artifact=str(target))
        return
    raise ControlError("Ayatana tray item was not present on the owned watcher")


def central_ink(frame, bounds):
    surface = {
        "x": frame.metadata["logical_x"],
        "y": frame.metadata["logical_y"],
        "width": frame.width / frame.metadata["scale"],
        "height": 48,
    }
    left, top, right, bottom = frame.rect(surface, bounds)
    center = (left + right) // 2
    half = max(1, (right - left) // 20)
    inset = (bottom - top) // 5
    # The missing-icon fallback has four separated dots and an empty central
    # column. The standard information glyph has strokes in this column.
    return sum(
        sum(frame.rgb(x, y)) < 540
        for y in range(top + inset, bottom - inset)
        for x in range(center - half, center + half)
    )


def presented(smoke):
    output = smoke.control.status()["outputs"][0]
    park_pointer(smoke, output)

    def ready():
        observed = tray_checks.visible(smoke, TITLE)
        if not observed:
            return None
        frame = Frame(smoke.control.capture(output["name"]))
        count = central_ink(frame, observed[0]["bounds"])
        return count if count >= 6 * output["scale"] ** 2 else None

    count = smoke.until("Ayatana named glyph rendered instead of fallback dots", ready)
    smoke.capture("tray-ayatana-icon")
    smoke.record("tray-library-icon-presented", central_glyph_pixels=count)


def run(smoke):
    libraries = ("gtk-3", "ayatana-appindicator3")
    missing = [name for name in libraries if ctypes.util.find_library(name) is None]
    if missing:
        smoke.record("tray-library-unavailable", missing=missing)
        return
    script = Path(__file__).resolve().parents[1] / "tests/fixtures/ayatana_tray.py"
    actions = smoke.artifacts / "tray-ayatana-actions.jsonl"
    log_path = smoke.artifacts / "tray-ayatana.log"
    env = {
        **os.environ,
        "DBUS_SESSION_BUS_ADDRESS": smoke.manifest["dbus_session_bus_address"],
        "HEARTHSPACE_SESSION_BUS_ADDRESS": smoke.manifest["dbus_session_bus_address"],
        "XDG_RUNTIME_DIR": smoke.manifest["runtime_dir"],
        "WAYLAND_DISPLAY": smoke.manifest["wayland_display"],
        "GDK_BACKEND": "wayland",
    }
    for key in ("DBUS_SESSION_BUS_PID", "DISPLAY", "AT_SPI_BUS_ADDRESS"):
        env.pop(key, None)
    with log_path.open("w") as log:
        process = subprocess.Popen(
            [sys.executable, str(script), "--log", str(actions)],
            env=env,
            stdout=log,
            stderr=log,
        )
        try:

            def ready():
                if process.poll() is not None:
                    log.flush()
                    raise ControlError(f"Ayatana client exited: {log_path.read_text()}")
                return tray_checks.visible(smoke, TITLE)

            smoke.until("deployed Ayatana indicator registered", ready)
            properties(smoke)
            presented(smoke)
            tray_checks.click(smoke, TITLE, button="right")
            shell_checks.opened(smoke, "tray")
            smoke.until(
                "Ayatana exported menu",
                lambda: tray_checks.visible(smoke, "Ayatana library action"),
            )
            tray_checks.snapshot(smoke, "tray-ayatana-menu")
            tray_checks.click(smoke, "Ayatana library action", menu=True)

            def activated():
                return actions.exists() and any(
                    json.loads(line).get("event") == "activated"
                    for line in actions.read_text().splitlines(keepends=True)
                    if line.endswith("\n")
                )

            smoke.until("GTK menu receives exported Ayatana action", activated)
            shell_checks.closed(smoke)
            smoke.record(
                "tray-library-verified", libraries=list(libraries), actions=str(actions)
            )
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=2)
            smoke.record(
                "tray-library-stopped", pid=process.pid, returncode=process.returncode
            )
    smoke.until(
        "exited Ayatana icon removed", lambda: not tray_checks.visible(smoke, TITLE)
    )
