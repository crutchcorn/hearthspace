"""Daily native assertions using semantic state and exact received data."""

import json
import os
import select
import shlex
import socket
import subprocess
from pathlib import Path
from types import SimpleNamespace

from .accessibility import run_accessibility
from .control import ControlError, Deadline
from .input import Input
from .text import clipboard_command, paste
from .viewport_fixture import set_viewport

UNICODE_TEXT = "Hearthspace café — こんにちは 🌍\nSecond line: naïve"


def read_line(path, command, deadline):
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(deadline.remaining())
        stream.connect(str(path))
        stream.settimeout(deadline.remaining())
        stream.sendall(command)
        stream.shutdown(socket.SHUT_WR)
        response = bytearray()
        while not response.endswith(b"\n"):
            stream.settimeout(deadline.remaining())
            chunk = stream.recv(1)
            if not chunk:
                raise ControlError("service closed before completing its response")
            response.extend(chunk)
            if len(response) > 64 * 1024:
                raise ControlError("service response exceeded 64 KiB")
        return response.decode()


def a11y(smoke, action, selector=None):
    args = SimpleNamespace(
        session=smoke.directory, a11y_action=action, selector=selector, output=None
    )
    return run_accessibility(args, smoke.control, smoke.control.deadline)


def focused(smoke, window_id):
    window = smoke.window(window_id)
    return window if window and window.get("focused") and window.get("mapped") else None


def terminal(smoke):
    before = {item["id"] for item in smoke.control.status()["windows"]}
    with Input(smoke.control) as inputs:
        inputs.chord("super+enter")

    def launched():
        return next(
            (
                item
                for item in smoke.control.status()["windows"]
                if item["id"] not in before
                and "foot" in (item.get("app_id") or "").lower()
                and item.get("focused")
                and item.get("mapped")
            ),
            None,
        )

    window = smoke.until("keyboard-launched focused Foot", launched)
    smoke.target = window["id"]
    # A predictable shell removes distribution prompt title rewrites. No click or
    # explicit focus command is permitted before proving shortcut autofocus.
    with Input(smoke.control) as inputs:
        inputs.type("exec bash --noprofile --norc\n")
        inputs.type(
            "export PS1='$ '; unset PROMPT_COMMAND; "
            f"printf '\\033]0;{smoke.marker}\\007'\n"
        )
    smoke.until(
        "typed terminal title",
        lambda: (smoke.window(smoke.target) or {}).get("title") == smoke.marker,
    )
    smoke.capture("keyboard-terminal")
    smoke.record(
        "keyboard-terminal-verified", window_id=smoke.target, title=smoke.marker
    )


def unicode_paste(smoke):
    destination = smoke.artifacts / "unicode-received.txt"
    command = (
        f"printf '%s' {shlex.quote(UNICODE_TEXT)} > {shlex.quote(str(destination))}"
    )
    with Input(smoke.control) as inputs:
        paste(inputs, clipboard_command(command), "ctrl+shift+v")
        inputs.chord("enter")
    smoke.until(
        "exact Unicode bytes received by terminal",
        lambda: (
            destination.is_file() and destination.read_bytes() == UNICODE_TEXT.encode()
        ),
    )
    smoke.record("unicode-verified", artifact=str(destination), expected=UNICODE_TEXT)
    smoke.capture("unicode-paste")


def selector(smoke):
    # A second terminal makes this a real focus transition, not a no-op.
    before = {item["id"] for item in smoke.control.status()["windows"]}
    with Input(smoke.control) as inputs:
        inputs.chord("super+enter")
    smoke.until(
        "second terminal focused",
        lambda: any(
            item["id"] not in before and item.get("focused")
            for item in smoke.control.status()["windows"]
        ),
    )
    expected_ids = {item["id"] for item in smoke.control.status()["windows"]}
    with Input(smoke.control) as inputs:
        inputs.chord("super")
    smoke.until(
        "search overlay mapped",
        lambda: any(
            panel["kind"] == "search"
            for panel in smoke.control.status().get("shell_panels", [])
        ),
    )
    with Input(smoke.control) as inputs:
        inputs.type(smoke.marker)
    smoke.until(
        "running-window result in launcher",
        lambda: a11y(smoke, "find", f"//*[@name='Window: {smoke.marker}']")["elements"],
    )
    smoke.capture("running-window-search")
    with Input(smoke.control) as inputs:
        inputs.chord("enter")
    try:
        smoke.until("original terminal selected", lambda: focused(smoke, smoke.target))
    except ControlError:
        smoke.failure_evidence("running-window-submit-failed")
        raise
    actual_ids = {item["id"] for item in smoke.control.status()["windows"]}
    if actual_ids != expected_ids:
        raise ControlError("running-window selector spawned or removed an application")
    smoke.record(
        "selector-verified", selected_id=smoke.target, window_ids=sorted(actual_ids)
    )
    smoke.capture("running-window-selected")


