"""Control center, real slider input and passive volume OSD acceptance."""

import shlex
import time
from pathlib import Path

from . import shell_checks as checks
from .control import ControlError
from .daily_checks import a11y, focused, terminal
from .input import Input
from .map_checks import button_center, selector


def visible(smoke, name, role=None):
    return a11y(smoke, "find", selector(name, role=role))["elements"]


def slider(smoke, name, *, disabled=False):
    target = selector(name, role="slider")
    if disabled:
        target += "[not(@focusable='true')]"
    return smoke.until(
        f"{'disabled ' if disabled else ''}{name} slider",
        lambda: a11y(smoke, "find", target)["elements"],
    )[0]


def slider_point(smoke, element):
    status = smoke.control.status()
    if len(status["outputs"]) != 1:
        raise ControlError("control acceptance requires one output")
    return button_center(
        element.get("bounds"), status["outputs"][0], checks.opened(smoke, "controls")
    )


def audio(smoke, *, volume=None, muted=None):
    def matches():
        state = smoke.service_fixture.status()
        return (
            state
            if (
                (volume is None or state["volume"] == volume)
                and (muted is None or state["muted"] == muted)
            )
            else None
        )

    return smoke.until(f"audio {volume}% muted={muted}", matches)


def balanced_channels(state):
    left, right = state["raw_volumes"]
    if right <= 0 or abs(left / right - 0.5) > 0.001:
        raise ControlError(
            "master volume adjustment changed the fixture's channel balance"
        )


def settled_audio(smoke):
    """Wait for the coalesced tail of a real drag before pressing an arrow."""
    previous = smoke.service_fixture.status()
    quiet_since = time.monotonic()
    while time.monotonic() - quiet_since < 0.3:
        time.sleep(min(0.03, smoke.control.deadline.remaining()))
        state = smoke.service_fixture.status()
        if state != previous:
            previous, quiet_since = state, time.monotonic()
    return previous


def osd(smoke, text, *, capture=None):
    smoke.until(
        "mapped passive control OSD", lambda: smoke.control.status().get("control_osds")
    )
    smoke.until(f"OSD {text}", lambda: visible(smoke, text))
    status = smoke.control.status()
    if len(status["control_osds"]) != 1:
        raise ControlError("one adjustment must show exactly one OSD")
    if status.get("shell_panels"):
        raise ControlError("OSD unexpectedly opened an interactive shell panel")
    if not focused(smoke, smoke.target):
        raise ControlError("volume OSD stole focus from the application")
    if capture:
        smoke.capture(capture)
    smoke.record(
        "control-osd", text=text, focus=smoke.target, geometry=status["control_osds"]
    )


def dismissed(smoke):
    start = time.monotonic()
    while smoke.control.status().get("control_osds"):
        if time.monotonic() - start > 3:
            raise ControlError("volume OSD did not dismiss automatically")
        time.sleep(min(0.03, smoke.control.deadline.remaining()))
    if not focused(smoke, smoke.target):
        raise ControlError("OSD dismissal changed application focus")
    smoke.record("osd-automatically-dismissed", elapsed=time.monotonic() - start)


