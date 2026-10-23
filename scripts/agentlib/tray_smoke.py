"""App tray acceptance on an owned compositor and private session bus."""

import argparse
import json
import subprocess
from pathlib import Path

from . import shell_checks, tray_library, tray_overflow, tray_touch
from . import tray_checks as checks
from .control import ControlError
from .daily import DailySmoke
from .daily_checks import a11y
from .input import Input
from .map_checks import selector
from .tray_fixture import fixture


def keyboard(smoke, peer):
    checks.open_menu(smoke)
    for key, label in (
        (None, "Open fixture"),
        ("down", "Enabled toggle"),
        ("down", "More"),
        ("right", "Nested action"),
        ("left", "More"),
        ("up", "Enabled toggle"),
    ):
        if key:
            with Input(smoke.control) as inputs:
                inputs.chord(key)
        smoke.until(
            f"keyboard selects {label}",
            lambda label=label: checks.visible(
                smoke, label, suffix="[@focused='true']"
            ),
        )
    checks.snapshot(smoke, "tray-keyboard-navigation")
    with Input(smoke.control) as inputs:
        inputs.chord("up")
    smoke.until(
        "keyboard returns to first app action",
        lambda: checks.visible(smoke, "Open fixture", suffix="[@focused='true']"),
    )
    with Input(smoke.control) as inputs:
        inputs.chord("enter")
    peer.expect("menu", id=1, event="clicked")
    shell_checks.closed(smoke)
    smoke.record("tray-keyboard-verified", skipped_disabled=True, nested_arrows=True)


def menu(smoke, peer):
    checks.open_menu(smoke)
    peer.expect("about-to-show", id=0)
    smoke.until("checked app menu item", lambda: checks.checked(smoke, True))
    disabled = smoke.until(
        "disabled app action is not focusable or selected",
        lambda: checks.visible(
            smoke,
            "Disabled action",
            suffix="[@role='menu item' and not(@focusable='true') and not(@selected='true')]",
        ),
    )
    smoke.until("accessible app menu separator", lambda: checks.separator(smoke))
    checks.snapshot(smoke, "tray-menu")
    checks.click(smoke, "Disabled action", menu=True)
    shell_checks.opened(smoke, "tray")
    try:
        a11y(smoke, "activate", selector("Disabled action", role="menu item"))
        semantic_rejection = None
    except ControlError as error:
        semantic_rejection = str(error)
    smoke.record(
        "tray-disabled-accessibility",
        elements=disabled,
        semantic_rejection=semantic_rejection,
        upstream_enabled_flag_issue="https://github.com/AccessKit/accesskit/pull/788",
    )
    checks.click(smoke, "Enabled toggle", menu=True)
    peer.expect("menu", id=2, event="clicked")
    if not shell_checks.panel(smoke, "tray"):
        checks.open_menu(smoke)
    smoke.until("app checkbox toggled off", lambda: checks.checked(smoke, False))
    checks.click(smoke, "More", menu=True)
    smoke.until("nested app menu", lambda: checks.visible(smoke, "Nested action"))
    checks.snapshot(smoke, "tray-nested-menu")
    checks.click(smoke, "Nested action", menu=True)
    peer.expect("menu", id=6, event="clicked")
    shell_checks.closed(smoke)
    checks.open_menu(smoke)
    peer.command("menu-update")
    smoke.until(
        "live app menu update", lambda: checks.visible(smoke, "Updated fixture")
    )
    if checks.visible(smoke, "Open fixture"):
        raise ControlError("app menu retained stale label after LayoutUpdated")
    checks.snapshot(smoke, "tray-menu-updated")
    # Accessibility action targets the same exported app action as pointer input.
    before = len(peer.events())
    a11y(smoke, "activate", selector("Updated fixture"))
    peer.expect("menu", after=before, id=1, event="clicked")
    shell_checks.closed(smoke)
    if any(
        event.get("action") == "menu" and event.get("id") == 3
        for event in peer.events()
    ):
        raise ControlError("disabled tray action was delivered to application")
    smoke.record("tray-menu-verified", disabled=disabled, nested=True, live_update=True)


