"""Parent-run matrix. Does not build, discover binaries, or relax worker assertions."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

SELECTOR = "x11_live_async_child_confinement_clamps_each_warp_emulated_relative_frame"


def core_event(event, ids, recipient, point, response_type):
    return (
        event["recipient"] == ids[recipient]
        and event["root"] == ids["root"]
        and event["root_xy"] == point
        and event["event_xy"] == point
        and event["same_screen"] is True
        and event["response_type"] == response_type
    )


def button_pair(events, ids, recipient, point):
    return len(events) == 2 and all(
        core_event(event, ids, recipient, point, 4 if down else 5)
        and event["pressed"] is down
        and event["detail"] == 1
        and event["button1"] is not down
        for event, down in zip(events, [True, False])
    )


def raw_frame(events, ids, delta):
    if len(events) != 1:
        return False
    event = events[0]
    mask = event["valuator_mask"]
    return (
        event["master"] == ids["master"]
        and event["source"] == ids["relative"]
        and event["response_type"] == 35  # Native GenericEvent.
        and event["event_type"] == 17  # XI_RawMotion.
        and len(mask) >= 1 and mask[0] == 3 and all(word == 0 for word in mask[1:])
        and event["delta_raw"] == [[delta, 0], [0, 0]]
    )


def event_evidence_matches(report, name):
    """Require actual complete delivery, including for both negative controls."""
    ids = report["identities"]
    frames = report["frames"]
    deltas = [100, -1, -1]
    xs = [149, 149, 149] if name == "candidate3" else [149, 148, 147]
    if len(frames) != 3 or ids["primary"] == ids["neighbor"]:
        return False
    if ids["master"] == ids["relative"]:
        return False
    for index, (frame, delta, x) in enumerate(zip(frames, deltas, xs)):
        point = [x, 120]
        pointer = frame["query_pointer"]
        a, b = frame["events"]["a"], frame["events"]["b"]
        snapshot = frame["snapshot"]
        if not (
            frame["delta"] == delta and frame["expected"] == [149 - index, 120]
            and pointer["root"] == ids["root"] and pointer["child"] == ids["child"]
            and pointer["same_screen"] is True
            and pointer["root_xy"] == point and pointer["window_xy"] == point
            and len(a["motions"]) == 1
            and core_event(a["motions"][0], ids, "primary", point, 6)
            and a["motions"][0]["child"] == ids["child"]
            and b["motions"] == [] and a["buttons"] == [] and b["buttons"] == []
            and raw_frame(a["raw"], ids, delta) and raw_frame(b["raw"], ids, delta)
            and snapshot["locked_on"] == ids["primary_surface"]
            and snapshot["accepted_locks"] == 1 and snapshot["cursor_hidden"] is True
            and snapshot["relative_frames"] == deltas[:index + 1]
        ):
            return False
    deliveries = report["deliveries"]
    confined = deliveries["confined"]
    restacked = deliveries["restacked"]
    fresh = deliveries["fresh_neighbor"]
    recipient, other = ("b", "a") if name == "default-off" else ("a", "b")
    window = "neighbor" if name == "default-off" else "primary"
    protocol = report["protocol"]
    return (
        button_pair(confined["a"]["buttons"], ids, "primary", [xs[-1], 120])
        and confined["b"]["buttons"] == []
        and button_pair(restacked[recipient]["buttons"], ids, window, [60, 20])
        and restacked[other]["buttons"] == []
        and button_pair(fresh["b"]["buttons"], ids, "neighbor", [60, 20])
        and fresh["a"]["buttons"] == []
        and protocol["locked_on"] is None and protocol["cursor_hidden"] is False
        and protocol["accepted_locks"] == 1
        and protocol["relative_frames"] == deltas and protocol["clicks"] == 3
    )


def classify(name, code, report):
    expected = {
        "default-off": {"recipient.primary_after_restack", "recipient.no_neighbor_leak"},
        "candidate3": {"endpoint.1", "endpoint.2", "motion.1", "motion.2", "buttons.confined"},
        "fixed": set(),
    }
    try:
        assertions = [failure["assertion"] for failure in report["failures"]]
        return (
            report["selector"] == SELECTOR and report["cleanup_verified"] is True
            and code == (0 if name == "fixed" else 1)
            and len(assertions) == len(set(assertions))
            and set(assertions) == expected[name]
            # No integrity, raw, lock, protocol, or cleanup failure is allowable.
            # Matching failure labels alone is NEVER sufficient evidence.
            and event_evidence_matches(report, name)
        )
    except (KeyError, TypeError, IndexError, ValueError):
        return False


def absolute_file(value):
    path = Path(value)
    if not path.is_absolute() or not path.is_file():
        raise argparse.ArgumentTypeError("an explicit absolute file path is required")
    return path.resolve()


def run(fixture, binary, targets, output):
    command = [str(fixture), "--xwayland", str(binary), "--output", str(output)]
    if targets:
        command.append("--surface-input-targets")
    # The Rust supervisor owns the 60s deadline and runtime removal. Give it
    # time to reap its worker group before this outer bound expires.
    result = subprocess.run(
        command,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        env={"PATH": "/usr/bin:/bin", "LC_ALL": "C"},
        timeout=75,
        check=False,
    )
    print(result.stdout, end="", flush=True)
    print(result.stderr, end="", flush=True)
    paths = [line.removeprefix("ARTIFACTS=") for line in result.stdout.splitlines()
             if line.startswith("ARTIFACTS=")]
    if len(paths) != 1:
        raise RuntimeError(f"missing supervisor evidence directory: {result.returncode}")
    artifact = Path(paths[0])
    if artifact.resolve() != output.resolve():
        raise RuntimeError("fixture returned an unexpected evidence directory")
    report_path = artifact / "report.json"
    report = json.loads(report_path.read_text()) if report_path.exists() else None
    return result.returncode, artifact, report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", required=True, type=absolute_file)
    parser.add_argument("--candidate3", required=True, type=absolute_file)
    parser.add_argument("--fixed", required=True, type=absolute_file)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if not args.output.is_absolute():
        parser.error("--output must be an absolute fresh directory")
    args.output.mkdir(mode=0o700)
    args.output = args.output.resolve()
    cases = [
        ("default-off", args.candidate3, False),
        ("candidate3", args.candidate3, True),
        ("fixed", args.fixed, True),
    ]
    summary = []
    for name, binary, targets in cases:
        code, artifact, report = run(args.fixture, binary, targets, args.output / name)
        assertions = {f["assertion"] for f in report["failures"]} if report else set()
        accepted = classify(name, code, report)
        item = {
            "case": name, "accepted": accepted, "exit_code": code,
            "binary": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "artifacts": str(artifact), "assertions": sorted(assertions),
        }
        summary.append(item)
        (artifact / "matrix-case.json").write_text(json.dumps(item, indent=2) + "\n")
        print(json.dumps(item), flush=True)
    # Each invocation's evidence is retained even when the matrix fails.
    return 0 if all(item["accepted"] for item in summary) else 1


if __name__ == "__main__":
    raise SystemExit(main())