def run(smoke):
    peer = smoke.service_fixture
    terminal(smoke)
    checks.click(smoke, "Control center")
    checks.opened(smoke, "controls")
    smoke.until("initial audio level", lambda: visible(smoke, "40%"))
    smoke.until("default output name", lambda: visible(smoke, "Fixture speakers"))
    smoke.until(
        "native-only brightness explanation",
        lambda: visible(
            smoke, "Brightness is available in a native Hearthspace session."
        ),
    )
    brightness = slider(smoke, "Brightness", disabled=True)
    volume = slider(smoke, "Volume")
    smoke.capture("control-center-initial")

    before = peer.mutations()
    with Input(smoke.control) as inputs:
        inputs.click(*slider_point(smoke, brightness))
    if peer.mutations() != before:
        raise ControlError("disabled brightness slider changed audio")
    start = slider_point(smoke, volume)
    scale = smoke.control.status()["outputs"][0]["scale"]
    end = (start[0] + volume["bounds"]["width"] / scale / 4, start[1])
    with Input(smoke.control) as inputs:
        inputs.drag(*start, *end, duration=0.2)
    smoke.until(
        "volume slider drag",
        lambda: state if 60 < (state := peer.status())["volume"] < 90 else None,
    )
    changed = settled_audio(smoke)
    smoke.until("dragged volume label", lambda: visible(smoke, f"{changed['volume']}%"))
    with Input(smoke.control) as inputs:
        inputs.chord("right")
    after = audio(smoke, volume=changed["volume"] + 1, muted=False)
    balanced_channels(after)
    smoke.until(
        "slider keyboard readback OSD",
        lambda: visible(smoke, f"Volume {after['volume']}%"),
    )
    smoke.capture("control-center-slider-input")
    smoke.record(
        "volume-slider-input",
        pointer=changed["volume"],
        keyboard=after["volume"],
        raw_channels=after["raw_volumes"],
    )
    checks.click(smoke, "Mute", "controls")
    audio(smoke, muted=True)
    smoke.until("muted control", lambda: visible(smoke, "Unmute", "button"))
    smoke.capture("control-center-muted")
    checks.click(smoke, "Unmute", "controls")
    audio(smoke, muted=False)

    peer.update(volume=40)
    smoke.until("external volume change reflected", lambda: visible(smoke, "40%"))
    checks.click(smoke, "Close control center", "controls")
    checks.closed(smoke)
    smoke.until("application focus restored", lambda: focused(smoke, smoke.target))
    dismissed(smoke)

    with Input(smoke.control) as inputs:
        inputs.chord("volume-up")
    balanced_channels(audio(smoke, volume=45, muted=False))
    osd(smoke, "Volume 45%", capture="volume-up-osd")
    marker = Path(smoke.manifest["runtime_dir"]) / "osd-input"
    with Input(smoke.control) as inputs:
        inputs.type(f"printf focused > {shlex.quote(str(marker))}\n")
    smoke.until(
        "terminal receives input while OSD visible",
        lambda: marker.exists() and marker.read_text() == "focused",
    )
    dismissed(smoke)
    with Input(smoke.control) as inputs:
        inputs.chord("volume-down")
    audio(smoke, volume=40)
    osd(smoke, "Volume 40%", capture="volume-down-osd")
    with Input(smoke.control) as inputs:
        inputs.chord("volume-mute")
    audio(smoke, muted=True)
    osd(smoke, "Volume muted", capture="volume-muted-osd")
    with Input(smoke.control) as inputs:
        inputs.chord("volume-up")
    audio(smoke, volume=45, muted=False)
    osd(smoke, "Volume 45%")
    dismissed(smoke)
    before = peer.mutations()
    with Input(smoke.control) as inputs:
        inputs.chord("brightness-up")
    osd(smoke, "Brightness unavailable", capture="brightness-unavailable-osd")
    dismissed(smoke)
    if peer.mutations() != before:
        raise ControlError("brightness key changed audio")

    peer.update(present=False)
    checks.click(smoke, "Control center")
    checks.opened(smoke, "controls")
    smoke.until(
        "missing output explanation",
        lambda: visible(smoke, "No audio output is available."),
    )
    missing = slider(smoke, "Volume", disabled=True)
    before = peer.mutations()
    with Input(smoke.control) as inputs:
        inputs.click(*slider_point(smoke, missing))
    smoke.capture("control-center-no-audio-output")
    if peer.mutations() != before:
        raise ControlError("disabled volume slider sent a mutation")
    checks.click(smoke, "Close control center", "controls")
    checks.closed(smoke)
    with Input(smoke.control) as inputs:
        inputs.chord("volume-up")
    osd(smoke, "Volume unavailable", capture="missing-audio-output-osd")
    dismissed(smoke)
    if peer.mutations() != before:
        raise ControlError("missing audio output received a mutation")
    peer.update(
        present=True,
        default_sink="fixture.new",
        description="Fixture headphones",
        volume=25,
    )
    checks.click(smoke, "Control center")
    checks.opened(smoke, "controls")
    smoke.until(
        "replacement default audio output", lambda: visible(smoke, "Fixture headphones")
    )
    smoke.until("replacement default volume", lambda: visible(smoke, "25%"))
    checks.click(smoke, "Close control center", "controls")
    checks.closed(smoke)
    with Input(smoke.control) as inputs:
        inputs.chord("volume-up")
    audio(smoke, volume=30)
    osd(smoke, "Volume 30%", capture="replacement-audio-output-osd")
    if peer.mutations()[-1]["argv"][2] != "fixture.new":
        raise ControlError("adjustment retained the previous default output")
    dismissed(smoke)
    smoke.record(
        "control-center-verified",
        slider_pointer=True,
        slider_keyboard=True,
        native_brightness_disabled=True,
        media_keys=True,
        mute=True,
        osd_focus_preserved=True,
        missing_audio_disabled=True,
        default_output_recovered=True,
        channel_balance_preserved=True,
    )
