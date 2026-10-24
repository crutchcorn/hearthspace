"""The concrete desktop artifact contract, shared with the Rust dev bootstrap."""

import json
import os
import subprocess
import sys
from pathlib import Path

INVENTORY = json.loads(Path(__file__).with_name("runtime-artifacts.json").read_text())
RUNTIME = tuple(item for item in INVENTORY if not item.get("development_only", False))
PACKAGES = tuple(item["package"] for item in RUNTIME)
BINARIES = tuple(binary for item in RUNTIME for binary in item["binaries"])


def package_for(binary):
    for item in INVENTORY:
        if binary in item["binaries"]:
            return item
    raise ValueError(f"not a declared Hearthspace development binary: {binary}")


def runtime_build_command(target_dir, *, release=False):
    command = ["cargo", "build", "--locked", "--target-dir", str(target_dir), "--bins"]
    if release:
        command.append("--release")
    for item in RUNTIME:
        command.extend(["-p", item["package"]])
        for feature in item["native_features"]:
            command.extend(["--features", f"{item['package']}/{feature}"])
    return command


def development_commands(target_dir):
    # Cargo-run children resolve features independently. Warm those exact package
    # graphs (including every session helper) before the harness claims a VT.
    commands = []
    for item in RUNTIME:
        command = [
            "cargo",
            "build",
            "--locked",
            "--target-dir",
            str(target_dir),
            "-p",
            item["package"],
            "--bins",
        ]
        for feature in item["native_features"]:
            command.extend(["--features", feature])
        commands.append(command)
    return commands


def build_executable(binary, repository):
    """Let Cargo check freshness and report the executable, including custom targets."""
    item = package_for(binary)
    command = [
        "cargo",
        "build",
        "--locked",
        "--message-format=json-render-diagnostics",
        "-p",
        item["package"],
        "--bin",
        binary,
    ]
    for feature in item["native_features"]:
        command.extend(["--features", feature])
    executables = set()
    with subprocess.Popen(
        command, cwd=repository, stdout=subprocess.PIPE, text=True
    ) as child:
        for line in child.stdout:
            try:
                message = json.loads(line)
            except ValueError:
                # Cargo cannot control arbitrary procedural-macro output.
                print(line, end="", file=sys.stderr)
                continue
            if not isinstance(message, dict):
                continue
            if (
                message.get("reason") == "compiler-artifact"
                and message.get("target", {}).get("name") == binary
                and "bin" in message.get("target", {}).get("kind", [])
                and not message.get("profile", {}).get("test", False)
                and message.get("executable")
            ):
                executables.add(Path(message["executable"]))
        status = child.wait()
    if status:
        raise subprocess.CalledProcessError(status, command)
    if len(executables) != 1:
        raise RuntimeError(f"Cargo did not report one executable for {binary}")
    executable = executables.pop()
    if (
        not executable.is_absolute()
        or not executable.is_file()
        or not os.access(executable, os.X_OK)
    ):
        raise RuntimeError(f"Cargo reported an unavailable executable: {executable}")
    return executable
