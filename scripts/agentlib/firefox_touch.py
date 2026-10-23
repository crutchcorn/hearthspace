"""Owned Firefox touch input and cancellation observations."""

from .control import ControlError
from .pinch_assertions import same_viewport
from .pinch_touchscreen import settled
from .touchpad import TouchDevice


class DeadlineTouchDevice(TouchDevice):
    def __init__(self, session, log, deadline, kind):
        super().__init__(session, log, kind, deadline=deadline)


def pinch_cancelled(before, status):
    # A fourth contact rolls back canvas motion and can start a fresh switcher
    # candidate. That candidate keeps pinch_active true until contacts release.
    return (
        same_viewport(before, status["viewport"])
        and not status["window_switcher"]["active"]
    )


def released_viewport(smoke, before):
    # The device acknowledgement can precede compositor processing of the up.
    smoke.until(
        "released Firefox pinch restores the exact inactive viewport without a switcher",
        lambda: (
            not (status := smoke.control.status())["viewport"]["pinch_active"]
            and pinch_cancelled(before, status)
        ),
    )
    viewport = settled(smoke)
    status = smoke.control.status()
    if (
        viewport["pinch_active"]
        or not same_viewport(before, viewport)
        or status["viewport"]["pinch_active"]
        or not pinch_cancelled(before, status)
    ):
        raise ControlError(
            "cancelled Firefox pinch did not restore the exact inactive viewport "
            "without a switcher"
        )
    return status["viewport"]