def interaction(smoke, peer):
    checks.presented(smoke, "red")
    checks.click(smoke)
    peer.expect("activate")
    checks.click(smoke, button="middle")
    peer.expect("secondary")
    with Input(smoke.control) as inputs:
        inputs.motion(*checks.point(smoke))
        inputs.scroll(0, 15)
    event = peer.expect("scroll", orientation="vertical")
    if event["delta"] == 0:
        raise ControlError("tray scroll delivered a zero delta")
    keyboard(smoke, peer)
    menu(smoke, peer)
    checks.open_menu(smoke)
    checks.dismiss(smoke)
    checks.open_menu(smoke)
    output = smoke.control.status()["outputs"][0]
    with Input(smoke.control) as inputs:
        inputs.click(
            output["logical_x"] + 10,
            output["logical_y"] + output["logical_height"] - 10,
        )
    shell_checks.closed(smoke)
    # Every app's menu is reachable without a secondary mouse button.
    if checks.visible(smoke, "Application tray"):
        raise ControlError("tray added a container button beside the app icons")
    checks.click(smoke)
    with Input(smoke.control) as inputs:
        inputs.chord("shift+f10")
    shell_checks.opened(smoke, "tray")
    smoke.until(
        "tray menu from focused app icon",
        lambda: checks.visible(smoke, "Updated fixture"),
    )
    checks.dismiss(smoke)
    peer.command("blue")
    checks.presented(smoke, "blue")
    peer.command("named-red")
    checks.presented(smoke, "red")
    peer.command("named-blue")
    checks.presented(smoke, "blue")
    smoke.record("tray-named-icon-invalidated", same_filename=True)
    peer.command("passive")
    checks.disappeared(smoke)
    peer.command("attention")
    checks.icon(smoke)
    peer.command("active")
    peer.command("unregister")
    checks.disappeared(smoke)
    peer.command("register")
    checks.presented(smoke, "blue")
    # Repeating the same registration must not create a second bar icon.
    peer.command("register")
    checks.icon(smoke)
    peer.command("menu-only")
    peer.command("red")
    checks.presented(smoke, "red")
    checks.click(smoke)
    shell_checks.opened(smoke, "tray")
    smoke.until(
        "menu-only primary activation", lambda: checks.visible(smoke, "Updated fixture")
    )
    checks.dismiss(smoke)
    peer.command("no-menu")
    peer.command("blue")
    checks.presented(smoke, "blue")
    checks.click(smoke, button="right")
    peer.expect("context")
    shell_checks.closed(smoke)


def run(smoke):
    shell_checks.closed(smoke)
    if checks.visible(smoke):
        raise ControlError("tray fixture already exists in acceptance session")
    with fixture(smoke) as peer:
        try:
            interaction(smoke, peer)
        except BaseException:
            smoke.failure_evidence("tray-live-failure")
            raise
    checks.disappeared(smoke)
    # A new process/unique owner can reuse the same well-known name and app ID.
    with fixture(smoke, 2) as peer:
        checks.presented(smoke, "red")
        tray_touch.run(smoke, peer)
        checks.open_menu(smoke)
    checks.disappeared(smoke)
    shell_checks.closed(smoke)
    smoke.until(
        "removed tray button remains absent",
        lambda: not checks.visible(smoke, "Application tray"),
    )
    tray_library.run(smoke)
    smoke.until(
        "tray container button remains absent after library app exit",
        lambda: not checks.visible(smoke, "Application tray"),
    )
    smoke.capture("tray-workflow-complete")
    smoke.record(
        "tray-workflow-verified", owner_restart=True, disappearing_menu_closed=True
    )


class TraySmoke(DailySmoke):
    def __init__(self, args):
        args.captures, args.notifications = 1, False
        super().__init__(args)

    def workflow(self):
        self.manifest = self.connect()
        self.step("ready", DailySmoke.ready)
        try:
            if not self.args.overflow_only:
                self.step("application-tray", run)
            if self.args.overflow or self.args.overflow_only:
                tray_overflow.run(self)
        except BaseException:
            self.failure_evidence("application-tray-failed")
            raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--total-timeout", type=float, default=240)
    parser.add_argument("--log-level", default="info")
    overflow = parser.add_mutually_exclusive_group()
    overflow.add_argument(
        "--overflow",
        action="store_true",
        help="also test standalone tray overflow on top and left",
    )
    overflow.add_argument(
        "--overflow-only",
        action="store_true",
        help="only test native standalone tray overflow",
    )
    args = parser.parse_args()
    try:
        TraySmoke(args).run()
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
