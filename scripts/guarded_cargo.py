"""One bounded Cargo service, with an independent durable resource monitor."""

import argparse
import json
import os
import shutil
import signal
import sys
import time
import uuid
from pathlib import Path

import guarded_cargo_evidence as evidence
import guarded_cargo_policy as policy
import guarded_cargo_watchdog as watchdog

ENTRY = Path(__file__).resolve().with_name("guarded-cargo")


def executable(name):
    found = shutil.which(name, path=os.environ.get("PATH", ""))
    if not found:
        raise RuntimeError(f"{name} is required on the caller's PATH")
    # Preserve argv[0] for rustup's cargo symlink; do not resolve it to rustup.
    return os.path.abspath(found)


def service_command(state, environment):
    properties = state["properties"] | {
        "Description": state["owner"],
        "WorkingDirectory": state["cwd"],
        "StandardInput": "null",
        "StandardOutput": "append:" + str(Path(state["evidence"]) / "build.log"),
        "StandardError": "inherit",
    }
    return [
        state["systemd_run"],
        "--user",
        "--quiet",
        "--expand-environment=no",
        "--unit=" + state["unit"],
        *[f"--property={key}={value}" for key, value in properties.items()],
        *[f"--setenv={key}={value}" for key, value in environment.items()],
        sys.executable,
        "-I",
        str(ENTRY),
        "--worker",
        str(Path(state["evidence"]) / "command.json"),
    ]


def worker(state_path):
    state = json.loads(state_path.read_text())
    directory = Path(state["evidence"])
    lock = None
    try:
        policy.normal_user()
        lock = policy.acquire_lock()
        verified = policy.verify_worker(state)
        environment = {key: os.environ[key] for key in state["environment_names"]}
        if policy.digest(environment) != state["environment_sha256"]:
            raise RuntimeError("forwarded build environment changed")
        relay = watchdog.start(state_path, ENTRY)
        evidence.save(directory / "verified.json", verified)
        deadline = time.monotonic() + 15
        ready = directory / "monitor-ready.json"
        while not ready.exists():
            if time.monotonic() >= deadline:
                raise RuntimeError("external monitor did not acknowledge readiness")
            time.sleep(0.05)
        acknowledgment = json.loads(ready.read_text())
        if acknowledgment != {
            "owner": state["owner"],
            "verified_sha256": evidence.file_hash(directory / "verified.json"),
        }:
            raise RuntimeError(
                "external monitor acknowledgment does not match the worker"
            )
        policy.verify_worker(state)
        if relay.poll() is not None:
            raise RuntimeError("watchdog relay exited before Cargo execution")
        evidence.save(
            directory / "executing.json",
            {"time_ns": time.time_ns(), "pid": os.getpid(), "watchdog_pid": relay.pid},
        )
        # Replace the worker; Cargo inherits the lock and all cgroup constraints.
        # Only explicitly selected variables reach Cargo, regardless of manager env.
        os.execve(state["argv"][0], state["argv"], environment)
    except Exception as error:
        evidence.save(directory / "worker-error.json", {"error": str(error)})
        raise
    finally:
        if lock is not None:
            os.close(lock)


def stop_owned(state):
    record = {}
    try:
        properties = policy.show(state["systemctl"], state["unit"])
        record["before"] = properties
        if properties.get("LoadState") != "not-found":
            policy.own_unit(state, properties)
            policy.command(
                [state["systemctl"], "--user", "stop", state["unit"]], timeout=15
            )
        properties = policy.show(state["systemctl"], state["unit"])
        record["after"] = properties
        group = policy.cgroup_path(state["slice_cgroup"]) / state["unit"]
        record.update(policy.empty_cgroup(group))
        if not record.get("cgroup_removed") and (
            record["cgroup_populated"] != "0" or record["remaining_tasks"]
        ):
            raise RuntimeError("owned cgroup still contains tasks after stop")
        if properties.get("LoadState") != "not-found":
            policy.own_unit(state, properties)
            if (
                properties.get("ActiveState") not in {"inactive", "failed"}
                or properties.get("MainPID") != "0"
                or properties.get("ControlPID") != "0"
                or properties.get("Job") not in {"", "0"}
            ):
                raise RuntimeError(
                    "owned unit still has a process, job, or active state after stop"
                )
            # Release failed transient units after recording the terminal state.
            policy.command(
                [state["systemctl"], "--user", "reset-failed", state["unit"]],
                check=False,
            )
        record["verified_empty"] = True
    finally:
        evidence.save(Path(state["evidence"]) / "cleanup.json", record)


