"""Opt-in fresh desktop settings, created only by the unprivileged session."""

import os
from pathlib import Path


def isolated_environment(state):
    if not state.get("isolated_profile", False):
        return {}
    uid = os.geteuid()
    if uid == 0 or uid != state["uid"]:
        raise RuntimeError("isolated profile must be created by the session user")
    artifacts = Path(state["session_dir"])
    if (
        not artifacts.is_absolute()
        or artifacts.is_symlink()
        or not artifacts.is_dir()
        or artifacts.stat().st_uid != uid
    ):
        raise RuntimeError("isolated profile requires the owned artifact directory")
    # The supervisor requires an empty runtime at startup. Retain profiles with
    # the other run artifacts instead. Never replace HOME: user identity stays real.
    profile = artifacts / "profile"
    profile.mkdir(mode=0o700)
    env = {"GSETTINGS_BACKEND": "keyfile"}
    for variable, leaf in (
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
    ):
        directory = profile / leaf
        directory.mkdir(mode=0o700)
        env[variable] = str(directory)
    return env
