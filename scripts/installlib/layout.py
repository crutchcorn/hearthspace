"""Build a self-contained release before requesting installation privileges."""

import hashlib
import json
import os
import shlex
import shutil
import subprocess
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
BINARIES = (
    "hearthspace-session",
    "hearthspace-app-scope-guard",
    "hearthspace-compositor",
    "hearthspace-shell",
    "hearthspace-settingsd",
    "hearthspace-settings",
    "hearthspace-notifications",
    "hearthspace-a11y",
)
PACKAGES = tuple(
    name
    for name in BINARIES
    if name not in ("hearthspace-app-scope-guard", "hearthspace-a11y")
) + ("waydriver-hearthspace",)
BASE = Path("lib/hearthspace")
MANIFEST = BASE / "install.json"
CURRENT = BASE / "current"
LAUNCHER = Path("bin/hearthspace-login")
SESSION = Path("share/wayland-sessions/hearthspace.desktop")


def prefix_path(value):
    path = Path(value)
    if (
        not path.is_absolute()
        or path == Path("/")
        or ".." in path.parts
        or any(char in str(path) for char in "\n\r\0:%")
    ):
        raise ValueError(
            "prefix must be absolute, without parent components, ':', '%' or newlines"
        )
    return path


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def metadata(path, mode):
    return {"sha256": digest(path), "mode": mode}


def launcher(prefix):
    current = shlex.quote(str(prefix / CURRENT))
    return f"""#!/bin/sh
# Installed by Hearthspace. Resolve once so upgrades cannot mix release binaries.
set -eu
release=$(readlink -f -- {current})
test -x "$release/bin/hearthspace-session" || {{ echo "Hearthspace release is unavailable" >&2; exit 1; }}
PATH="$release/bin:${{PATH:-/usr/local/bin:/usr/bin:/bin}}"
XDG_DATA_DIRS="$release/share:${{XDG_DATA_DIRS:-/usr/local/share:/usr/share}}"
export PATH XDG_DATA_DIRS
unset HEARTHSPACE_DEV HEARTHSPACE_REPO_ROOT
exec "$release/bin/hearthspace-session" --installed --backend native --login-session --no-watch-apps --binary-dir "$release/bin" "$@"
""".encode()


def session_entry(prefix):
    executable = str(prefix / LAUNCHER)
    # Exec is desktop-entry quoting, not shell quoting. Prefix validation excludes
    # field codes and control characters; double escaping survives key parsing.
    escaped = executable.replace("\\", "\\\\\\\\")
    for char in ('"', "`", "$"):
        escaped = escaped.replace(char, "\\\\" + char)
    content = (REPOSITORY / "data/wayland-sessions/hearthspace.desktop").read_text()
    lines = []
    for line in content.split("\n"):
        if line.startswith("Exec="):
            line = f'Exec="{escaped}"'
        elif line.startswith("TryExec="):
            line = "TryExec=" + executable.replace("\\", "\\\\")
        lines.append(line)
    return "\n".join(lines).encode()


def build(profile):
    if os.geteuid() == 0:
        raise RuntimeError("build as your normal user; do not run cargo through sudo")
    command = [
        "cargo",
        "build",
        "--locked",
        "--target-dir",
        str(REPOSITORY / "target"),
        "--bins",
        "--features",
        "hearthspace-compositor/udev",
    ]
    if profile == "release":
        command.append("--release")
    for package in PACKAGES:
        command.extend(["-p", package])
    subprocess.run(command, cwd=REPOSITORY, check=True)


def bundle(directory, binary_dir, prefix):
    """Copy user-built artifacts into an immutable, hashed installation input."""
    release = directory / "release"
    files = {}
    sources = {Path("bin") / name: binary_dir / name for name in BINARIES}
    sources[Path("bin/hearthspace-login-guard")] = (
        REPOSITORY / "scripts/runtime/hearthspace-login-guard"
    )
    sources[Path("share/applications/dev.hearthspace.Settings.desktop")] = (
        REPOSITORY / "data/applications/dev.hearthspace.Settings.desktop"
    )
    sources[Path("share/icons/hicolor/scalable/apps/hearthspace.svg")] = (
        REPOSITORY / "assets/logo.svg"
    )
    for relative, source in sources.items():
        if not source.is_file() or source.is_symlink():
            raise RuntimeError(
                f"required runtime file is missing or a symlink: {source}"
            )
        mode = 0o755 if relative.parts[0] == "bin" else 0o644
        if mode == 0o755 and not os.access(source, os.X_OK):
            raise RuntimeError(f"runtime file is not executable: {source}")
        target = release / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        target.chmod(mode)
        files[str(relative)] = metadata(target, mode)
    settings = release / "share/applications/dev.hearthspace.Settings.desktop"
    settings.write_text(settings.read_text() + "Icon=hearthspace\n")
    files["share/applications/dev.hearthspace.Settings.desktop"] = metadata(
        settings, 0o644
    )
    release_id = hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()
    stable = {str(LAUNCHER): launcher(prefix), str(SESSION): session_entry(prefix)}
    for relative, content in stable.items():
        target = directory / "stable" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
        target.chmod(0o755 if relative == str(LAUNCHER) else 0o644)
    spec = {"version": 1, "prefix": str(prefix), "release": release_id, "files": files}
    (directory / "bundle.json").write_text(json.dumps(spec, sort_keys=True))
    return spec


def dependencies():
    commands = [
        "python3",
        "dbus-run-session",
        "systemctl",
        "loginctl",
        "swaylock",
        "Xwayland",
        "readlink",
        "systemd-run",
        "fusermount3",
    ]
    absolute = [
        "/usr/libexec/xdg-permission-store",
        "/usr/libexec/xdg-document-portal",
        "/usr/libexec/xdg-desktop-portal-gtk",
        "/usr/libexec/xdg-desktop-portal",
        "/usr/libexec/polkit-mate-authentication-agent-1",
    ]
    missing = [name for name in commands if shutil.which(name) is None]
    missing.extend(name for name in absolute if not os.access(name, os.X_OK))
    if missing:
        raise RuntimeError("missing session dependencies: " + ", ".join(missing))