def terminal(properties):
    return properties.get("ActiveState") in {"failed", "inactive"} or (
        properties.get("ActiveState") == "active"
        and properties.get("SubState") == "exited"
    )


def monitor(state, log):
    directory = Path(state["evidence"])
    group = policy.cgroup_path(state["slice_cgroup"]) / state["unit"]
    if policy.self_cgroup().is_relative_to(group):
        raise RuntimeError("the monitor must be outside the workload cgroup")
    deadline = time.monotonic() + 45 * 60 + 20
    next_sample = time.monotonic()
    acknowledged = False
    while time.monotonic() < deadline:
        properties = policy.show(state["systemctl"], state["unit"], timeout=1)
        policy.own_unit(state, properties)
        done = terminal(properties)
        sample = evidence.host_sample()
        sample["service"] = properties
        try:
            sample["cgroup"] = evidence.cgroup_sample(group)
        except FileNotFoundError:
            if not done:
                raise
            sample["cgroup_unavailable_after_exit"] = True
        evidence.append_sample(directory / "telemetry.jsonl", sample, log)
        evidence.check_temperature(sample)
        if done:
            return properties
        # Only a complete, flushed, cool sample renews monitor liveness.
        evidence.save(
            directory / "monitor-heartbeat.json",
            {
                "owner": state["owner"],
                "sample_monotonic_ns": sample["monotonic_ns"],
            },
            replace=True,
        )
        verified = directory / "verified.json"
        if not acknowledged and verified.exists():
            evidence.save(
                directory / "monitor-ready.json",
                {
                    "owner": state["owner"],
                    "verified_sha256": evidence.file_hash(verified),
                },
            )
            acknowledged = True
        next_sample += policy.INTERVAL
        time.sleep(max(0, next_sample - time.monotonic()))
    raise RuntimeError("supervisor runtime deadline reached")


def interrupted(signum, _frame):
    # selectors swallows InterruptedError while subprocess.run waits for output.
    raise RuntimeError(f"supervisor interrupted by signal {signum}")


