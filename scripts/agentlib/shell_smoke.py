"""Compact shell acceptance shared by headless and owned native sessions."""

import argparse
import json
import shlex
import subprocess
from pathlib import Path

from . import daily_checks
from . import shell_checks as checks
from .control import ControlError
from .daily import DailySmoke
from .input import Input, keycode


def keyboard_search(smoke):
    leaked = smoke.artifacts / "terminal-leaked-search.txt"
    quoted = shlex.quote(str(leaked))
    with Input(smoke.control) as inputs:
        inputs.type(
            f"stty -echo; : > {quoted}; "
            f"while IFS= read -r -n1 key; do printf '%s' \"$key\" >> {quoted}; done\n"
        )
    smoke.until("terminal input guard ready", leaked.exists)
    with Input(smoke.control) as inputs:
        inputs.down("key", keycode("super"))
        if checks.panel(smoke, "search"):
            raise ControlError("Meta press opened search before release")
        inputs.up("key", keycode("super"))
        # Intentionally no render/readiness wait between release and typing.
        inputs.type("settings")
    checks.opened(smoke, "search")
    smoke.until(
        "complete immediately typed search query",
        lambda: checks.query_text(smoke) == "settings",
    )
    smoke.until(
        "Settings remains discoverable in search",
        lambda: checks.elements(smoke, "Open: Hearthspace Settings"),
    )
    if leaked.read_bytes():
        raise ControlError("Meta search leaked typed characters to the prior terminal")
    smoke.capture("search-settings")
    smoke.record("meta-immediate-search", query="settings", terminal_received_bytes=0)
    with Input(smoke.control) as inputs:
        inputs.chord("super")
    checks.closed(smoke)
    with Input(smoke.control) as inputs:
        inputs.chord("super+space")
    checks.opened(smoke, "search")
    with Input(smoke.control) as inputs:
        inputs.chord("esc")
    checks.closed(smoke)
    # The existing daily selector opens a second terminal through Super+Enter,
    # then queries and focuses the original window using a bare Meta tap.
    daily_checks.selector(smoke)
    checks.closed(smoke)


def launch_settings(smoke):
    before = {item["id"] for item in smoke.control.status()["windows"]}
    checks.click(smoke, "Search")
    checks.opened(smoke, "search")
    with Input(smoke.control) as inputs:
        inputs.type("Hearthspace Settings")
    checks.click(smoke, "Open: Hearthspace Settings", "search")
    checks.closed(smoke)
    window = smoke.until(
        "Settings launched from search",
        lambda: next(
            (
                item
                for item in smoke.control.status()["windows"]
                if item["id"] not in before
                and item.get("mapped")
                and item.get("focused")
                and item.get("title") == "Hearthspace Settings"
            ),
            None,
        ),
    )
    smoke.capture("settings-launched-from-search")
    smoke.control.request(f"close-window {window['id']}")
    smoke.until(
        "owned Settings window closed", lambda: smoke.window(window["id"]) is None
    )
    smoke.until(
        "application focus restored after Settings",
        lambda: (smoke.window(smoke.target) or {}).get("focused"),
    )
    smoke.record("settings-launched-from-search", window=window, closed=True)


def pointer_panels(smoke):
    checks.click(smoke, "Search")
    checks.opened(smoke, "search")
    checks.click(smoke, "Search")
    checks.closed(smoke)
    smoke.until(
        "search icon toggle restores application keyboard focus",
        lambda: smoke.window(smoke.target)["focused"],
    )
    checks.click(smoke, "Search")
    checks.opened(smoke, "search")
    if checks.elements(smoke, f"Window: {smoke.marker}"):
        raise ControlError(
            "empty search still exposes the removed unfiltered window list"
        )
    with Input(smoke.control) as inputs:
        inputs.type(smoke.marker)
    smoke.until(
        "pointer-opened search result",
        lambda: checks.elements(smoke, f"Window: {smoke.marker}"),
    )
    checks.click(smoke, "Close search", "search")
    checks.closed(smoke)
    checks.click(smoke, "Notifications")
    checks.opened(smoke, "notifications")
    smoke.capture("bell-notifications")
    checks.click(smoke, "Power")
    checks.opened(smoke, "power")
    if checks.panel(smoke, "notifications"):
        raise ControlError("opening Power left Notifications open")
    smoke.until("nested Lock option", lambda: checks.elements(smoke, "Lock"))
    checks.tab_to(smoke, "Lock")
    smoke.capture("power-options")
    checks.click(smoke, "Log out", "power")
    smoke.until(
        "logout confirmation", lambda: checks.elements(smoke, "Confirm log out")
    )
    smoke.capture("power-logout-confirmation")
    checks.tab_to(smoke, "Cancel")
    with Input(smoke.control) as inputs:
        inputs.chord("enter")
    smoke.until("logout cancelled", lambda: checks.elements(smoke, "Log out"))
    checks.click(smoke, "Close power menu", "power")
    checks.closed(smoke)
    if smoke.control.status().get("locked"):
        raise ControlError("cancelled power menu unexpectedly locked the session")
    smoke.record(
        "power-menu-verified",
        confirmation_cancelled=True,
        lock_option_present=True,
        keyboard_focus=["Lock", "Cancel"],
        keyboard_cancel=True,
    )


def run(smoke):
    checks.bar(smoke)
    smoke.capture("compact-shell-bar")
    daily_checks.terminal(smoke)
    if checks.panel(smoke, "search"):
        raise ControlError("Super+Enter incorrectly toggled search on Meta release")
    keyboard_search(smoke)
    launch_settings(smoke)
    pointer_panels(smoke)
    daily_checks.notifications(smoke)
    checks.closed(smoke)
    checks.bar(smoke)
    smoke.capture("shell-workflow-complete")
    smoke.record(
        "shell-workflow-verified",
        meta_search=True,
        running_window_search=True,
        settings_search=True,
        bell_notifications=True,
        power_cancel=True,
    )


class ShellSmoke(DailySmoke):
    def __init__(self, args):
        args.captures, args.notifications = 1, True
        super().__init__(args)

    def workflow(self):
        self.manifest = self.connect()
        self.step("ready", DailySmoke.ready)
        try:
            self.step("compact-shell", run)
        except BaseException:
            self.failure_evidence("compact-shell-failed")
            raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--total-timeout", type=float, default=240)
    parser.add_argument("--log-level", default="info")
    args = parser.parse_args()
    try:
        ShellSmoke(args).run()
        return 0
    except (
        ControlError,
        OSError,
        ValueError,
        KeyError,
        subprocess.SubprocessError,
        KeyboardInterrupt,
    ) as error:
        print(json.dumps({"ok": False, "error": str(error)}), flush=True)
        return 1
