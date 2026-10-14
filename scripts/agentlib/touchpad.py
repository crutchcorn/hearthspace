"""A temporary uinput touchpad, restricted to an owned native test VT."""

import argparse
import json
import os
import selectors
import signal
import subprocess
import sys
import time
from pathlib import Path

from .control import ControlError

ENTRY = Path(__file__).resolve().parents[1] / "pinch-smoke"


def contact_positions(width, fingers):
    # A stationary contact below the two moving fingers is correctly classified
    # by libinput as a resting thumb. Keep the third contact above them and move
    # every contact during a three-finger pinch so its finger count stays three.
    return [
        (1000 - width / 2, 650),
        (1000 + width / 2, 650),
        (1000, 650 - width / 3),
    ][:fingers]


class Touchpad:
    def __init__(self, session, log):
        self.session = session
        self.log = log
        self.process = None

    def __enter__(self):
        self.process = subprocess.Popen(
            [
                "sudo",
                "-n",
                "/usr/bin/python3",
                str(ENTRY),
                "--touchpad",
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
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout, selectors.EVENT_READ)
            if not selector.select(15):
                raise ControlError("virtual touchpad timed out")
        line = self.process.stdout.readline()
        if not line:
            raise ControlError("virtual touchpad exited; inspect its retained log")
        result = json.loads(line)
        if not result.get("ok"):
            raise ControlError(result.get("error", "virtual touchpad failed"))
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
                f"virtual touchpad worker exited with status {returncode}"
            )


def require_owned_vt(session):
    manifest = json.loads((session / "session.json").read_text())
    active = Path("/sys/class/tty/tty0/active").read_text().strip()
    if manifest.get("status") != "ready" or active != f"tty{manifest['vt']}":
        raise ControlError("virtual touchpad requires its owned native VT to be active")
    return manifest


def worker(session):
    # Optional dependency: ordinary unit tests/headless runs never import evdev.
    from evdev import AbsInfo, UInput
    from evdev import ecodes as e

    if os.geteuid() != 0:
        raise ControlError("virtual touchpad requires sudo for /dev/uinput")
    require_owned_vt(session)

    def interrupted(*_):
        raise SystemExit("virtual touchpad stopped")

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGALRM, interrupted)
    signal.alarm(180)
    axes = [
        (e.ABS_X, AbsInfo(0, 0, 2000, 0, 0, 20)),
        (e.ABS_Y, AbsInfo(0, 0, 1400, 0, 0, 20)),
        (e.ABS_MT_POSITION_X, AbsInfo(0, 0, 2000, 0, 0, 20)),
        (e.ABS_MT_POSITION_Y, AbsInfo(0, 0, 1400, 0, 0, 20)),
        (e.ABS_MT_SLOT, AbsInfo(0, 0, 2, 0, 0, 0)),
        (e.ABS_MT_TRACKING_ID, AbsInfo(0, 0, 65535, 0, 0, 0)),
    ]
    counts = [e.BTN_TOOL_FINGER, e.BTN_TOOL_DOUBLETAP, e.BTN_TOOL_TRIPLETAP]
    caps = {e.EV_KEY: [e.BTN_LEFT, e.BTN_TOUCH, *counts], e.EV_ABS: axes}
    with UInput(
        caps,
        name="Hearthspace pinch acceptance touchpad",
        phys="hearthspace/pinch-smoke",
        input_props=[e.INPUT_PROP_POINTER, e.INPUT_PROP_BUTTONPAD],
    ) as device:
        subprocess.run(["udevadm", "settle", "--timeout=5"], timeout=6, check=True)
        properties = subprocess.check_output(
            ["udevadm", "info", "--query=property", f"--name={device.device.path}"],
            text=True,
            timeout=5,
        )
        if "ID_INPUT_TOUCHPAD=1" not in properties.splitlines():
            raise ControlError("udev did not classify the virtual device as a touchpad")
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

            def positions(width):
                points = contact_positions(width, fingers)
                for slot, (x, y) in enumerate(points):
                    device.write(e.EV_ABS, e.ABS_MT_SLOT, slot)
                    device.write(e.EV_ABS, e.ABS_MT_POSITION_X, round(x))
                    device.write(e.EV_ABS, e.ABS_MT_POSITION_Y, round(y))
                device.write(e.EV_ABS, e.ABS_X, round(points[0][0]))
                device.write(e.EV_ABS, e.ABS_Y, points[0][1])

            for line in sys.stdin:
                require_owned_vt(session)
                request = json.loads(line)
                operation = request["operation"]
                if operation == "begin":
                    if fingers or request["fingers"] not in (2, 3):
                        raise ControlError("invalid touchpad begin")
                    fingers = request["fingers"]
                    distance = float(request["distance"])
                    if not 200 <= distance <= 1600:
                        raise ControlError("touchpad distance must be in [200, 1600]")
                    for slot in range(fingers):
                        tracking += 1
                        device.write(e.EV_ABS, e.ABS_MT_SLOT, slot)
                        device.write(e.EV_ABS, e.ABS_MT_TRACKING_ID, tracking)
                    device.write(e.EV_KEY, e.BTN_TOUCH, 1)
                    device.write(e.EV_KEY, counts[fingers - 1], 1)
                    positions(distance)
                    device.syn()
                    time.sleep(0.06)
                elif operation == "move":
                    target = float(request["distance"])
                    if not fingers or not 200 <= target <= 1600:
                        raise ControlError("invalid touchpad move")
                    for step in range(1, 41):
                        require_owned_vt(session)
                        positions(distance + (target - distance) * step / 40)
                        device.syn()
                        time.sleep(0.01)
                    distance = target
                elif operation == "add-finger":
                    if fingers != 2:
                        raise ControlError(
                            "adding a finger requires two active contacts"
                        )
                    fingers = 3
                    tracking += 1
                    device.write(e.EV_ABS, e.ABS_MT_SLOT, 2)
                    device.write(e.EV_ABS, e.ABS_MT_TRACKING_ID, tracking)
                    device.write(e.EV_KEY, e.BTN_TOOL_DOUBLETAP, 0)
                    device.write(e.EV_KEY, e.BTN_TOOL_TRIPLETAP, 1)
                    positions(distance)
                    device.syn()
                    time.sleep(0.1)
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
    parser.add_argument("--touchpad", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        worker(args.touchpad.resolve())
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
