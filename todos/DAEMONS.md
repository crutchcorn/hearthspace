# Daemons

Hearthspace now has standalone helper daemons, starting with
`hearthspace-settingsd`. These should remain real binaries rather than hidden
`hearthspace` modes so they can be launched, tested, packaged, and eventually
socket-activated independently.

## Current State

- `hearthspace-settingsd` is a standalone Cargo binary.
- `hearthspace-settings` is a standalone Xilem frontend binary.
- The settings frontend connects to `$XDG_RUNTIME_DIR/hearthspace-settingsd.sock`.
- The settings daemon writes `~/.config/hearthspace/config.toml`.
- Nothing currently autostarts `hearthspace-settingsd`; it must be launched
  manually before `hearthspace-settings` can persist changes.

## Near-Term Autostart

The compositor should eventually start Hearthspace-owned daemons during session
startup, alongside the shell.

For `settingsd`, this likely means:

- Add daemon spawning to compositor startup, near shell spawning.
- Use `std::env::current_exe()` only for the main `hearthspace` binary; helper
  daemons are separate executables, so production startup needs a reliable way to
  locate sibling binaries such as `hearthspace-settingsd`.
- Preserve the user's runtime/config environment, especially `XDG_RUNTIME_DIR`,
  `HOME`, and `XDG_CONFIG_HOME`.
- Log daemon spawn failures, but do not fail compositor startup.
- Treat an already-running daemon as non-fatal.
- Decide whether the compositor should keep child handles and terminate daemons
  on shutdown, or leave process lifetime to the session/runtime manager.

The minimal acceptable first version is fire-and-forget spawn with clear logging,
provided the daemon exits cleanly when another instance is already listening.

## Daemon Robustness Requirements

Before compositor autostart, each daemon should be safe to start repeatedly:

- Check whether its socket already has a live listener.
- Exit cleanly or return a distinct non-error status when already running.
- Remove stale socket files only after verifying no listener is active.
- Keep protocol errors scoped to the client connection.
- Avoid panicking on malformed client input.
- Keep config writes atomic once settings become more important than the current
  scaffold.

## Frontend Behavior

Frontend apps should not own settings persistence directly once a daemon exists.

Settings frontends should:

- Connect to the daemon socket.
- Show a clear unavailable/backend-disconnected status if the daemon is missing.
- Retry or expose a refresh action once the UI grows beyond the current scaffold.
- Avoid writing `config.toml` directly except in dedicated migration/recovery
  tooling.

## Future Systemd User Integration

Once Hearthspace has install packaging, daemon startup should move toward systemd
user services or socket activation.

Likely units:

- `hearthspace-settingsd.service`
- `hearthspace-settingsd.socket`

Benefits:

- Daemons start on demand when a client connects.
- Process lifecycle is managed outside the compositor.
- Crashes and restarts can use standard user-service policy.
- The compositor and frontend only need to know the socket path.

Open packaging questions:

- Where unit files are installed.
- Whether Hearthspace enables units during install or first run.
- How native sessions, nested sessions, and development runs should differ.
- Whether daemon sockets should always live under the user's real
  `XDG_RUNTIME_DIR` or a Hearthspace-specific nested runtime directory.

## Testing Expectations

Daemon changes should have focused tests that do not depend on global user state:

- Inject config paths rather than mutating `HOME` or `XDG_CONFIG_HOME` when
  possible.
- Use temporary directories for config/socket paths.
- Use `UnixStream::pair()` for request/response tests where a real bound socket
  is unnecessary.
- Add one smoke test with a real `UnixListener` only when socket binding behavior
  itself is under test.
