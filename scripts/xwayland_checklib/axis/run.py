"""Explicit axis callback check, with a pinned legacy trace and optional external controls."""

import argparse
import hashlib
import importlib.util
import json
import os
import shlex
import sys
from pathlib import Path

from ..process import fresh_output
from ..process import invoke as bounded_invoke

ART = Path(__file__).resolve().parent
BASELINE = ART / "legacy-24.1.10"
BASELINE_MANIFEST_SHA256 = (
    "72d01a63cbbf5f46c0662e8d2461d9bd08cf36ac9a886ddf841372f811481456"
)
INITIAL_INPUT_SHA256 = (
    "d5744ce94d276deefd050267176c1d874a5d0974abc990c87b8717a843545e61"
)
SOURCE_FILES = (
    "hw/xwayland/xwayland-input.c",
    "hw/xwayland/xwayland-input.h",
    "hw/xwayland/xwayland-input-target.c",
    "hw/xwayland/xwayland-input-target.h",
    "dix/inpututils.c",
    "include/inpututils.h",
)


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_oracle(filename):
    module_name = Path(filename).stem.replace("-", "_")
    spec = importlib.util.spec_from_file_location(module_name, ART / filename)
    oracle = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(oracle)
    return oracle


def compile_entry(database, suffix):
    entries = [e for e in database if Path(e["file"]).as_posix().endswith(suffix)]
    require(len(entries) == 1, f"expected one compile command for {suffix}")
    return entries[0]


def relocated_flags(entry, source, original_source):
    directory = Path(entry["directory"]).resolve()
    arguments = entry.get("arguments") or shlex.split(entry["command"])
    require(arguments and not arguments[0].startswith("-"), "missing compiler")
    # This configured database uses cc directly. Fail rather than silently
    # reinterpret shell syntax, env wrappers, compiler launchers, or response files.
    require(
        Path(arguments[0]).name in ("cc", "gcc", "clang"),
        "unsupported compiler launcher in compile_commands",
    )

    def relocate(value):
        path = (directory / value).resolve()
        if path.is_relative_to(original_source):
            return str(source / path.relative_to(original_source))
        return str(path)

    original_file = (directory / entry["file"]).resolve()
    flags = []
    i = 1
    while i < len(arguments):
        arg = arguments[i]
        if arg in ("-o", "-MF", "-MT", "-MQ"):
            i += 2
            continue
        if arg in ("-c", "-MD", "-MMD", "-MP", "-MG"):
            i += 1
            continue
        if arg in ("-I", "-isystem", "-iquote", "-include"):
            require(i + 1 < len(arguments), f"missing argument for {arg}")
            flags.extend((arg, relocate(arguments[i + 1])))
            i += 2
            continue
        if arg.startswith("-I"):
            flags.append("-I" + relocate(arg[2:]))
        elif not arg.startswith("-") and (directory / arg).resolve() == original_file:
            pass
        else:
            require(
                not arg.startswith("@") and arg not in ("&&", ";", "|"),
                "unexpected shell syntax/response file",
            )
            require(
                arg.startswith("-"), f"unexpected compiler positional argument: {arg}"
            )
            flags.append(arg)
        i += 1
    flags.extend(("-UNDEBUG", "-ffunction-sections", "-fdata-sections", "-fno-lto"))
    return [arguments[0]], flags


def invoke(command, out, name, run=False):
    return bounded_invoke(
        command,
        out,
        name,
        timeout=5 if run else 45,
        cpu=2 if run else 30,
        address_space=(256 if run else 2048) * 1024**2,
    )


