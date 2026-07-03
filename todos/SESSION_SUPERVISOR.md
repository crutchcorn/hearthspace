# Hearthspace Session Supervisor Implementation Plan

This document describes how to scaffold Hearthspace so that the compositor, shell, settings daemon, and development-only helper processes can be launched together from a single command.

The intended developer workflow is:

```sh
cargo run
```

from the repository root.

That command should start a full Hearthspace development session, including:

- `hearthspace-settingsd`
- `hearthspace-compositor`
- `hearthspace-shell`
- development app registration for apps like `hearthspace-settings`
- optional background rebuild/watch processes for development apps

The design should also grow cleanly into an installed desktop session later.

## Goals

### Primary goals

- Make `cargo run` start a complete Hearthspace session.
- Keep the compositor, shell, and settings daemon as separate binaries.
- Provide a session supervisor that owns process startup, shutdown, environment variables, and readiness coordination.
- Let the shell launch Hearthspace-owned apps like `hearthspace-settings` through normal app search.
- Support development workflows where Hearthspace apps are rebuilt automatically when their code changes.
- Preserve the ability to run each component directly for debugging.
- Keep the design compatible with future installed sessions, display managers, and systemd user units.

### Non-goals

- Do not collapse the compositor, shell, and settings daemon into one binary.
- Do not make `hearthspace-settingsd` responsible for UI.
- Do not require a full system install for development.
- Do not require the settings UI to be embedded in the shell.
- Do not hard-code paths that only work on one developer machine.

## Proposed workspace layout

Recommended crate layout:

```txt
crates/
  hearthspace-session/       # cargo run target; starts the full dev session
  hearthspace-compositor/    # Wayland compositor
  hearthspace-shell/         # Xilem shell UI
  hearthspace-settingsd/     # settings daemon
  hearthspace-settings/      # Xilem settings UI app
  hearthspace-common/        # shared paths, logging, config, errors, process utilities
  hearthspace-ipc/           # shared IPC schemas and client/server helpers
```

The root workspace should make `hearthspace-session` the default member so that `cargo run` from the repo root starts the session supervisor.

Example root `Cargo.toml` shape:

```toml
[workspace]
resolver = "2"
members = [
  "crates/hearthspace-session",
  "crates/hearthspace-compositor",
  "crates/hearthspace-shell",
  "crates/hearthspace-settingsd",
  "crates/hearthspace-settings",
  "crates/hearthspace-common",
  "crates/hearthspace-ipc",
]

default-members = ["crates/hearthspace-session"]
```

Each component should still remain directly runnable:

```sh
cargo run -p hearthspace-session
cargo run -p hearthspace-compositor
cargo run -p hearthspace-shell
cargo run -p hearthspace-settingsd
cargo run -p hearthspace-settings
```

## Process tree

During development, `cargo run` should create a process tree like this:

```txt
hearthspace-session
├─ hearthspace-settingsd
├─ hearthspace-compositor
├─ hearthspace-shell
├─ cargo watch/build process for hearthspace-settings          # optional but recommended
└─ other dev helper processes                                 # optional future expansion
```

The session supervisor is responsible for:

- selecting a Wayland socket name
- creating runtime paths
- starting `settingsd`
- starting the compositor
- waiting for compositor readiness
- starting the shell
- registering development apps
- starting file watchers/build helpers
- forwarding relevant environment variables
- shutting down children when the session ends
- reporting clear logs when anything fails

## Development vs installed mode

The session supervisor should support two process-launch modes.

### Development mode

Development mode is used when running from the repository with:

```sh
cargo run
```

In this mode, the session supervisor may spawn child processes using:

```sh
cargo run -p <package> --
```

This is slower than launching binaries directly but convenient during early development because it avoids requiring an install step.

Example:

```sh
cargo run -p hearthspace-settingsd --
cargo run -p hearthspace-compositor -- --wayland-display hearthspace-dev-12345
cargo run -p hearthspace-shell --
```

### Installed mode

Installed mode is used later by a display manager or installed desktop session.

In this mode, `hearthspace-session` should spawn installed binaries directly:

