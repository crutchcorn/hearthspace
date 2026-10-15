"""Bounded keyboard unplug fixture restricted to an owned native test session."""

import argparse
import json
import os
import re
import signal
import subprocess
import sys
import time
from pathlib import Path

from .control import ControlError
from .touchpad import TouchDevice, require_owned_vt

ENTRY = Path(__file__).resolve().parents[1] / "window-switcher-smoke"


class KeyboardDevice(TouchDevice):
    """Reuse the native worker's deadline, framing and cleanup implementation."""

    def __enter__(self):
        self.process = subprocess.Popen(
            [
                "sudo",
                "-n",
                "/usr/bin/python3",
                str(ENTRY),
                "--keyboard",
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


def removal_proof(trace):
    alt = re.search(r"KEYBOARD_KEY[^\n]*KEY_LEFTALT[^\n]*pressed", trace)
    tab = re.search(r"KEYBOARD_KEY[^\n]*KEY_TAB[^\n]*pressed", trace)
    removed = trace.find("DEVICE_REMOVED")
    if alt is None or tab is None or not alt.start() < tab.start() < removed:
        raise ControlError("keyboard removal trace lacks Alt+Tab before device removal")
    release = re.search(r"KEYBOARD_KEY[^\n]*KEY_LEFTALT[^\n]*released", trace)
    return {
        "alt_tab_before_removal": True,
        "libinput_released_alt": release is not None and release.start() < removed,
    }


def worker(session):
    from evdev import UInput
    from evdev import ecodes as e

    if os.geteuid() != 0:
        raise ControlError("virtual keyboard requires sudo for /dev/uinput")
    require_owned_vt(session)

    def interrupted(*_):
        raise SystemExit("virtual keyboard stopped")

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGALRM, interrupted)
    signal.alarm(60)
    with UInput(
        {e.EV_KEY: list(range(1, 256))},
        name="Hearthspace window switcher keyboard acceptance",
        phys="hearthspace/window-switcher-smoke/keyboard",
    ) as device:
        subprocess.run(["udevadm", "settle", "--timeout=5"], timeout=6, check=True)
        properties = subprocess.check_output(
            ["udevadm", "info", "--query=property", f"--name={device.device.path}"],
            text=True,
            timeout=5,
        )
        if "ID_INPUT_KEYBOARD=1" not in properties.splitlines():
            raise ControlError("udev did not classify the virtual device as a keyboard")
        observer = subprocess.Popen(
            [
                "stdbuf",
                "-oL",
                "libinput",
                "debug-events",
                "--show-keycodes",
                "--device",
                device.device.path,
            ],
            stdout=sys.stderr,
            stderr=sys.stderr,
        )
        try:
            time.sleep(0.5)
            if observer.poll() is not None:
                raise ControlError("libinput keyboard observer failed to start")
            print(
                json.dumps(
                    {"ok": True, "device": device.device.path, "udev": properties}
                ),
                flush=True,
            )
            held = False
            for line in sys.stdin:
                require_owned_vt(session)
                operation = json.loads(line)["operation"]
                if operation != "hold-alt-tab" or held:
                    raise ControlError("keyboard fixture accepts one held Alt+Tab only")
                device.write(e.EV_KEY, e.KEY_LEFTALT, 1)
                device.syn()
                device.write(e.EV_KEY, e.KEY_TAB, 1)
                device.syn()
                device.write(e.EV_KEY, e.KEY_TAB, 0)
                device.syn()
                held = True
                print(json.dumps({"ok": True, "operation": operation}), flush=True)
        finally:
            try:
                # Deliberately unplug with Alt still held. No key-up is emitted
                # by the injector; libinput's own neutralization is recorded.
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
    parser.add_argument("--keyboard", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        worker(args.keyboard.resolve())
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
