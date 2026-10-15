"""Lock acknowledgement requires a confirmed secure presentation, never a capture."""

import time


def lock_session(control, wait="locked"):
    result = {"ok": True, "command": "lock", "accepted": True, "verification": wait}
    status = control.status()
    if status.get("locked") is True and status.get("lock_confirmed") is True:
        return {**result, "locked": True, "lock_confirmed": True}
    if status.get("locked") is not True:
        control.request("lock-session")
    if wait == "none":
        return result
    while True:
        status = control.status()
        if status.get("locked") is True and status.get("lock_confirmed") is True:
            return {**result, "locked": True, "lock_confirmed": True}
        time.sleep(min(0.05, control.deadline.remaining()))
