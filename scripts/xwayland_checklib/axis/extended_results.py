"""Ordered ingress oracle: protocol distance checks and explicit revision-4 policy.

Expected records are literal outcomes, not a simulation of candidate state.
The original 42-case oracle is intentionally a separate, unchanged module.
"""

import json
import math
from pathlib import Path


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def scroll(**axes):
    return {
        "type": "scroll",
        "button": 0,
        "flags": 2,
        "h": axes.get("h"),
        "v": axes.get("v"),
    }


DOWN = {"type": "down", "button": 1, "flags": 0, "h": None, "v": None}
UP = {"type": "up", "button": 1, "flags": 0, "h": None, "v": None}
POLICY = "representation selection and pre-button ingress order"
DISTANCE = "relative distance accumulation"
STOP = "zero-stop ingress and representation selection"


def single_expectations(axis):
    def s(value):
        return scroll(**{axis: value})

    return {
        "v120_before_axis_same_flush": (POLICY, [[s(-0.25)]]),
        "axis_before_v120_same_flush": (POLICY, [[s(-0.25)]]),
        "v120_before_axis_button": (POLICY, [[s(-0.25), DOWN, UP]]),
        # This is a declared precision tradeoff, not a Wayland requirement.
        "axis_before_v120_button": (POLICY, [[s(-2), DOWN, UP]]),
        "between_buttons_v120_first": (POLICY, [[DOWN, s(0.75), UP]]),
        "between_buttons_axis_first": (POLICY, [[DOWN, s(0.75), UP]]),
        "v120_multiple_before_flush": (DISTANCE, [[s(0.75)]]),
        "v120_multiple_across_flush": (POLICY, [[s(0.25), DOWN, s(0.5), UP]]),
        "axis_multiple_before_flush": (DISTANCE, [[s(2)]]),
        "axis_multiple_across_flush_late_v120": (POLICY, [[s(0.5), DOWN, s(1.5), UP]]),
        "stop_before_late_v120": (STOP, [[s(0), DOWN, s(0.25), UP]]),
        "frame_reset": (
            POLICY,
            [
                [s(0.25), DOWN, UP],
                [s(-0.5), DOWN, UP],
                [s(0.5)],
                [s(-1.5)],
                [s(0)],
                [],
                [s(0.75)],
            ],
        ),
    }


EXPECTED = {
    f"{name}_{direction}": specification
    for axis, direction in (("v", "vertical"), ("h", "horizontal"))
    for name, specification in single_expectations(axis).items()
}
EXPECTED.update(
    {
        "dual_order_same_flush": (POLICY, [[scroll(h=0.5, v=-0.25)]]),
        "dual_independent_choices": (
            POLICY,
            [
                [scroll(h=0.5, v=-2), DOWN, UP],
                [scroll(h=2, v=-0.5), DOWN, UP],
            ],
        ),
        "dual_stop_other_axis": (
            STOP,
            [
                [scroll(h=1.5, v=0), DOWN, scroll(v=-0.25), UP],
                [scroll(h=0, v=-1.5), DOWN, scroll(h=0.5), UP],
            ],
        ),
        "dual_multiple_before_flush": (DISTANCE, [[scroll(h=-1.5, v=2)]]),
    }
)


def load_cases(path):
    path = Path(path)
    require(path.stat().st_size <= 1024 * 1024, "extended trace exceeds 1 MiB")
    cases, current = {}, None
    for line in path.read_text().splitlines():
        row = json.loads(line)
        if row["kind"] == "begin":
            require(current is None, "nested extended case")
            key = (row["name"], row["version"], row["gate"])
            require(key not in cases, "duplicate extended case")
            current = {"key": key, "events": []}
        elif row["kind"] == "event":
            require(current is not None, "extended event outside case")
            current["events"].append(row)
        elif row["kind"] == "end":
            require(current is not None, "extended end outside case")
            current["end"] = row
            cases[current["key"]] = current
            current = None
        else:
            raise RuntimeError("unknown extended trace record")
    require(current is None, "unterminated extended trace")
    require(
        set(cases) == {(name, 8, gate) for name in EXPECTED for gate in (0, 1)},
        "missing or unexpected extended cases",
    )
    return cases


def observable(events):
    # Preserve every ingress field and sequence position except endpoint name,
    # matching the original oracle's default-off compatibility boundary.
    return [
        {key: value for key, value in event.items() if key != "via"} for event in events
    ]


def expected_records(name):
    return [
        {"kind": "event", "frame": frame, "target": 42, "screen": 7, **event}
        for frame, events in enumerate(EXPECTED[name][1])
        for event in events
    ]