def off_canvas(smoke):
    original = smoke.window(smoke.target)
    smoke.control.request(f"move-window {smoke.target} 100000 100000")
    smoke.until(
        "off-canvas window hidden",
        lambda: (smoke.window(smoke.target) or {}).get("visible") is False,
    )
    smoke.capture("off-canvas-hidden")
    smoke.control.request(f"focus-window {smoke.target}")
    visible = smoke.until(
        "off-canvas focus recovery",
        lambda: (
            window
            if (window := focused(smoke, smoke.target)) and window.get("visible")
            else None
        ),
    )
    smoke.record("off-canvas-recovered", window=visible)
    smoke.capture("off-canvas-recovered")
    smoke.control.request(
        f"move-window {smoke.target} {original['canvas_x']} {original['canvas_y']}"
    )
    set_viewport(smoke, x=0, y=0, scale=1)
    smoke.control.request(f"focus-window {smoke.target}")


def window_states(smoke):
    for command, fullscreen, maximized in [
        (f"maximize-window {smoke.target} true", False, True),
        (f"fullscreen-window {smoke.target} true", True, True),
        (f"fullscreen-window {smoke.target} false", False, True),
        (f"restore-window {smoke.target}", False, False),
    ]:
        smoke.control.request(command)

        def matches(fullscreen=fullscreen, maximized=maximized):
            window = smoke.window(smoke.target)
            return (
                window
                if window
                and window.get("fullscreen") == fullscreen
                and window.get("maximized") == maximized
                and bool(window.get("presentation_output")) == fullscreen
                and bool(window.get("output_coordinates")) == fullscreen
                else None
            )

        window = smoke.until(command, matches)
        smoke.record("window-state", command=command, window=window)
        smoke.capture(
            f"state-full-{int(fullscreen)}-max-{int(maximized)}-{command.split()[0]}"
        )


def captures(smoke):
    for output in smoke.control.status()["outputs"]:
        previous = None
        for index in range(smoke.args.captures):
            metadata, _ = smoke.capture(
                f"repeat-{output['name']}-{index}", output["name"]
            )
            if metadata["output"] != output["name"]:
                raise ControlError("capture returned a different output")
            if previous is not None and metadata["frame_id"] <= previous:
                raise ControlError("presented frame IDs did not advance")
            previous = metadata["frame_id"]


def stalled_settings(smoke):
    path = smoke.manifest["settings_socket"]
    with socket.socket(socket.AF_UNIX) as stalled:
        stalled.settimeout(smoke.control.deadline.remaining())
        stalled.connect(path)
        stalled.sendall(b"set test ")
        # The settings server expires unfinished peers after 250 ms. A healthy
        # reply must arrive while this peer is still pending, before that expiry.
        deadline = Deadline(min(0.2, smoke.control.deadline.remaining()))
        reply = read_line(path, b"get\n", deadline)
        if not reply.startswith("ok "):
            raise ControlError(f"healthy settings query failed: {reply}")
        if select.select([stalled], [], [], 0)[0]:
            raise ControlError(
                "stalled client was dropped before healthy query completed"
            )
        status = smoke.control.status()
        smoke.capture("stalled-settings-client")
        smoke.record(
            "settings-responsive", response=reply.strip(), backend=status["backend"]
        )


def accessibility(smoke):
    selector = "(//*[@role='button' and @name='Canvas map'])[1]"
    matches = smoke.until(
        "semantic map button", lambda: a11y(smoke, "find", selector)["elements"]
    )
    if matches[0].get("role") != "Button":
        raise ControlError("shell button has no usable accessibility role")
    a11y(smoke, "activate", selector)
    zoom = "(//*[@role='button' and @name='Zoom out'])[1]"
    smoke.until("semantic zoom button", lambda: a11y(smoke, "find", zoom)["elements"])
    snapshot = a11y(smoke, "snapshot")
    (smoke.artifacts / "accessibility.xml").write_text(snapshot["snapshot"])
    before = smoke.control.status()["viewport"]["scale"]
    a11y(smoke, "activate", zoom)
    smoke.until(
        "semantic viewport zoom",
        lambda: smoke.control.status()["viewport"]["scale"] < before,
    )
    smoke.capture("semantic-map-zoom")
    a11y(smoke, "activate", selector)
    smoke.until(
        "semantic map toggle dismisses the popover",
        lambda: not smoke.control.status().get("canvas_maps"),
    )
    smoke.record("semantic-action-verified", selector=selector, elements=matches)


def notifications(smoke):
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
            a11y(
                smoke,
                "activate",
                "(//*[(@role='button' or @role='toggle button') and @name='Notifications'])[1]",
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
