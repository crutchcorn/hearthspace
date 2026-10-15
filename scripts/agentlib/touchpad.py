"""Temporary uinput touch devices, restricted to an owned native test VT."""

import argparse
import json
import math
import os
import selectors
import signal
import subprocess
import sys
import time
from pathlib import Path

from .control import ControlError

ENTRY = Path(__file__).resolve().parents[1] / "pinch-smoke"


def contact_positions(width, fingers, translation=(0, 0)):
    # A stationary contact below the two moving fingers is correctly classified
    # by libinput as a resting thumb. Keep the third contact above them and move
    # every contact during a three-finger pinch so its finger count stays three.
    if fingers not in range(1, 6):
        raise ControlError("touchpad requires 1–5 contacts")
    if len(translation) != 2 or not all(math.isfinite(value) for value in translation):
        raise ControlError("touchpad translation must contain two finite coordinates")
    points = [
        (1000 - width / 2, 650),
        (1000 + width / 2, 650),
        (1000, 650 - width / 3),
        (1000 - width / 4, 650 - width / 6),
        (1000 + width / 4, 650 - width / 6),
    ][:fingers]
    return [(x + translation[0], y + translation[1]) for x, y in points]


def checked_points(values):
    if not 1 <= len(values) <= 5 or any(
        len(point) != 2 or not 0 <= point[0] <= 2000 or not 0 <= point[1] <= 1400
        for point in values
    ):
        raise ControlError("touchscreen requires 1–5 points within its absolute range")
    return [[round(x), round(y)] for x, y in values]


class TouchDevice:
    def __init__(self, session, log, kind="touchpad", deadline=None):
        self.session = session
        self.log = log
        self.kind = kind
        self.process = None
        self.deadline = deadline

    def __enter__(self):
        self.process = subprocess.Popen(
            [
                "sudo",
                "-n",
                "/usr/bin/python3",
                str(ENTRY),
                f"--{self.kind}",
                str(self.session),
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.log,
            text=True,
        )
        try:
            self.device = self.receive()
            return self
        except BaseException:
            self.__exit__(*sys.exc_info())
            raise

    def receive(self):
        end = time.monotonic() + 15
        data = bytearray()
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout, selectors.EVENT_READ)
            while not data.endswith(b"\n"):
                remaining = end - time.monotonic()
                if self.deadline is not None:
                    remaining = min(remaining, self.deadline.remaining())
                if remaining <= 0 or not selector.select(remaining):
                    raise ControlError("virtual touch device timed out")
                byte = os.read(self.process.stdout.fileno(), 1)
                if not byte:
                    raise ControlError("virtual touch device exited before reply")
                data.extend(byte)
                if len(data) > 65536:
                    raise ControlError("oversized virtual touch device reply")
        result = json.loads(data)
        if not result.get("ok"):
            raise ControlError(result.get("error", "virtual touch device failed"))
        return result

    def request(self, operation, **values):
        self.process.stdin.write(json.dumps({"operation": operation, **values}) + "\n")
        self.process.stdin.flush()
        return self.receive()

    def __exit__(self, error_type=None, *_):
        if self.process is None:
            return
        try:
            self.process.stdin.close()
        except BrokenPipeError:
            pass
        try:
            returncode = self.process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            subprocess.run(
                ["sudo", "-n", "kill", "-TERM", str(self.process.pid)],
                timeout=5,
                check=True,
            )
            returncode = self.process.wait(timeout=5)
        finally:
            self.process.stdout.close()
        if returncode and error_type is None:
            raise ControlError(
                f"virtual touch device worker exited with status {returncode}"
            )


def require_owned_vt(session):
    manifest = json.loads((session / "session.json").read_text())
    active = Path("/sys/class/tty/tty0/active").read_text().strip()
    if manifest.get("status") != "ready" or active != f"tty{manifest['vt']}":
        raise ControlError(
            "virtual touch device requires its owned native VT to be active"
        )
    return manifest


