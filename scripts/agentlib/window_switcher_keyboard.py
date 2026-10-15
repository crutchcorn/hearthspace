"""Native keyboard removal while Alt holds a live switcher preview."""

import shlex
from pathlib import Path

from .control import ControlError
from .input import Input
from .keyboard_device import KeyboardDevice, removal_proof
from .pinch_assertions import same_viewport
from .window_switcher_smoke_assertions import previews, selection
from .window_switcher_smoke_checks import prepare, settled


def unplug(smoke):
    before = prepare(smoke)
    middle, last = list(smoke.targets)[1:]
    path = smoke.artifacts / "libinput-keyboard-unplug.log"
    with (
        path.open("w") as log,
        KeyboardDevice(smoke.directory, log, deadline=smoke.control.deadline) as device,
    ):
        smoke.record("virtual-switcher-keyboard", **device.device)
        device.request("hold-alt-tab")
        smoke.until(
            "kernel keyboard holds switcher",
            lambda: smoke.control.status()["window_switcher"]["active"],
        )
        held = settled(smoke)
        selection(held, last, active=True)
        if held["window_switcher"]["source"] != "keyboard":
            raise ControlError("kernel Alt+Tab did not own the switcher")
        previews(held, smoke.capture("keyboard-unplug-held"), smoke.targets)
    if Path(device.device["device"]).exists():
        raise ControlError("owned keyboard survived unplug cleanup")
    proof = removal_proof(path.read_text())
    smoke.until(
        "removed keyboard cancels switcher",
        lambda: not smoke.control.status()["window_switcher"]["active"],
    )
    after = settled(smoke)
    # Some libinput versions neutralize held keys before DEVICE_REMOVED. That
    # genuine Alt release commits normally; removal with no release cancels.
    expected, target = (
        (held, last) if proof["libinput_released_alt"] else (before, middle)
    )
    selection(after, target, active=False)
    if not same_viewport(expected["viewport"], after["viewport"]):
        raise ControlError(
            "keyboard removal viewport disagrees with backend release ordering"
        )
    smoke.capture("keyboard-unplug-finished")
    witness = smoke.artifacts / "keyboard-unplug-typing.txt"
    text = "keyboard_modifiers_recovered"
    with Input(smoke.control) as inputs:
        inputs.type(f"printf '{text}' > {shlex.quote(str(witness))}\n")
    smoke.until(
        "post-unplug typing reaches the restored client",
        lambda: witness.exists() and witness.read_text() == text,
    )
    smoke.capture("keyboard-unplug-typing-recovered")
    smoke.record(
        "switcher-keyboard-unplug-verified",
        trace=str(path),
        device_removed=True,
        typing_witness=str(witness),
        before=before,
        after=after,
        **proof,
    )