def bundled_reference():
    require(
        digest(BASELINE / "manifest.json") == BASELINE_MANIFEST_SHA256,
        "maintained legacy metadata changed; do not rebaseline",
    )
    manifest = json.loads((BASELINE / "manifest.json").read_text())
    for name, expected in manifest["files_sha256"].items():
        require(
            digest(BASELINE / name) == expected,
            f"maintained legacy capture digest mismatch: {name}",
        )
    return BASELINE / "run.stdout"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source",
        type=Path,
        required=True,
        help="explicit root of the candidate Xwayland source tree",
    )
    parser.add_argument("--compile-commands", type=Path, required=True)
    parser.add_argument(
        "--output", type=Path, required=True, help="fresh evidence directory"
    )
    parser.add_argument("--suite", choices=("initial", "extended"), default="extended")
    parser.add_argument(
        "--expect",
        choices=("pass", "wheel-regression", "legacy-capture"),
        default="pass",
    )
    parser.add_argument(
        "--legacy-reference",
        type=Path,
        help="external accepted candidate-1 capture; defaults to the pinned maintained trace",
    )
    args = parser.parse_args(argv)
    require(os.geteuid() != 0, "run as a normal user")
    require(
        args.source.is_absolute() and args.compile_commands.is_absolute(),
        "source and compile database paths must be absolute",
    )
    if args.expect == "pass" and args.legacy_reference is None:
        require(
            args.suite == "extended",
            "bundled reference requires the complete 98-case suite",
        )
        args.legacy_reference = bundled_reference()
    require(
        args.expect != "legacy-capture" or args.suite == "extended",
        "legacy-capture is only for the extended suite",
    )
    require(
        args.expect != "wheel-regression" or args.suite == "initial",
        "wheel-regression retains the original 42-case / exactly-11 contract",
    )
    require(
        args.expect != "legacy-capture" or args.legacy_reference is None,
        "legacy-capture creates a new reference; do not supply an old trace",
    )
    if args.legacy_reference is not None:
        args.legacy_reference = args.legacy_reference.resolve(strict=True)
        reference_dir = args.legacy_reference.parent
        reference_manifest = json.loads((reference_dir / "manifest.json").read_text())
        reference_result = json.loads((reference_dir / "result.json").read_text())
        require(
            reference_manifest["source_sha256"][SOURCE_FILES[0]]
            == INITIAL_INPUT_SHA256,
            "legacy reference is not from saved candidate-1",
        )
        require(
            reference_manifest.get("suite", "initial") == args.suite,
            "legacy reference must be captured with the same suite",
        )
        reference_expectation = (
            "legacy-capture" if args.suite == "extended" else "wheel-regression"
        )
        require(
            reference_result["expectation"] == reference_expectation,
            "legacy reference has not passed the appropriate capture/control check",
        )
        if args.suite == "extended":
            require(
                reference_result["accepted"] and reference_result["legacy_captured"],
                "extended legacy capture was not accepted",
            )
            initial_result = reference_result["initial_suite"]
            require(
                initial_result["expectation"] == "wheel-regression"
                and initial_result["cases"] == 42
                and len(initial_result["positive_oracle_failing_cases"]) == 11,
                "extended reference lacks the original exactly-11 negative control",
            )
    source = args.source.resolve(strict=True)
    compdb = args.compile_commands.resolve(strict=True)
    before = {name: digest(source / name) for name in SOURCE_FILES}
    if args.expect in ("wheel-regression", "legacy-capture"):
        require(
            before[SOURCE_FILES[0]] == INITIAL_INPUT_SHA256,
            "negative control must be the exact saved candidate-1 input source",
        )
    database = json.loads(compdb.read_text())
    input_entry = compile_entry(database, "hw/xwayland/xwayland-input.c")
    original_source = (
        (Path(input_entry["directory"]) / input_entry["file"]).resolve().parents[2]
    )
    compiler, flags = relocated_flags(input_entry, source, original_source)
    utils_entry = compile_entry(database, "dix/inpututils.c")
    utils_compiler, utils_flags = relocated_flags(utils_entry, source, original_source)

    out = fresh_output(args.output)
    (out / "tmp").mkdir(mode=0o700)
    defines = [
        "-DFIXTURE_INPUT_SOURCE=" + json.dumps(str(source / SOURCE_FILES[0])),
        "-DFIXTURE_TARGET_SOURCE=" + json.dumps(str(source / SOURCE_FILES[2])),
    ]
    commands = []
    for filename in ("fixture.c", "fixture-support.c"):
        stem = Path(filename).stem
        commands.append(
            (
                stem,
                compiler
                + flags
                + defines
                + [
                    "-I" + str(ART),
                    "-MMD",
                    "-MF",
                    str(out / f"{stem}.d"),
                    "-c",
                    str(ART / filename),
                    "-o",
                    str(out / f"{stem}.o"),
                ],
            )
        )
    commands.append(
        (
            "inpututils",
            utils_compiler
            + utils_flags
            + [
                "-MMD",
                "-MF",
                str(out / "inpututils.d"),
                "-c",
                str(source / "dix/inpututils.c"),
                "-o",
                str(out / "inpututils.o"),
            ],
        )
    )
    binary = out / "axis-callback-fixture"
    commands.append(
        (
            "link",
            compiler
            + [
                "-fno-lto",
                "-Wl,--gc-sections",
                "-Wl,--no-undefined",
                "-Wl,-Map=" + str(out / "link.map"),
                str(out / "fixture.o"),
                str(out / "fixture-support.o"),
                str(out / "inpututils.o"),
                "-lm",
                "-o",
                str(binary),
            ],
        )
    )
    manifest = {
        "source": str(source),
        "compile_commands": str(compdb),
        "compile_commands_sha256": digest(compdb),
        "source_sha256": before,
        "fixture_sha256": {p.name: digest(p) for p in ART.iterdir() if p.is_file()},
        "commands": {name: command for name, command in commands},
        "expectation": args.expect,
        "suite": args.suite,
        "run_commands": (
            [[str(binary)], [str(binary), "extended"]]
            if args.suite == "extended"
            else [[str(binary)]]
        ),
        "legacy_reference": str(args.legacy_reference)
        if args.legacy_reference
        else None,
        "legacy_reference_sha256": digest(args.legacy_reference)
        if args.legacy_reference
        else None,
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    for name, command in commands:
        print(f"{name}: {shlex.join(command)}", flush=True)
        invoke(command, out, name)
    after = {name: digest(source / name) for name in SOURCE_FILES}
    require(before == after, "candidate source changed during compilation")
    oracle = load_oracle("check_results.py")
    if args.suite == "initial":
        invoke([str(binary)], out, "run", run=True)
        result = oracle.check_trace(
            out / "run.stdout", args.expect, args.legacy_reference
        )
    else:
        invoke([str(binary)], out, "initial", run=True)
        initial_reference = (
            args.legacy_reference.parent / "initial.stdout"
            if args.legacy_reference
            else None
        )
        initial_result = oracle.check_trace(
            out / "initial.stdout",
            "wheel-regression" if args.expect == "legacy-capture" else "pass",
            initial_reference,
        )
        invoke([str(binary), "extended"], out, "run", run=True)
        result = load_oracle("extended_results.py").evaluate_trace(
            out / "run.stdout", args.expect, args.legacy_reference
        )
        result["initial_suite"] = initial_result
        result["total_cases"] = initial_result["cases"] + result["cases"]
        result["source_sha256"] = before
        # Preserve diagnostics even when the new positive oracle rejects a
        # candidate. A failed evaluation never becomes an accepted reference.
        (out / "extended-evaluation.json").write_text(
            json.dumps(result, indent=2) + "\n"
        )
        require(
            result["accepted"],
            f"extended oracle rejected candidate; see {out / 'extended-evaluation.json'}",
        )
    result["source_sha256"] = before
    (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    print(f"Artifacts: {out}")
    return result


if __name__ == "__main__":
    sys.dont_write_bytecode = True
    try:
        main()
    except (RuntimeError, OSError, ValueError, KeyError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        sys.exit(1)
