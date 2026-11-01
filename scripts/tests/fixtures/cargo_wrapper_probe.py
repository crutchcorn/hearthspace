"""Small fake-Cargo boundary for wrapper precedence, never a compiler or daemon.

Only test-owned config files and the tested string/table cases are modeled.
Cargo remains responsible for real discovery, includes, validation and merging.
The precedence contract comes from Cargo's config/environment reference and
GlobalContext::{maybe_get_tool, env_config}; Clippy supplies its own driver.
"""

import tomllib
from pathlib import Path

NAMES = (
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
)


def merge(base, overlay):
    for key, value in overlay.items():
        if key in base and type(base[key]) is not type(value):
            raise ValueError("fixture Cargo config type conflict")
        if isinstance(value, dict):
            merge(base.setdefault(key, {}), value)
        else:
            base[key] = value


def child_environment(config, environment):
    child = dict(environment)
    for name, value in config.get("env", {}).items():
        if isinstance(value, str):
            value = {"value": value}
        if name not in child or value.get("force", False):
            child[name] = value["value"]
            if value.get("relative", False):
                child[name] = str(Path.cwd() / child[name])
    return child


def probe(root, arguments, environment):
    config, loaded = {}, []
    ancestors = [p for p in (Path.cwd(), *Path.cwd().parents) if p.is_relative_to(root)]
    directories = [Path(environment["CARGO_HOME"])] + [
        p / ".cargo" for p in reversed(ancestors)
    ]
    for directory in directories:
        path = directory / "config"
        if not path.exists():
            path = directory / "config.toml"
        if path.exists():
            merge(config, tomllib.loads(path.read_text()))
            loaded.append(str(path))
    for name, key in zip(NAMES[2:], ("rustc-wrapper", "rustc-workspace-wrapper")):
        if name in environment:
            config.setdefault("build", {})[key] = environment[name]
    args = iter(arguments)
    for argument in args:
        if argument == "--":
            break
        if argument == "--config":
            value = next(args)
        elif argument.startswith("--config="):
            value = argument.split("=", 1)[1]
        else:
            continue
        path = Path(value)
        if path.is_file():
            loaded.append(str(path))
            value = path.read_text()
        merge(config, tomllib.loads(value))
    child = child_environment(config, environment)
    if arguments[0] == "clippy":
        # cargo-clippy replaces this variable before invoking its child Cargo.
        child["RUSTC_WORKSPACE_WRAPPER"] = "fixture-clippy-driver"
        child = child_environment(config, child)
    effective = {
        name: child.get(name, config.get("build", {}).get(key, ""))
        for name, key in zip(NAMES[:2], ("rustc-wrapper", "rustc-workspace-wrapper"))
    }
    expected = {"RUSTC_WRAPPER": "", "RUSTC_WORKSPACE_WRAPPER": ""}
    if arguments[0] == "clippy":
        expected["RUSTC_WORKSPACE_WRAPPER"] = "fixture-clippy-driver"
    if effective != expected or any(child.get(name) for name in NAMES[2:]):
        raise RuntimeError("fixture detected a configured compiler wrapper")
    return {"loaded": loaded, "config": config, "effective": effective}
