"""Independent callback-output oracle. Explicit checks survive python -O."""

import json
from pathlib import Path

EXPECTED = {
    "discrete_vertical_split": [{"v": -1}, {"v": -0.5}],
    "discrete_horizontal_split": [{"h": -1}, {"h": -0.5}],
    "discrete_dual_split": [{"h": 1, "v": -1}],
    "discrete_adjacent": [{"h": 2, "v": -1}, {}],
    "continuous": [{"h": 2, "v": -0.5}, {"v": -1}],
    "continuous_split": [{"h": 2, "v": -0.5}],
    "v120_standalone_reset": [
        {"h": 0.5, "v": -1},
        {"h": 2, "v": -1},
        {"v": -2},
        {},
        {"h": 0.25},
    ],
    "v120_vertical_split": [{"v": -1}, {"v": -0.5}],
    "v120_horizontal_split": [{"h": -1}, {"h": -0.5}],
}
REGRESSIONS = {
    "discrete_vertical_split",
    "discrete_horizontal_split",
    "discrete_dual_split",
    "v120_vertical_split",
    "v120_horizontal_split",
}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def load_cases(path):
    path = Path(path)
    require(path.stat().st_size <= 1024 * 1024, "trace exceeds 1 MiB")
    cases = {}
    current = None
    for line in path.read_text().splitlines():
        row = json.loads(line)
        if row["kind"] == "begin":
            require(current is None, "nested case")
            key = (row["name"], row["version"], row["gate"])
            require(key not in cases, "duplicate case")
            current = {"key": key, "events": []}
        elif row["kind"] == "event":
            require(current is not None, "event outside case")
            current["events"].append(row)
        elif row["kind"] == "end":
            require(current is not None, "end outside case")
            current["end"] = row
            cases[current["key"]] = current
            current = None
        else:
            raise RuntimeError("unknown trace record")
    require(current is None, "unterminated trace")
    expected_keys = {
        (name, version, gate)
        for name in EXPECTED
        for version in ((8,) if name.startswith("v120") else (5, 6, 7))
        for gate in (0, 1)
    }
    require(set(cases) == expected_keys, "missing or unexpected cases")
    return cases


def case_errors(case, known_regression=False):
    name, _version, gate = case["key"]
    frames = EXPECTED[name]
    events, end = case["events"], case["end"]
    errors = []
    if end["frames"] != len(frames) or end["events"] != len(events):
        errors.append("frame/event accounting mismatch")
    if not gate and end["drains"] != 0:
        errors.append("gate-off path unexpectedly drains MI")
    buttons = [
        (e["type"], e["button"], e["frame"])
        for e in events
        if e["type"] in ("down", "up")
    ]
    expected_buttons = [("down", 1, 0), ("up", 1, 0)] if "split" in name else []
    if buttons != expected_buttons:
        errors.append(f"normal buttons {buttons!r} != {expected_buttons!r}")
    if gate and "split" in name:
        frame_zero = [e["type"] for e in events if e["frame"] == 0]
        wanted_order = ["scroll", "down", "up"]
        if name == "continuous_split" or (known_regression and name in REGRESSIONS):
            wanted_order.append("scroll")
        if frame_zero != wanted_order:
            errors.append(f"frame 0 order {frame_zero!r} != {wanted_order!r}")
    for event in events:
        if event["frame"] not in range(len(frames)):
            errors.append("event outside expected frames")
        if event["type"] not in ("scroll", "down", "up"):
            errors.append("unexpected event type")
    for frame, expected in enumerate(frames):
        for axis in ("h", "v"):
            observed = [
                e[axis]
                for e in events
                if e["frame"] == frame and e["type"] == "scroll" and e[axis] is not None
            ]
            wanted = [expected[axis]] if axis in expected else []
            if known_regression and gate and name in REGRESSIONS and frame == 0:
                wanted = wanted * 2
            if observed != wanted:
                errors.append(f"frame {frame} {axis}: {observed!r} != {wanted!r}")
    return errors


def check_trace(trace, expectation, legacy_reference=None):
    cases = load_cases(trace)
    failures = {
        key: errors for key, case in cases.items() if (errors := case_errors(case))
    }
    if expectation == "wheel-regression":
        expected_failures = {key for key in cases if key[2] and key[0] in REGRESSIONS}
        require(
            set(failures) == expected_failures,
            f"negative control failed unexpected cases: {failures!r}",
        )
        for key, case in cases.items():
            errors = case_errors(case, known_regression=True)
            require(
                not errors, f"negative-control signature differs at {key}: {errors}"
            )
    else:
        require(not failures, f"callback regressions: {failures!r}")
        require(legacy_reference is not None, "pass mode requires --legacy-reference")
        reference = load_cases(legacy_reference)
        for key, case in cases.items():
            if key[2]:
                continue
            require(not case_errors(reference[key]), "legacy reference has a failure")

            # Endpoint spelling is not observable behavior; ordered records are.
            def observable(events):
                return [
                    {k: v for k, v in event.items() if k != "via"} for event in events
                ]

            require(
                observable(case["events"]) == observable(reference[key]["events"]),
                f"flag-off ordered ingress trace changed: {key}",
            )
            require(
                case["end"] == reference[key]["end"],
                f"flag-off drain/accounting changed: {key}",
            )
    return {
        "expectation": expectation,
        "cases": len(cases),
        "positive_oracle_failing_cases": [list(key) for key in failures],
        "legacy_compared": legacy_reference is not None,
    }