```txt
/usr/bin/hearthspace-settingsd
/usr/bin/hearthspace-compositor
/usr/bin/hearthspace-shell
/usr/bin/hearthspace-settings
```

The supervisor should not assume that `cargo` or the source tree exists in installed mode.

### Recommended mode detection

For now, use explicit CLI flags:

```sh
cargo run -- --dev
hearthspace-session --installed
```

Default behavior:

- When run through `cargo run`, default to development mode.
- When launched as an installed binary, default to installed mode.
- Allow override flags either way.

Potential implementation options:

- Detect `HEARTHSPACE_DEV=1`
- Detect whether `CARGO_MANIFEST_DIR` or repo root is known
- Use explicit `--dev` / `--installed` flags
- Use `cfg(debug_assertions)` as a fallback, but do not rely on it as the only mechanism

## Runtime paths

Use XDG paths and avoid global writable locations.

Important paths:

```txt
$XDG_RUNTIME_DIR/hearthspace/
  settings.sock
  session.sock
  compositor-ready.json
  dev-applications/

$XDG_RUNTIME_DIR/<WAYLAND_DISPLAY>
  Wayland socket created by compositor
```

Recommended per-session runtime directory:

```txt
$XDG_RUNTIME_DIR/hearthspace/session-<pid>/
```

Example:

```txt
/run/user/1000/hearthspace/session-42691/
  settings.sock
  compositor-ready.json
  applications/
    hearthspace-settings.desktop
```

Use a per-session directory so multiple nested/dev sessions can coexist without stomping each other.

## Environment variables

The session supervisor should construct and pass a consistent environment to children.

Recommended environment variables:

```txt
WAYLAND_DISPLAY=hearthspace-dev-<pid>
XDG_CURRENT_DESKTOP=Hearthspace
DESKTOP_SESSION=hearthspace
HEARTHSPACE_DEV=1
HEARTHSPACE_SESSION_DIR=$XDG_RUNTIME_DIR/hearthspace/session-<pid>
HEARTHSPACE_SETTINGS_SOCKET=$XDG_RUNTIME_DIR/hearthspace/session-<pid>/settings.sock
HEARTHSPACE_SESSION_SOCKET=$XDG_RUNTIME_DIR/hearthspace/session-<pid>/session.sock
HEARTHSPACE_REPO_ROOT=/absolute/path/to/repo              # dev mode only
```

For shell-launched apps, the shell should inherit and forward at least:

```txt
WAYLAND_DISPLAY
XDG_CURRENT_DESKTOP
DESKTOP_SESSION
HEARTHSPACE_SETTINGS_SOCKET
HEARTHSPACE_SESSION_SOCKET
```

## Startup order

Recommended startup order:

```txt
1. Initialize logging.
2. Resolve runtime paths.
3. Generate session-specific environment.
4. Register dev apps by generating .desktop files.
5. Start hearthspace-settingsd.
6. Wait for settingsd readiness.
7. Start hearthspace-compositor with explicit Wayland display name.
8. Wait for compositor readiness.
9. Start hearthspace-shell.
10. Start optional dev watchers/build processes.
11. Monitor children until the session exits.
12. Shut down remaining children cleanly.
```

`settingsd` should start before the compositor and shell so both can query settings on startup.

The compositor should start before the shell because the shell is a Wayland client.

The shell should start after the compositor has created its Wayland socket and is ready to accept clients.

## Readiness

Avoid arbitrary sleeps.

### Settings daemon readiness

`hearthspace-settingsd` should support at least one readiness mechanism:

- create its Unix socket
- respond to a `ping` IPC request
- optionally write a readiness message to a provided file descriptor

Recommended first-class readiness behavior:

```sh
hearthspace-settingsd --socket "$HEARTHSPACE_SETTINGS_SOCKET" --ready-fd 3
```

When ready, it writes a newline-delimited JSON message to the FD:

```json
{"service":"settingsd","status":"ready"}
```

The session supervisor should still support a fallback that waits for the socket file and then sends a ping.

### Compositor readiness

