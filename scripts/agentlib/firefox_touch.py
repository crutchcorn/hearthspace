"""Use the existing owned TouchDevice worker within the Firefox step deadline."""

import json
import os
import selectors
import time

from .control import ControlError
from .touchpad import TouchDevice


class DeadlineTouchDevice(TouchDevice):
    def __init__(self, session, log, deadline, kind):
        super().__init__(session, log, kind)
        self.deadline = deadline

    def receive(self):
        end = time.monotonic() + min(15, self.deadline.remaining())
        data = bytearray()
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout, selectors.EVENT_READ)
            while not data.endswith(b"\n"):
                remaining = min(end - time.monotonic(), self.deadline.remaining())
                if remaining <= 0 or not selector.select(remaining):
                    raise ControlError("virtual Firefox touch device timed out")
                byte = os.read(self.process.stdout.fileno(), 1)
                if not byte:
                    raise ControlError(
                        "virtual Firefox touch device exited before reply"
                    )
                data.extend(byte)
                if len(data) > 65536:
                    raise ControlError("oversized virtual touch device reply")
        result = json.loads(data)
        if not result.get("ok"):
            raise ControlError(result.get("error", "virtual touch device failed"))
        return result
