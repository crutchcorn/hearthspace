"""Independent libinput gesture deltas retained by native input workers."""

import re

from .control import ControlError

NUMBER = r"[+-]?(?:\d+(?:\.\d*)?|\.\d+)"
EVENT = re.compile(
    r"^\s*\S+\s+GESTURE_(SWIPE|PINCH)_(BEGIN|UPDATE|END)"
    r"(?:\s+\d+)?\s+\+\S+s\s+(\d+)\s*(.*)$"
)
DELTA = re.compile(rf"^({NUMBER})/\s*({NUMBER})\s+\(")
SCALE = re.compile(rf"\)\s+({NUMBER})\s+@")


def gesture(trace, kind, fingers=3, *, require_end=False):
    """Read the last matching gesture; deltas are accelerated logical units."""
    current, matching = None, None
    for line in trace.splitlines():
        match = EVENT.match(line)
        if match is None:
            continue
        event_kind, phase, count, detail = match.groups()
        if phase == "BEGIN":
            current = {
                "kind": event_kind,
                "fingers": int(count),
                "translation": [0.0, 0.0],
                "scale": 1.0,
                "updates": 0,
                "ended": False,
                "cancelled": False,
            }
            if event_kind == kind and int(count) == fingers:
                matching = current
        elif current is not None and (event_kind, int(count)) == (
            current["kind"],
            current["fingers"],
        ):
            if phase == "END":
                current["ended"] = True
                current["cancelled"] = "cancelled" in detail
                current = None
            elif phase == "UPDATE":
                delta = DELTA.match(detail)
                scale = SCALE.search(detail) if event_kind == "PINCH" else None
                if delta is None or (event_kind == "PINCH" and scale is None):
                    raise ControlError("unrecognized libinput gesture update")
                current["translation"] = [
                    total + float(value)
                    for total, value in zip(current["translation"], delta.groups())
                ]
                if scale:
                    current["scale"] = float(scale.group(1))
                current["updates"] += 1
    if matching is None or matching["updates"] == 0:
        raise ControlError(f"trace lacks an updated {fingers}-finger {kind}")
    if require_end and not matching["ended"]:
        raise ControlError("trace lacks the gesture ending")
    return matching