def supervise(arguments, evidence_path):
    policy.normal_user()
    for path in (
        str(Path.cwd()),
        str(ENTRY),
        sys.executable,
        str(Path(evidence_path).absolute().resolve()),
    ):
        if "%" in path or "$" in path:
            raise ValueError("service paths containing % or $ are unsupported")
    argv = [executable("cargo"), *policy.cargo_arguments(arguments)]
    environment = policy.build_environment(os.environ)
    allowed_cpus = sorted(os.sched_getaffinity(0))
    if not allowed_cpus:
        raise RuntimeError("the caller has no permitted logical CPU")
    directory = evidence.fresh_directory(evidence_path)
    state = None
    attempted = False
    result = {"outcome": "rejected", "exit_code": 1}
    previous_handlers = {
        sig: signal.signal(sig, interrupted)
        for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)
    }
    try:
        with (directory / "build.log").open("xb", buffering=0) as log:
            evidence.sync_directory(directory)
            sample = evidence.host_sample()
            evidence.append_sample(directory / "telemetry.jsonl", sample, log)
            evidence.sync_directory(directory)
            evidence.check_temperature(sample)
            systemctl, systemd_run, git = (
                executable(name) for name in ("systemctl", "systemd-run", "git")
            )
            slice_properties = policy.show(systemctl, "app.slice")
            slice_cgroup = slice_properties.get("ControlGroup", "")
            policy.require_controllers(policy.cgroup_path(slice_cgroup))
            # Fast refusal before launch; the worker takes and retains the lock.
            lock = policy.acquire_lock()
            os.close(lock)
            source = evidence.source_record(git, Path.cwd())
            if directory.is_relative_to(Path(source["root"])):
                raise RuntimeError(
                    "choose an evidence directory outside the source checkout"
                )
            evidence.save(directory / "source.json", source)
            identity = uuid.uuid4().hex
            state = {
                "unit": f"hearthspace-guarded-cargo-{identity}.service",
                "owner": f"Hearthspace guarded Cargo {identity}",
                "cwd": str(Path.cwd()),
                "evidence": str(directory),
                "argv": argv,
                "cargo_executable_sha256": evidence.file_hash(Path(argv[0])),
                "environment_names": sorted(environment),
                "environment_sha256": policy.digest(environment),
                "systemctl": systemctl,
                "systemd_run": systemd_run,
                "slice_cgroup": slice_cgroup,
                "properties": policy.PROPERTIES | {"CPUAffinity": str(allowed_cpus[0])},
                "caller_allowed_cpus": allowed_cpus,
                "cpu_affinity": allowed_cpus[0],
                "compiler_wrapper_policy": "configured wrappers disabled; Clippy's local driver retained",
                "source_sha256": evidence.file_hash(directory / "source.json"),
                "temperature_policy": "k10temp Tctl >= 85 C: operational cutoff, not a hardware limit",
                "started_ns": time.time_ns(),
                "boot_id": (policy.PROC / "sys/kernel/random/boot_id")
                .read_text()
                .strip(),
                "uname": dict(
                    zip(
                        ("sysname", "nodename", "release", "version", "machine"),
                        os.uname(),
                    )
                ),
            }
            evidence.save(directory / "command.json", state)
            print(f"guarded-cargo: evidence {directory}", flush=True)
            print(
                "guarded-cargo: configured compiler wrappers disabled; "
                f"logical CPU {allowed_cpus[0]}, CPU quota 50%",
                flush=True,
            )
            attempted = True  # A lost launch reply can still mean the unit exists.
            policy.command(service_command(state, environment))
            properties = monitor(state, log)
            success = (
                properties.get("Result") == "success"
                and properties.get("ExecMainCode") == "1"
                and properties.get("ExecMainStatus") == "0"
                and (directory / "executing.json").is_file()
            )
            code = (
                int(properties.get("ExecMainStatus", "1"))
                if properties.get("ExecMainCode") == "1"
                else 1
            )
            result = {
                "outcome": "completed" if success else "failed",
                "exit_code": 0 if success else (code or 1),
                "service": properties,
            }
    except (OSError, ValueError, RuntimeError, KeyError, KeyboardInterrupt) as error:
        result["error"] = str(error)
        print(f"guarded-cargo: {error}", file=sys.stderr)
    finally:
        # A second interrupt must not abandon the bounded cleanup request.
        for sig in previous_handlers:
            signal.signal(sig, signal.SIG_IGN)
        try:
            if attempted:
                try:
                    stop_owned(state)
                    result["cleanup"] = "owned unit stopped"
                except (OSError, ValueError, RuntimeError, KeyError) as error:
                    result.update(
                        outcome="cleanup-failed", exit_code=1, cleanup_error=str(error)
                    )
                    print(
                        "guarded-cargo: cleanup failed; the unit's watchdog and runtime bounds remain in effect",
                        file=sys.stderr,
                    )
            if (directory / "build.log").exists():
                with (directory / "build.log").open("ab") as log:
                    os.fsync(log.fileno())
            evidence.finish(directory, result)
        finally:
            for sig, handler in previous_handlers.items():
                signal.signal(sig, handler)
    return result["exit_code"]


def main(argv=None):
    arguments = sys.argv[1:] if argv is None else argv
    try:
        if len(arguments) == 3 and arguments[0] == "--watchdog":
            return watchdog.relay(Path(arguments[1]), int(arguments[2]))
        if len(arguments) == 2 and arguments[0] == "--worker":
            worker(Path(arguments[1]))
            return 1  # exec is the only successful worker exit.
        parser = argparse.ArgumentParser(description=__doc__)
        parser.add_argument(
            "--evidence",
            required=True,
            help="fresh directory on persistent local storage, outside the checkout",
        )
        parser.add_argument(
            "cargo",
            nargs=argparse.REMAINDER,
            help="build/check/clippy; fixed -j1, one logical CPU, configured compiler wrappers disabled",
        )
        options = parser.parse_args(arguments)
        return supervise(options.cargo, options.evidence)
    except (OSError, ValueError, RuntimeError, KeyError, KeyboardInterrupt) as error:
        print(f"guarded-cargo: {error}", file=sys.stderr)
        return 1
