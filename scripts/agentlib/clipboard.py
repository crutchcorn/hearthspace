"""Exercise real GTK clipboard transfers through compositor keyboard input."""

import hashlib
import json
import subprocess
from contextlib import ExitStack
from pathlib import Path

from .clipboard_agent import run_agent_clipboard
from .control import ControlError, Deadline
from .input import Input
from .primary import run_primary


def stop_client(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


class ClipboardRun:
    def __init__(self, smoke, env, fixture, resources):
        self.smoke = smoke
        self.control = smoke.control
        self.env = env
        self.fixture = Path(fixture).resolve()
        self.resources = resources
        self.clients = []

    def read(self, client):
        if client["process"].poll() is not None:
            raise ControlError(f"clipboard client exited; see {client['log']}")
        path = client["output"]
        state = json.loads(path.read_text()) if path.exists() else {}
        return state if state.get("pid") == client["process"].pid else {}

    def wait(self, client, label, predicate):
        deadline = Deadline(20)

        def condition():
            state = self.read(client)
            if predicate(state):
                return state
            try:
                deadline.remaining()
            except ControlError as error:
                raise ControlError(
                    f"timed out waiting for {label}: {client['title']}"
                ) from error
            return None

        return self.smoke.until(label, condition)

    def window(self, client):
        return next(
            (
                window
                for window in self.control.status()["windows"]
                if window["title"] == client["title"] and window["height"] > 100
            ),
            None,
        )

    def launch(self, backend, name, x):
        source = self.smoke.artifacts / f"{name}-input.txt"
        output = self.smoke.artifacts / f"{name}-state.json"
        source.write_text("", encoding="utf-8")
        title = f"Hearthspace Clipboard {name}"
        log_path = self.smoke.artifacts / f"{name}.log"
        log = self.resources.enter_context(log_path.open("w"))
        env = {
            **self.env,
            "GDK_BACKEND": backend,
            "GTK_A11Y": "none",
            "GIO_USE_VFS": "local",
            "GSETTINGS_BACKEND": "memory",
            "GDK_DEBUG": ",".join(
                filter(None, (self.env.get("GDK_DEBUG"), "no-portals"))
            ),
        }
        # Clipboard fixtures need no desktop portals. In a private session,
        # activating one also starts document-portal FUSE mounts and host helpers.
        # Host accessibility/display settings must not redirect these fixtures.
        env.pop("AT_SPI_BUS_ADDRESS", None)
        env.pop("XAUTHORITY", None)
        display = self.smoke.until(
            "XWayland display", lambda: self.control.status().get("xwayland_display")
        )
        env["DISPLAY"] = display
        command = [
            str(self.fixture),
            "--title",
            title,
            "--input-file",
            str(source),
            "--output-file",
            str(output),
        ]
        process = subprocess.Popen(command, env=env, stdout=log, stderr=log)
        self.resources.callback(stop_client, process)
        client = {
            "process": process,
            "input": source,
            "output": output,
            "title": title,
            "log": log_path,
        }
        self.clients.append(client)
        state = self.wait(client, f"{name} map", lambda state: state.get("ready"))
        if state["backend"] != backend:
            raise ControlError(f"expected {backend}, got {state['backend']}")
        window = self.smoke.until(f"{name} window", lambda: self.window(client))
        # GTK's own title bar is draggable when compositor chrome is absent.
        titlebar = window["titlebar_height"] or 32
        with Input(self.control) as inputs:
            inputs.drag(
                window["x"] + window["width"] / 2,
                window["y"] + titlebar / 2,
                x + window["width"] / 2,
                120 + titlebar / 2,
                duration=0.5,
            )
        self.smoke.until(
            f"{name} placement",
            lambda: (
                (placed := self.window(client))
                # GTK can update its client-side shadow margins while mapping;
                # allow that offset while keeping the two editors separate.
                and abs(placed["x"] - x) < 32
                and abs(placed["y"] - 120) < 32
            ),
        )
        self.smoke.record(
            "clipboard-client", backend=backend, pid=process.pid, argv=command
        )
        return client

    def focus(self, client):
        window = self.window(client)
        if window is None:
            raise ControlError(f"window disappeared: {client['title']}")
        self.control.request(f"focus-window {window['id']}")
        window = self.window(client)
        with Input(self.control) as inputs:
            inputs.click(window["x"] + 70, window["y"] + window["height"] / 2)
        self.wait(client, "editor focus", lambda state: state.get("focused"))

    def reload(self, client, text):
        self.focus(client)
        client["input"].write_text(text, encoding="utf-8")
        before = self.read(client)["revision"]
        with Input(self.control) as inputs:
            inputs.chord("f5")
        return self.wait(
            client,
            "editor reload",
            lambda state: (
                state.get("revision", -1) > before and state.get("text") == text
            ),
        )

    def transfer(self, source, target, text, label, *, capture=True, cut=False):
        self.reload(target, "Paste must replace this sentinel")
        before_target = self.read(target)["clipboard_revision"]
        before_source = self.reload(source, text)
        operation = "cut" if cut else "copy"
        with Input(self.control) as inputs:
            inputs.chord("ctrl+a")
            inputs.chord("ctrl+x" if cut else "ctrl+c")
        source_state = self.wait(
            source,
            f"{operation} selection",
            lambda state: (
                state.get("clipboard_revision", -1)
                > before_source["clipboard_revision"]
                and (
                    not cut
                    or (
                        state.get("revision", -1) > before_source["revision"]
                        and state.get("text") == ""
                    )
                )
            ),
        )
        self.focus(target)
        self.wait(
            target,
            "new clipboard offer",
            lambda state: state.get("clipboard_revision", -1) > before_target,
        )
        before_paste = self.read(target)["paste_completed"]
        with Input(self.control) as inputs:
            inputs.chord("ctrl+a")
            inputs.chord("ctrl+v")
        state = self.wait(
            target,
            label,
            lambda state: (
                state.get("paste_completed", -1) > before_paste
                and state.get("text") == text
            ),
        )
        payload = text.encode("utf-8")
        self.smoke.record(
            label,
            bytes=len(payload),
            sha256=hashlib.sha256(payload).hexdigest(),
            target_backend=state["backend"],
            source_serialization=self.read(source)["clipboard_serialization"],
            paste_completed=state["paste_completed"],
            exact_text=True,
            operation=operation,
            **({"source_empty": source_state["text"] == ""} if cut else {}),
        )
        if capture:
            self.smoke.capture(label)

    def close(self, client):
        stop_client(client["process"])
        self.smoke.until("closed source window", lambda: self.window(client) is None)
        self.smoke.record("clipboard-source-closed", title=client["title"])


def run_clipboard(smoke, env, fixture):
    """Run with a connected Smoke helper and a private display/session environment."""
    if not Path(fixture).is_file():
        raise ControlError(
            "build hearthspace-clipboard-test with --features test-apps first"
        )
    required = ("XDG_RUNTIME_DIR", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS")
    if any(not env.get(key) for key in required):
        raise ControlError(
            "clipboard fixtures require an explicit private session environment"
        )
    smoke.control.request("reset-view")
    smoke.until(
        "clipboard layout at normal zoom",
        lambda: smoke.control.status()["viewport"]["scale"] == 1.0,
    )
    with ExitStack() as resources:
        run = ClipboardRun(smoke, env, fixture, resources)
        wayland = run.launch("wayland", "wayland", 40)
        x11 = run.launch("x11", "x11", 660)
        run.transfer(
            wayland, x11, "Wayland → X11\nCafé, 日本語, 🦀\n", "wayland-to-x11"
        )
        run.transfer(
            x11, wayland, "X11 → Wayland\nnaïve, Ελληνικά, 🔥\n", "x11-to-wayland"
        )
        run.transfer(
            wayland,
            x11,
            "Cut Wayland → X11\nCafé, 日本語, 🦀\n",
            "cut-wayland-to-x11",
            cut=True,
        )
        run.transfer(
            x11,
            wayland,
            "Cut X11 → Wayland\nnaïve, Ελληνικά, 🔥\n",
            "cut-x11-to-wayland",
            cut=True,
        )
        # Exercise large transfers in both directions, including Smithay's INCR path.
        large = "Clipboard payload: λ 日本語 🦀\n" * 16384
        run.transfer(
            wayland,
            x11,
            "Wayland large\n" + large,
            "large-wayland-to-x11",
            capture=False,
        )
        run.transfer(
            x11, wayland, "X11 large\n" + large, "large-x11-to-wayland", capture=False
        )
        # A new Wayland owner must survive the old X11 owner's destruction.
        run.transfer(
            wayland,
            x11,
            "Replacement owner survives old source exit",
            "replacement-owner",
        )
        run.close(x11)
        peer = run.launch("wayland", "wayland-peer", 660)
        run.focus(peer)
        before = run.read(peer)["paste_completed"]
        with Input(smoke.control) as inputs:
            inputs.chord("ctrl+v")
        run.wait(
            peer,
            "clipboard after old X11 source exit",
            lambda state: (
                state.get("paste_completed", -1) > before
                and state.get("text") == "Replacement owner survives old source exit"
            ),
        )
        smoke.record("old-x11-owner-exit", exact_text=True)
        run.transfer(peer, wayland, "Native Wayland copy/paste", "wayland-to-wayland")
        # Closing the actual owner must not freeze or poison subsequent transfers.
        run.close(peer)
        replacement = run.launch("x11", "replacement-x11", 660)
        run.transfer(
            wayland,
            replacement,
            "Clipboard works after source exit",
            "source-exit-recovery",
        )
        # Exercise destruction of the active X11 owner too, rather than only an
        # X11 client whose clipboard has already been replaced by Wayland.
        run.transfer(
            replacement,
            wayland,
            "The active X11 clipboard owner is about to exit",
            "active-x11-owner",
            capture=False,
        )
        run.close(replacement)
        recovered = run.launch("x11", "recovered-x11", 660)
        run.transfer(
            wayland,
            recovered,
            "Clipboard works after the active X11 owner exits",
            "x11-source-exit-recovery",
        )
        run_primary(run, wayland, recovered)
        run_agent_clipboard(run, wayland, recovered)
        smoke.capture("clipboard-final")
    smoke.record(
        "clipboard-clients-stopped",
        pids=[client["process"].pid for client in run.clients],
    )