The compositor should accept:

```sh
hearthspace-compositor --wayland-display hearthspace-dev-<pid> --ready-fd 3
```

When ready, it writes:

```json
{"service":"compositor","status":"ready","wayland_display":"hearthspace-dev-42691"}
```

Fallback behavior:

- wait for `$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY` to exist
- optionally try connecting to the Wayland socket

### Shell readiness

The shell can optionally support readiness later, but it is not required for the session to continue.

If implemented:

```sh
hearthspace-shell --ready-fd 3
```

Ready message:

```json
{"service":"shell","status":"ready"}
```

## Process supervision

The session supervisor should monitor children and define failure behavior explicitly.

Recommended behavior:

### Critical processes

These are critical:

- `hearthspace-settingsd`
- `hearthspace-compositor`
- `hearthspace-shell`

If the compositor exits, shut down the whole session.

If `settingsd` exits, either:

- shut down the whole session, or
- restart it if the exit looks crash-like and restart count is below a limit

If the shell exits, restart it during development unless the user requested session shutdown.

Suggested development defaults:

```txt
compositor exits -> shut down session
settingsd exits  -> restart up to 3 times, then shut down session
shell exits      -> restart up to 3 times, then shut down session
watcher exits    -> log warning, do not shut down session
```

### Graceful shutdown

On `SIGINT`, `SIGTERM`, compositor exit, or unrecoverable child failure:

1. Ask shell to exit.
2. Ask compositor to exit.
3. Ask settingsd to exit.
4. Terminate dev watchers.
5. Wait with timeout.
6. Kill remaining children.
7. Clean up generated runtime files.

The session supervisor should avoid orphaned `cargo run`, compositor, shell, or settingsd processes.

## Logging

Use `tracing` across components.

The session supervisor should produce clear structured logs:

```txt
[session] runtime dir: /run/user/1000/hearthspace/session-42691
[session] WAYLAND_DISPLAY=hearthspace-dev-42691
[session] starting settingsd
[session] settingsd ready
[session] starting compositor
[session] compositor ready
[session] starting shell
[session] generated desktop entry: hearthspace-settings.desktop
[session] starting settings app watcher
```

Child logs should be prefixed by process name if possible:

```txt
[settingsd] ...
[compositor] ...
[shell] ...
[settings-watch] ...
```

If prefixing stdout/stderr is too much for the first implementation, ensure each child initializes `tracing` with its own target/module names.

## Development app registration

The session supervisor should generate temporary `.desktop` files for Hearthspace development apps.

This lets the shell's app search discover and launch Hearthspace apps naturally, without requiring installation.

Generated desktop directory:

```txt
$HEARTHSPACE_SESSION_DIR/share/applications/
```

Generated desktop file:

```txt
$HEARTHSPACE_SESSION_DIR/share/applications/hearthspace-settings.desktop
```

Example:

```ini
[Desktop Entry]
Type=Application
Name=Hearthspace Settings
Comment=Configure Hearthspace
Exec=/absolute/path/to/repo/scripts/dev-run-app hearthspace-settings
Icon=hearthspace
Categories=Settings;System;
X-Hearthspace-Dev=true
```

The shell/app launcher should include this directory when searching for applications.

Recommended environment:

```txt
XDG_DATA_DIRS=$HEARTHSPACE_SESSION_DIR:/usr/local/share:/usr/share
```

This matches the normal XDG lookup shape because the generated file lives under `$HEARTHSPACE_SESSION_DIR/share/applications`.

## Development app launching

Use a wrapper script for development-launched Hearthspace apps.

Example path:

```txt
scripts/dev-run-app
```

Example implementation:

```sh
#!/usr/bin/env bash
set -euo pipefail

APP="$1"
REPO_ROOT="${HEARTHSPACE_REPO_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"

cd "$REPO_ROOT"

if [[ ! -x "target/debug/$APP" ]]; then
  cargo build -p "$APP"
fi

exec "$REPO_ROOT/target/debug/$APP"
```

The `.desktop` file should call this script.

