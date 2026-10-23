"""Wait for committed Firefox geometry before targeting or verifying a restore."""

from . import firefox_assertions as verify
from .control import ControlError
from .visual import marker_bounds


def restored_for_resize(smoke):
    def ready():
        window = smoke.window(smoke.target)
        return (
            window
            if verify.restored_client_size(window, smoke.floating_before_max)
            else None
        )

    smoke.until("CSD restore commits saved floating client size", ready)
    smoke.capture("csd-restore-after")
    # Capturing can wait for presentation; target the edge from a fresh sample.
    return smoke.until("fresh restored Firefox geometry after capture", ready)


def fullscreen_exit(smoke, before, before_view):
    last_error = None

    def ready():
        nonlocal last_error
        status = smoke.control.status()
        after = next(
            (window for window in status["windows"] if window["id"] == smoke.target),
            None,
        )
        try:
            proof = verify.verify_fullscreen_exit(
                before_view, status["viewport"], before, after
            )
        except ControlError as error:
            last_error = str(error)
            return None
        return after, status["viewport"], proof

    try:
        smoke.until("F11 exit restores committed canvas geometry", ready)
        image = smoke.capture("fullscreen-exit")
        after, after_view, proof = smoke.until(
            "restored canvas geometry after F11 exit capture", ready
        )
        marker_bounds(image, after)
    except ControlError as error:
        raise ControlError(f"Firefox F11 exit: {last_error or error}") from error
    smoke.record(
        "fullscreen-exit-restored",
        before=before,
        after=after,
        before_viewport=before_view,
        after_viewport=after_view,
        **proof,
    )
