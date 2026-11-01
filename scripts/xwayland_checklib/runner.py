"""Explicit, serial checks of one provenance-verified prepared Xwayland."""

import argparse
import fcntl
import importlib.util
import json
import os
import shutil
import signal
import sys
from pathlib import Path

from xwayland_provenance import verify_prepared

from .axis import run as axis
from .process import fresh_output, invoke, private_environment, require

REPOSITORY = Path(__file__).resolve().parents[2]
RELATIVE = REPOSITORY / "tests/xwayland-relative"


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def relative_classifier():
    spec = importlib.util.spec_from_file_location(
        "relative_matrix", RELATIVE / "matrix.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.classify


def build_relative(out):
    cargo = shutil.which("cargo")
    require(
        cargo is not None, "cargo is required to build the standalone relative fixture"
    )
    runtime = out / "tmp"
    env = private_environment(runtime)
    # Rustup/registry caches are explicit build inputs. Desktop and arbitrary
    # compiler-injection environment variables are never inherited.
    env.update(
        {
            "PATH": str(Path(cargo).parent) + ":/usr/bin:/bin",
            "CARGO_HOME": os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")),
            "RUSTUP_HOME": os.environ.get("RUSTUP_HOME", str(Path.home() / ".rustup")),
            "CARGO_TARGET_DIR": str(RELATIVE / "target"),
            "CARGO_BUILD_JOBS": "1",
        }
    )
    invoke(
        [
            cargo,
            "build",
            "--locked",
            "--jobs",
            "1",
            "--manifest-path",
            RELATIVE / "Cargo.toml",
        ],
        out,
        "relative-build",
        cwd=RELATIVE,
        env=env,
        timeout=1200,
        cpu=900,
        address_space=8 * 1024**3,
        file_size=1024**3,
    )
    binary = RELATIVE / "target/debug/relative-confinement-fixture"
    require(
        binary.is_file() and os.access(binary, os.X_OK),
        "standalone relative fixture build produced no executable",
    )
    return binary


def run_relative(fixture, xwayland, out):
    evidence = out / "relative"
    # The supervisor owns its worker group and 60-second deadline. On TERM it
    # reaps that group and closes its private runtime within invoke's grace.
    # Worker/Xwayland parent-death SIGKILL remains the escalation fallback.
    invoke(
        [
            fixture,
            "--xwayland",
            xwayland,
            "--surface-input-targets",
            "--output",
            evidence,
        ],
        out,
        "relative",
        timeout=75,
        cpu=65,
    )
    report = json.loads((evidence / "report.json").read_text())
    require(
        (evidence / "exit-code.txt").read_text().strip() == "0",
        "relative supervisor did not record successful cleanup",
    )
    require(
        relative_classifier()("fixed", 0, report),
        "relative fixture did not meet the complete positive delivery contract",
    )
    require(
        not list(evidence.glob("private-*")),
        "relative fixture left its private runtime behind",
    )
    return {
        "accepted": True,
        "selector": report["selector"],
        "cleanup_verified": report["cleanup_verified"],
        "report": str(evidence / "report.json"),
    }


def require_axis_result(result):
    initial = result.get("initial_suite", {})
    require(
        result.get("suite") == "extended"
        and result.get("expectation") == "pass"
        and result.get("accepted") is True
        and result.get("enabled_oracle_passed") is True
        and result.get("cases") == 56
        and result.get("total_cases") == 98
        and result.get("legacy_compared") is True
        and result.get("legacy_captured") is False
        and result.get("enabled_failures") == []
        and result.get("shape_failures") == []
        and result.get("legacy_failures") == []
        and initial.get("expectation") == "pass"
        and initial.get("cases") == 42
        and initial.get("positive_oracle_failing_cases") == []
        and initial.get("legacy_compared") is True,
        "axis checks were incomplete, skipped, or rejected",
    )


def run_checks(args, out):
    before = verify_prepared(args.xwayland, args.source, args.compile_commands)
    write_json(out / "prepared.json", before)
    axis.bundled_reference()  # Validate the recorded baseline before any build.
    fixture = build_relative(out)
    relative = run_relative(fixture, args.xwayland, out)
    result = axis.main(
        [
            "--source",
            str(args.source),
            "--compile-commands",
            str(args.compile_commands),
            "--output",
            str(out / "axis"),
            "--suite",
            "extended",
            "--expect",
            "pass",
        ]
    )
    require_axis_result(result)
    after = verify_prepared(args.xwayland, args.source, args.compile_commands)
    require(before == after, "prepared inputs changed during regression checks")
    summary = {
        "accepted": True,
        "relative": relative,
        "axis_cases": result["total_cases"],
        "axis_default_off_compared": 49,
        "axis_report": str(out / "axis/result.json"),
    }
    write_json(out / "result.json", summary)
    return summary


def interrupted(signum, _frame):
    raise RuntimeError(f"interrupted by signal {signum}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--xwayland", required=True, type=Path)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--compile-commands", required=True, type=Path)
    parser.add_argument(
        "--output",
        required=True,
        type=Path,
        help="absolute fresh evidence directory; its parent must exist",
    )
    args = parser.parse_args(argv)
    sys.dont_write_bytecode = True
    out = None
    old_handler = signal.signal(signal.SIGTERM, interrupted)
    try:
        require(os.geteuid() != 0, "run checks as a normal user")
        out = fresh_output(args.output)
        # Lock only this check's standalone build/suites. No app startup builds.
        target = RELATIVE / "target"
        target.mkdir(exist_ok=True)
        with (target / ".check-xwayland-input.lock").open("a") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                raise RuntimeError(
                    "another Xwayland input check holds the fixture lock"
                ) from None
            summary = run_checks(args, out)
        print(json.dumps(summary, indent=2))
        return 0
    except (
        RuntimeError,
        OSError,
        ValueError,
        KeyError,
        TypeError,
        KeyboardInterrupt,
    ) as error:
        if out is not None:
            write_json(out / "failure.json", {"accepted": False, "error": str(error)})
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    finally:
        signal.signal(signal.SIGTERM, old_handler)