This ensures the settings app can still launch even if it has not been built yet.

## Automatic rebuilds for settings UI app

To make changes to `hearthspace-settings` available naturally from shell search, the session supervisor should optionally run a watcher.

Recommended command:

```sh
cargo watch -w crates/hearthspace-settings -w crates/hearthspace-common -w crates/hearthspace-ipc -x "build -p hearthspace-settings"
```

Alternative if using `watchexec`:

```sh
watchexec \
  -w crates/hearthspace-settings \
  -w crates/hearthspace-common \
  -w crates/hearthspace-ipc \
  -- cargo build -p hearthspace-settings
```

The watcher should be optional and development-only.

Possible CLI flags:

```sh
cargo run -- --watch-apps
cargo run -- --no-watch-apps
```

Recommended default in dev mode:

```txt
watch apps: enabled if cargo-watch or watchexec is installed
watch apps: disabled with a clear warning if neither is installed
```

Example warning:

```txt
[session] cargo-watch not found; Hearthspace Settings will build on first launch instead of rebuilding automatically
```

## App search integration

The shell's app search should be able to find:

- normal system apps
- temporary Hearthspace development apps
- future installed Hearthspace apps

The app indexer should scan:

```txt
$HEARTHSPACE_SESSION_DIR/share/applications
~/.local/share/applications
/usr/local/share/applications
/usr/share/applications
```

If the app indexer relies on `XDG_DATA_DIRS`, ensure the session supervisor passes a modified `XDG_DATA_DIRS` to the shell.

The generated settings app should show up when searching for:

```txt
settings
hearthspace
config
preferences
```

## IPC expectations

`settingsd` should expose an IPC API for:

- get setting
- set setting
- list settings
- watch setting changes
- validate setting
- reset setting to default
- export effective config
- report config file paths

The settings UI should use the same IPC API as the shell and compositor.

The settings UI should not write config files directly.

The compositor and shell should subscribe to relevant setting changes and apply live changes where possible.

Example logical flow:

```txt
hearthspace-settings app
→ settingsd.set("canvas.show_grid", false)
→ settingsd validates and writes config atomically
→ settingsd emits change event
→ shell/compositor receive change event
→ canvas grid updates
```

## Config file locations

The session supervisor should not own settings storage, but all components should agree on XDG paths.

User config:

```txt
~/.config/hearthspace/config.toml
~/.config/hearthspace/keybindings.toml
~/.config/hearthspace/apps.toml
```

Runtime IPC:

```txt
$XDG_RUNTIME_DIR/hearthspace/session-<pid>/settings.sock
```

State:

```txt
~/.local/state/hearthspace/
```

Cache:

```txt
~/.cache/hearthspace/
```

## CLI shape

Recommended `hearthspace-session` CLI:

```txt
Usage: hearthspace-session [OPTIONS]

Options:
  --dev
      Run child processes through `cargo run -p ...`.

  --installed
      Run installed child binaries directly.

  --wayland-display <NAME>
      Wayland display socket name. Defaults to hearthspace-dev-<pid> in dev mode.

  --watch-apps
      Start development watchers for Hearthspace apps.

  --no-watch-apps
      Do not start development watchers.

  --no-shell-restart
      Do not restart shell if it exits.

  --no-settingsd-restart
      Do not restart settingsd if it exits.

  --log-level <LEVEL>
      Set log level for session and children.

  --print-env
      Print the generated session environment.

  --dry-run
      Print what would be started without starting processes.
```

Use `clap` for CLI parsing.

## Suggested Rust crates

Likely useful dependencies:

```toml
anyhow = "1"
camino = "1"
clap = { version = "4", features = ["derive", "env"] }
directories = "6"
nix = { version = "0.30", features = ["signal", "process"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["macros", "process", "rt-multi-thread", "signal", "time"] }
tracing = "0.1"
tracing-subscriber = "0.3"
which = "8"
```

Adjust versions to match the existing workspace.

## Session supervisor implementation outline