def worker(session, kind):
    # Optional dependency: ordinary unit tests/headless runs never import evdev.
    from evdev import AbsInfo, UInput
    from evdev import ecodes as e

    if os.geteuid() != 0:
        raise ControlError("virtual touch device requires sudo for /dev/uinput")
    require_owned_vt(session)

    def interrupted(*_):
        raise SystemExit("virtual touch device stopped")

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGALRM, interrupted)
    signal.alarm(180)
    axes = [
        (e.ABS_X, AbsInfo(0, 0, 2000, 0, 0, 20)),
        (e.ABS_Y, AbsInfo(0, 0, 1400, 0, 0, 20)),
        (e.ABS_MT_POSITION_X, AbsInfo(0, 0, 2000, 0, 0, 20)),
        (e.ABS_MT_POSITION_Y, AbsInfo(0, 0, 1400, 0, 0, 20)),
        (e.ABS_MT_SLOT, AbsInfo(0, 0, 4, 0, 0, 0)),
        (e.ABS_MT_TRACKING_ID, AbsInfo(0, 0, 65535, 0, 0, 0)),
    ]
    counts = (
        [
            e.BTN_TOOL_FINGER,
            e.BTN_TOOL_DOUBLETAP,
            e.BTN_TOOL_TRIPLETAP,
            e.BTN_TOOL_QUADTAP,
            e.BTN_TOOL_QUINTTAP,
        ]
        if kind == "touchpad"
        else []
    )
    keys = [e.BTN_TOUCH, e.BTN_LEFT, *counts] if counts else [e.BTN_TOUCH]
    props = (
        [e.INPUT_PROP_POINTER, e.INPUT_PROP_BUTTONPAD]
        if counts
        else [e.INPUT_PROP_DIRECT]
    )
    caps = {e.EV_KEY: keys, e.EV_ABS: axes}
    with UInput(
        caps,
        name=f"Hearthspace pinch acceptance {kind}",
        phys=f"hearthspace/pinch-smoke/{kind}",
        input_props=props,
    ) as device:
        subprocess.run(["udevadm", "settle", "--timeout=5"], timeout=6, check=True)
        properties = subprocess.check_output(
            ["udevadm", "info", "--query=property", f"--name={device.device.path}"],
            text=True,
            timeout=5,
        )
        if f"ID_INPUT_{kind.upper()}=1" not in properties.splitlines():
            raise ControlError(f"udev did not classify the virtual device as a {kind}")
        observer = subprocess.Popen(
            [
                "stdbuf",
                "-oL",
                "libinput",
                "debug-events",
                "--device",
                device.device.path,
            ],
            stdout=sys.stderr,
            stderr=sys.stderr,
        )
        try:
            # Allow both libinput contexts to discover the newly added device.
            time.sleep(0.5)
            if observer.poll() is not None:
                raise ControlError("libinput diagnostic observer failed to start")
            print(
                json.dumps(
                    {"ok": True, "device": device.device.path, "udev": properties}
                ),
                flush=True,
            )
            fingers, distance, tracking = 0, 0, 0
            touch_points = []
            translation = (0, 0)

            def positions(width, offset):
                points = (
                    contact_positions(width, fingers, offset)
                    if counts
                    else touch_points
                )
                for slot, (x, y) in enumerate(points):
                    device.write(e.EV_ABS, e.ABS_MT_SLOT, slot)
                    device.write(e.EV_ABS, e.ABS_MT_POSITION_X, round(x))
                    device.write(e.EV_ABS, e.ABS_MT_POSITION_Y, round(y))
                device.write(e.EV_ABS, e.ABS_X, round(points[0][0]))
                device.write(e.EV_ABS, e.ABS_Y, round(points[0][1]))

            for line in sys.stdin:
                require_owned_vt(session)
                request = json.loads(line)
                operation = request["operation"]
                if operation == "begin":
                    if fingers or request["fingers"] not in (1, 2, 3, 4, 5):
                        raise ControlError("invalid touchpad begin")
                    fingers = request["fingers"]
                    if counts:
                        distance = float(request["distance"])
                        if not 200 <= distance <= 1600:
                            raise ControlError(
                                "touchpad distance must be in [200, 1600]"
                            )
                        translation = request.get("translation", (0, 0))
                        checked_points(
                            contact_positions(distance, fingers, translation)
                        )
                    else:
                        touch_points = checked_points(request["points"])
                        if len(touch_points) != fingers:
                            raise ControlError(
                                "touchscreen finger count and positions differ"
                            )
                    for slot in range(fingers):
                        tracking += 1
                        device.write(e.EV_ABS, e.ABS_MT_SLOT, slot)
                        device.write(e.EV_ABS, e.ABS_MT_TRACKING_ID, tracking)
                    device.write(e.EV_KEY, e.BTN_TOUCH, 1)
                    if counts:
                        device.write(e.EV_KEY, counts[fingers - 1], 1)
                    positions(distance, translation)
                    device.syn()
                    time.sleep(0.06 if counts else 0.02)
                elif operation == "move":
                    if not fingers:
                        raise ControlError("cannot move without active contacts")
                    target = float(request["distance"]) if counts else distance
                    old_points = touch_points
                    if counts:
                        if not 200 <= target <= 1600:
                            raise ControlError("invalid touchpad move")
                        old_translation = translation
                        target_translation = request.get("translation", translation)
                        checked_points(
                            contact_positions(target, fingers, target_translation)
                        )
                    else:
                        new_points = checked_points(request["points"])
                        if len(new_points) != fingers:
                            raise ControlError(
                                "touchscreen move must retain its contact count"
                            )
                    for step in range(1, 41):
                        require_owned_vt(session)
                        if not counts:
                            touch_points = [
                                [a + (b - a) * step / 40 for a, b in zip(old, new)]
                                for old, new in zip(old_points, new_points)
                            ]
                        else:
                            translation = [
                                old + (new - old) * step / 40
                                for old, new in zip(old_translation, target_translation)
                            ]
                        positions(
                            distance + (target - distance) * step / 40, translation
                        )
                        device.syn()
                        time.sleep(0.01)
                    distance = target
                elif operation == "add-finger":
                    if not 1 <= fingers <= 4 or (counts and fingers not in (3, 4)):
                        raise ControlError("invalid contact count when adding a finger")
                    if not counts:
                        touch_points = checked_points([*touch_points, request["point"]])
                    fingers += 1
                    tracking += 1
                    device.write(e.EV_ABS, e.ABS_MT_SLOT, fingers - 1)
                    device.write(e.EV_ABS, e.ABS_MT_TRACKING_ID, tracking)
                    if counts:
                        device.write(e.EV_KEY, counts[fingers - 2], 0)
                        device.write(e.EV_KEY, counts[fingers - 1], 1)
                    positions(distance, translation)
                    device.syn()
                    # libinput debounces finger-count changes for 100 ms. Leave
                    # time for both readers to dispatch cancellation before an
                    # immediately following end lifts all remaining contacts.
                    time.sleep(0.2 if counts else 0.02)
                elif operation == "lift":
                    if not fingers:
                        raise ControlError("individual lift requires active contacts")
                    if counts:
                        device.write(e.EV_KEY, counts[fingers - 1], 0)
                    fingers -= 1
                    device.write(e.EV_ABS, e.ABS_MT_SLOT, fingers)
                    device.write(e.EV_ABS, e.ABS_MT_TRACKING_ID, -1)
                    if counts and fingers:
                        device.write(e.EV_KEY, counts[fingers - 1], 1)
                    if not counts:
                        touch_points.pop()
                    if fingers:
                        positions(distance, translation)
                    else:
                        device.write(e.EV_KEY, e.BTN_TOUCH, 0)
                    device.syn()
                    if counts:
                        time.sleep(0.2)
                elif operation == "end":
                    for slot in range(fingers):
                        device.write(e.EV_ABS, e.ABS_MT_SLOT, slot)
                        device.write(e.EV_ABS, e.ABS_MT_TRACKING_ID, -1)
                    device.write(e.EV_KEY, e.BTN_TOUCH, 0)
                    for code in counts:
                        device.write(e.EV_KEY, code, 0)
                    fingers = 0
                    device.syn()
                else:
                    raise ControlError("unknown touchpad operation")
                print(json.dumps({"ok": True, "operation": operation}), flush=True)
        finally:
            try:
                # Keep the independent libinput reader alive long enough to
                # retain the actual END/DEVICE_REMOVED ordering on unplug.
                device.close()
                time.sleep(0.25)
            finally:
                observer.terminate()
                try:
                    observer.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    observer.kill()
                    observer.wait(timeout=5)


def worker_main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--touchpad", type=Path)
    group.add_argument("--touchscreen", type=Path)
    args = parser.parse_args(argv)
    try:
        kind = "touchpad" if args.touchpad else "touchscreen"
        worker(getattr(args, kind).resolve(), kind)
        return 0
    except (
        ControlError,
        OSError,
        ValueError,
        KeyError,
        subprocess.SubprocessError,
    ) as error:
        print(json.dumps({"ok": False, "error": str(error)}), flush=True)
        return 1
