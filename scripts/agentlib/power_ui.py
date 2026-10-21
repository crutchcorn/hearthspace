"""Battery and Suspend acceptance through real shell input on isolated peers."""

import json
import time
from pathlib import Path

from . import shell_checks as checks
from .control import ControlError
from .daily_checks import a11y, read_line
from .input import Input
from .map_checks import SHELL, selector


def visible(smoke, name, role=None):
    return a11y(smoke, "find", selector(name, role=role))["elements"]


def battery(smoke, percentage, state):
    smoke.until(
        f"battery {percentage}% {state}",
        lambda: checks.elements(smoke, f"Battery: {percentage}%, {state}"),
    )
    smoke.record("battery-visible", percentage=percentage, state=state)


def inbox(smoke):
    path = Path(smoke.manifest["runtime_dir"]) / "hearthspace-notifications.sock"
    response = json.loads(
        read_line(path, b'{"command":"list"}\n', smoke.control.deadline)
    )
    if not response.get("ok"):
        raise ControlError(f"notification query failed: {response}")
    return response["notifications"]


def warning(smoke, title, percentage, urgency):
    def received():
        matches = [item for item in inbox(smoke) if item["summary"] == title]
        if len(matches) > 1:
            raise ControlError(f"duplicate {title} notifications: {matches}")
        if not matches:
            return None
        item = matches[0]
        if item["urgency"] != urgency or not item["body"].startswith(
            f"{percentage}% remaining"
        ):
            raise ControlError(f"incorrect battery warning: {item}")
        return item

    item = smoke.until(f"delivered {title} notification", received)
    checks.click(smoke, "Notifications")
    checks.opened(smoke, "notifications")
    smoke.until(f"visible {title}", lambda: visible(smoke, title))
    smoke.until("visible warning details", lambda: visible(smoke, item["body"]))
    smoke.capture(title.lower().replace(" ", "-"))
    checks.click(smoke, "Close notifications", "notifications")
    checks.closed(smoke)
    smoke.record("battery-warning-visible", notification=item)
    return item["id"]


def no_duplicates(smoke, expected):
    # Keep observing after each changed reading has reached the shell, so a
    # warning posted just after the UI update cannot evade the assertion.
    end = time.monotonic() + 1.0
    while True:
        actual = {item["id"] for item in inbox(smoke)}
        if actual != set(expected):
            raise ControlError(
                f"unexpected battery notification IDs: {actual}; expected {expected}"
            )
        if time.monotonic() >= end:
            return
        time.sleep(min(0.05, smoke.control.deadline.remaining()))


def run(smoke):
    peer = smoke.service_fixture
    battery(smoke, 72, "Charging")
    checks.bar(smoke)
    smoke.capture("battery-charging")
    checks.click(smoke, "Battery: 72%, Charging", corner="system")
    checks.opened(smoke, "controls")
    smoke.until("battery dropdown", lambda: visible(smoke, "Battery", "panel"))
    for name in ("72%", "Charging"):
        query = (
            selector("Battery", role="panel")
            + f"//*[@showing='true' and @name='{name}']"
        )
        smoke.until(
            f"battery dropdown {name}",
            lambda query=query: a11y(smoke, "find", query)["elements"],
        )
    smoke.capture("battery-dropdown-detail")
    with Input(smoke.control) as inputs:
        inputs.chord("esc")
    checks.closed(smoke)

    peer.update(percentage=14.0, state=2, warning=3, on_battery=True)
    battery(smoke, 14, "Discharging")
    low = warning(smoke, "Low battery", 14, 1)
    peer.update(percentage=13.0)
    battery(smoke, 13, "Discharging")
    no_duplicates(smoke, [low])

    peer.update(percentage=4.0, warning=4)
    battery(smoke, 4, "Discharging")
    critical = warning(smoke, "Critical battery", 4, 2)
    peer.update(percentage=3.0)
    battery(smoke, 3, "Discharging")
    no_duplicates(smoke, [low, critical])
    peer.update(percentage=8.0, state=1, warning=1, on_battery=False)
    battery(smoke, 8, "Charging")
    no_duplicates(smoke, [low, critical])

    peer.update(present=False, kind=0, percentage=0.0, state=0)

    def no_battery():
        elements = a11y(
            smoke,
            "find",
            f"{SHELL}//*[@showing='true' and (starts-with(@name,'Battery:') or starts-with(@name,'UPS:') or @name='8%' or @name='0%')]",
        )["elements"]
        return not elements

    smoke.until("no battery indicator on a machine without a battery", no_battery)
    checks.bar(smoke)
    smoke.capture("desktop-without-battery")

    # A desktop can suspend even without a battery. Only the fake coordinator
    # listens at this runtime path; activating the real menu never sleeps CI.
    checks.click(smoke, "Power")
    checks.opened(smoke, "power")
    smoke.until(
        "Suspend on a battery-free desktop", lambda: checks.elements(smoke, "Suspend")
    )
    if not no_battery():
        raise ControlError("battery-free power menu retained a battery detail")
    if peer.power.suspend_events():
        raise ControlError("power menu sent Suspend before user activation")
    smoke.capture("desktop-suspend-option")
    checks.click(smoke, "Suspend", "power")
    events = smoke.until(
        "Suspend delivered to owned mock coordinator", peer.power.suspend_events
    )
    checks.closed(smoke)
    if len(events) != 1 or peer.power.suspend_events() != events:
        raise ControlError("one activation produced multiple suspend requests")
    if smoke.control.status().get("locked"):
        raise ControlError("mock Suspend unexpectedly locked the headless desktop")

    reason = "This test system does not support suspend."
    peer.power.unavailable(reason)
    checks.click(smoke, "Power")
    checks.opened(smoke, "power")
    smoke.until("unsupported suspend explanation", lambda: visible(smoke, reason))
    disabled = a11y(
        smoke, "find", selector("Suspend", role="button") + "[not(@focusable='true')]"
    )["elements"]
    if not disabled:
        raise ControlError("unsupported Suspend action is still focusable")
    checks.click(smoke, "Suspend", "power")
    checks.opened(smoke, "power")
    smoke.capture("suspend-unavailable")
    checks.click(smoke, "Close power menu", "power")
    checks.closed(smoke)
    if peer.power.suspend_events() != events:
        raise ControlError("disabled Suspend action reached the coordinator")
    smoke.capture("power-controls-complete")
    smoke.record(
        "power-controls-verified",
        low_id=low,
        critical_id=critical,
        notifications_deduplicated=True,
        absent_battery_hidden=True,
        suspend_without_battery=True,
        unsupported_suspend_disabled=True,
        suspend_requests=events,
    )