def shape_errors(case):
    name, _, gate = case["key"]
    frames = len(EXPECTED[name][1])
    events, end = case["events"], case["end"]
    errors = []
    if end != {
        "kind": "end",
        "frames": frames,
        "events": len(events),
        "drains": frames if gate else 0,
    }:
        errors.append(f"frame/event/drain accounting mismatch: {end!r}")
    if len(events) > 64:
        errors.append("more than 64 events")
    fields = {
        "kind",
        "frame",
        "type",
        "button",
        "flags",
        "via",
        "target",
        "screen",
        "h",
        "v",
    }
    for event in events:
        if set(event) != fields:
            errors.append("unexpected ingress record fields")
            continue
        if event["frame"] not in range(frames) or event["via"] not in (
            "legacy",
            "target",
        ):
            errors.append("unexpected frame/queue endpoint")
        if (event["target"], event["screen"]) != ((42, 7) if gate else (0, 0)):
            errors.append("incorrect target context")
        axes = [event[axis] for axis in ("h", "v") if event[axis] is not None]
        if any(
            not isinstance(value, (int, float)) or not math.isfinite(value)
            for value in axes
        ):
            errors.append("invalid scroll value")
        if event["type"] == "scroll":
            if not axes or event["button"] != 0 or event["flags"] != 2:
                errors.append("invalid scroll ingress")
        elif event["type"] in ("down", "up"):
            if axes or event["button"] != 1 or event["flags"] != 0:
                errors.append("invalid button ingress")
        else:
            errors.append("unknown ingress event")
    actual_buttons = [
        (e["frame"], e["type"], e["button"])
        for e in events
        if e["type"] in ("down", "up")
    ]
    wanted_buttons = [
        (e["frame"], e["type"], e["button"])
        for e in expected_records(name)
        if e["type"] in ("down", "up")
    ]
    if actual_buttons != wanted_buttons:
        errors.append("normal-button order/frame changed")
    return errors


def precision_observations(cases):
    observations = []
    for axis, direction in (("v", "vertical"), ("h", "horizontal")):
        observation = {
            "axis": axis,
            "v120_steps": -0.25,
            "continuous_steps": -2,
            "policy_cost_steps": -1.75,
        }
        for boundary, name in (
            ("same_flush", "axis_before_v120_same_flush"),
            ("across_button", "axis_before_v120_button"),
        ):
            key = (f"{name}_{direction}", 8, 1)
            observation[boundary] = {
                "case": list(key),
                "observed_scroll_values": [
                    e[axis]
                    for e in cases[key]["events"]
                    if e["type"] == "scroll" and e[axis] is not None
                ],
            }
        observations.append(observation)
    return observations


def evaluate_trace(trace, expectation, legacy_reference=None):
    require(expectation in ("pass", "legacy-capture"), "invalid extended expectation")
    require(
        expectation != "pass" or legacy_reference is not None,
        "extended pass requires an extended legacy capture",
    )
    cases = load_cases(trace)
    reference = load_cases(legacy_reference) if legacy_reference else None
    shape_failures, enabled_failures, legacy_failures = [], [], []
    for key, case in cases.items():
        errors = shape_errors(case)
        if errors:
            shape_failures.append({"case": list(key), "errors": errors})
        if key[2]:
            wanted = expected_records(key[0])
            if observable(case["events"]) != wanted:
                enabled_failures.append(
                    {
                        "case": list(key),
                        "contract": EXPECTED[key[0]][0],
                        "expected_ordered_ingress": wanted,
                        "observed_ordered_ingress": observable(case["events"]),
                    }
                )
        elif reference is not None:
            require(
                not shape_errors(reference[key]), f"invalid legacy reference: {key}"
            )
            if (
                observable(case["events"]) != observable(reference[key]["events"])
                or case["end"] != reference[key]["end"]
            ):
                legacy_failures.append(list(key))
    return {
        "suite": "extended",
        "expectation": expectation,
        "cases": len(cases),
        "accepted": not (
            shape_failures
            or legacy_failures
            or (expectation == "pass" and enabled_failures)
        ),
        "enabled_oracle_passed": not (shape_failures or enabled_failures),
        "enabled_failures": enabled_failures,
        "shape_failures": shape_failures,
        "legacy_failures": legacy_failures,
        "legacy_compared": reference is not None,
        "legacy_captured": expectation == "legacy-capture",
        "precision_observations": precision_observations(cases),
    }
