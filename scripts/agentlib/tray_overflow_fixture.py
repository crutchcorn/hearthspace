"""Many independent SNI object paths on the explicitly owned private bus."""

import contextlib
import json
import os
import subprocess
from pathlib import Path

from .control import ControlError
from .tray_fixture import Fixture

PREFIX = "Hearthspace Overflow "


def title(index):
    return f"{PREFIX}{index:03d}"


@contextlib.contextmanager
def fixture(smoke, count):
    peer = Fixture(smoke, "overflow")
    env = {
        **os.environ,
        "DBUS_SESSION_BUS_ADDRESS": smoke.manifest["dbus_session_bus_address"],
        "HEARTHSPACE_SESSION_BUS_ADDRESS": smoke.manifest["dbus_session_bus_address"],
        "XDG_RUNTIME_DIR": smoke.manifest["runtime_dir"],
    }
    for key in ("DBUS_SESSION_BUS_PID", "DISPLAY", "AT_SPI_BUS_ADDRESS"):
        env.pop(key, None)
    path = smoke.artifacts / "tray-overflow-peer.log"
    with path.open("w") as log:
        peer.process = subprocess.Popen(
            [
                "/usr/bin/python3",
                "-m",
                "agentlib.tray_overflow_fixture",
                str(count),
                str(peer.actions),
            ],
            cwd=Path(__file__).resolve().parents[1],
            env=env,
            stdout=log,
            stderr=log,
        )
        try:

            def ready():
                if peer.process.poll() is not None:
                    raise ControlError(f"overflow SNI peer exited: {path.read_text()}")
                return any(event.get("action") == "ready" for event in peer.events())

            smoke.until("all independent overflow items registered", ready)
            smoke.record(
                "tray-overflow-fixture-ready", count=count, actions=str(peer.actions)
            )
            yield peer
        finally:
            if peer.process.poll() is None:
                peer.process.terminate()
                try:
                    peer.process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    peer.process.kill()
                    peer.process.wait(timeout=2)
            smoke.record(
                "tray-overflow-fixture-stopped",
                pid=peer.process.pid,
                returncode=peer.process.returncode,
            )


def worker(count, actions):
    # PyGObject is needed only by the native fixture, never by helper unit tests.
    from gi.repository import Gio, GLib

    address = os.environ.get("HEARTHSPACE_SESSION_BUS_ADDRESS")
    if not address or address != os.environ.get("DBUS_SESSION_BUS_ADDRESS"):
        raise ValueError("requires the explicit, matching Hearthspace private bus")
    if not 16 <= count <= 64:
        raise ValueError("overflow fixture requires 16–64 items")
    properties = {
        "Category": ("s", "ApplicationStatus"),
        "Id": ("s", ""),
        "Title": ("s", ""),
        "Status": ("s", "Active"),
        "IconName": ("s", ""),
        "IconPixmap": ("a(iiay)", [(16, 16, bytes([255, 224, 48, 32]) * 256)]),
        "AttentionIconName": ("s", ""),
        "AttentionIconPixmap": ("a(iiay)", []),
        "OverlayIconName": ("s", ""),
        "OverlayIconPixmap": ("a(iiay)", []),
        "IconThemePath": ("s", ""),
        "WindowId": ("u", 0),
        "ItemIsMenu": ("b", False),
        "Menu": ("o", "/"),
        "ToolTip": ("(sa(iiay)ss)", ("", [], "Overflow acceptance", "")),
    }
    methods = {
        "Activate": ("i", "i"),
        "SecondaryActivate": ("i", "i"),
        "ContextMenu": ("i", "i"),
        "Scroll": ("i", "s"),
    }
    interface = "org.kde.StatusNotifierItem"
    xml = f'<node><interface name="{interface}">'
    for name, (signature, _) in properties.items():
        xml += f'<property name="{name}" type="{signature}" access="read"/>'
    for name, signatures in methods.items():
        xml += f'<method name="{name}">'
        xml += "".join(
            f'<arg type="{signature}" direction="in"/>' for signature in signatures
        )
        xml += "</method>"
    info = Gio.DBusNodeInfo.new_for_xml(xml + "</interface></node>").interfaces[0]
    connection = Gio.DBusConnection.new_for_address_sync(
        address,
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT
        | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None,
        None,
    )
    loop = GLib.MainLoop()
    connection.connect("closed", lambda *_: loop.quit())

    def record(event):
        with actions.open("a") as log:
            log.write(json.dumps(event) + "\n")

    def method(_bus, _sender, path, _interface, name, arguments, invocation):
        record({"action": name, "path": path, "arguments": arguments.unpack()})
        invocation.return_value(GLib.Variant("()", ()))

    def property_value(_bus, _sender, path, _interface, name):
        index = int(path.rsplit("/", 1)[-1][4:])
        signature, value = properties[name]
        if name == "Title":
            value = title(index)
        elif name == "Id":
            value = f"hearthspace-overflow-{index:03d}"
        return GLib.Variant(signature, value)

    for index in range(count):
        path = f"/Overflow/Item{index:03d}"
        connection.register_object(path, info, method, property_value, None)
        connection.call_sync(
            "org.kde.StatusNotifierWatcher",
            "/StatusNotifierWatcher",
            "org.kde.StatusNotifierWatcher",
            "RegisterStatusNotifierItem",
            GLib.Variant("(s)", (path,)),
            None,
            Gio.DBusCallFlags.NONE,
            5000,
            None,
        )
    record({"action": "ready", "count": count, "owner": connection.get_unique_name()})
    loop.run()


if __name__ == "__main__":
    import sys

    worker(int(sys.argv[1]), Path(sys.argv[2]))