High-level pseudo-Rust:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();

    let args = Args::parse();
    let mode = resolve_mode(&args)?;
    let paths = SessionPaths::create(&args)?;
    let env = SessionEnv::new(&paths, &args)?;

    generate_dev_desktop_entries(&paths, &env)?;

    let mut supervisor = Supervisor::new(paths, env, mode);

    supervisor.start_settingsd().await?;
    supervisor.wait_for_settingsd_ready().await?;

    supervisor.start_compositor().await?;
    supervisor.wait_for_compositor_ready().await?;

    supervisor.start_shell().await?;

    if supervisor.should_watch_apps() {
        supervisor.start_settings_app_watcher().await?;
    }

    supervisor.monitor_until_exit().await?;
    supervisor.shutdown().await?;

    Ok(())
}
```

Core types:

```rust
struct SessionPaths {
    runtime_dir: Utf8PathBuf,
    session_dir: Utf8PathBuf,
    applications_dir: Utf8PathBuf,
    settings_socket: Utf8PathBuf,
    session_socket: Utf8PathBuf,
}

struct SessionEnv {
    wayland_display: String,
    vars: Vec<(String, String)>,
}

enum RunMode {
    DevCargo,
    InstalledBinaries,
}

struct ManagedProcess {
    name: String,
    critical: bool,
    restart_policy: RestartPolicy,
    child: tokio::process::Child,
}

enum RestartPolicy {
    Never,
    OnCrash { max_restarts: usize },
}
```

## Acceptance criteria

This task is complete when:

1. `cargo run` from the workspace root starts `hearthspace-session`.
2. `hearthspace-session` starts `hearthspace-settingsd`.
3. `hearthspace-session` starts `hearthspace-compositor`.
4. `hearthspace-session` waits for compositor readiness without arbitrary sleeps.
5. `hearthspace-session` starts `hearthspace-shell`.
6. The shell can launch normal apps using the compositor's `WAYLAND_DISPLAY`.
7. The session generates a dev `.desktop` entry for `hearthspace-settings`.
8. The shell app search can find and launch `Hearthspace Settings`.
9. The settings app runs as a normal Wayland client inside Hearthspace.
10. The settings app can communicate with `hearthspace-settingsd`.
11. In dev mode, the settings app is rebuilt automatically when its code changes if a watcher tool is available.
12. If watcher tooling is unavailable, launching the settings app builds it on demand.
13. If the compositor exits, the session supervisor shuts down the shell, settings daemon, and watchers.
14. `Ctrl+C` on the session supervisor shuts down all child processes cleanly.
15. Logs clearly show process startup, readiness, and shutdown.
16. Each component remains directly runnable with `cargo run -p <component>`.

## Future installed-session direction

Later, this architecture can map to a real desktop session.

Possible installed files:

```txt
/usr/bin/hearthspace-session
/usr/bin/hearthspace-compositor
/usr/bin/hearthspace-shell
/usr/bin/hearthspace-settingsd
/usr/bin/hearthspace-settings

/usr/share/wayland-sessions/hearthspace.desktop
/usr/share/applications/hearthspace-settings.desktop
```

Display manager session file:

```ini
[Desktop Entry]
Name=Hearthspace
Comment=A spatial Linux desktop for persistent, local-first computing
Exec=hearthspace-session --installed
Type=Application
DesktopNames=Hearthspace
```

Future systemd user direction:

```txt
hearthspace-session.target
├─ hearthspace-settingsd.service
├─ hearthspace-compositor.service
└─ hearthspace-shell.service
```

Do not start with systemd user units unless they become useful. The Rust session supervisor should remain the main development entrypoint for now.

## Design principle

The session supervisor should make development feel like a real desktop session without requiring the project to be installed as a real desktop session.

The intended experience:

```sh
cargo run
```

Then, inside Hearthspace:

1. Search for apps.
2. Launch normal Linux apps.
3. Search for “Hearthspace Settings.”
4. Launch the settings UI as a normal app.
5. Edit settings.
6. Watch shell/compositor behavior update through `settingsd`.

This keeps the compositor, shell, settings daemon, and settings app separate while making the whole system feel integrated and dogfoodable.
