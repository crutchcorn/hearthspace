# Settings Architecture

Hearthspace settings are owned by `hearthspace-settingsd`. Other processes should treat the daemon as the settings authority and should not write the settings file directly.

The intended boundary is:

```text
settings UI / compositor / shell clients
-> hearthspace-ipc client helpers
-> Unix socket request
-> hearthspace-settingsd
-> config file on disk
```

This keeps persistence, defaults, request validation, and response formatting in one place while allowing the settings app to stay a normal Wayland client.

## Processes

`hearthspace-session` starts `hearthspace-settingsd` before the compositor and shell. It waits until the daemon answers a `get` request before continuing session startup.

The session supervisor gives all child processes the same settings socket path through `HEARTHSPACE_SETTINGS_SOCKET`. In a supervised session this socket lives in the per-session runtime directory:

```text
$XDG_RUNTIME_DIR/hearthspace/session-<pid>/settings.sock
```

If `HEARTHSPACE_SETTINGS_SOCKET` is not set, `hearthspace-ipc` falls back to:

```text
$XDG_RUNTIME_DIR/hearthspace-settingsd.sock
```

The daemon creates the socket parent directory, checks whether an existing listener is already running, removes stale socket files, and then accepts one-line requests on a Unix domain socket. A second daemon instance exits cleanly when it can connect to the existing socket.

`hearthspace-session` restarts `hearthspace-settingsd` up to the configured restart limit. If the daemon exits and cannot be restarted, the session treats that as a fatal settings backend failure.

## Persistence

The settings file path comes from `hearthspace_common::config_path()`:

```text
$XDG_CONFIG_HOME/hearthspace/config.toml
```

If `XDG_CONFIG_HOME` is not set, Hearthspace uses:

```text
~/.config/hearthspace/config.toml
```

`hearthspace-settingsd` reads this file on every request. A missing file is not an error; the daemon returns `Settings::default()` and does not create the file until the first successful write.

Writes are atomic at the file level. The daemon writes the complete formatted settings to a temporary file next to the config file, syncs that temporary file, and renames it over `config.toml`. Failed renames clean up the temporary file when possible.

The file is named `config.toml` and uses Hearthspace's settings schema. The current implementation uses a small parser and formatter in `hearthspace-settingsd` rather than a general TOML library, so the daemon should remain the compatibility boundary for the file format.

The daemon currently formats the file like this:

```text
test = false
background_moves_with_canvas = true

[[monitor]]
id = primary
name = Primary
width = 1920
height = 1080
scale = 1
x = 0
y = 0

[[monitor]]
id = secondary
name = Secondary
width = 1920
height = 1080
scale = 1
x = 1920
y = 0
```

Unknown top-level keys and unknown monitor keys are ignored. Invalid booleans, invalid integers, invalid positive integers, and malformed key/value lines make parsing fail for that request. Monitor entries without an `id` are dropped. If no valid monitors remain, the daemon restores the default monitor list.

## Settings Model

The shared settings model lives in `hearthspace-ipc`:

```text
Settings
- test: bool
- background_moves_with_canvas: bool
- monitors: Vec<MonitorConfig>

MonitorConfig
- id: String
- name: String
- width: i32
- height: i32
- scale: i32
- x: i32
- y: i32
```

Defaults are deliberately useful in development and headless cases:

```text
test = false
background_moves_with_canvas = true
primary: 1920x1080 scale 1 at 0,0
secondary: 1920x1080 scale 1 at 1920,0
```

Monitor `x` and `y` are logical desktop positions. `scale` must be positive. Monitor IDs cannot be empty or contain whitespace when sent through the IPC client helpers.

Monitor layout is the settings model's active compositor consumer today. `background_moves_with_canvas` is persisted, returned through IPC, and exposed by the settings app; compositor background rendering still uses its compiled default until that rendering path is wired to settings.

## IPC Protocol

The wire protocol is intentionally small and line-oriented. Clients send one request line, shut down the write side of the socket, and read the complete response.

Supported requests are:

```text
get
set test <true|false>
set background-moves-with-canvas <true|false>
set monitor <id> <x> <y>
set monitor-scale <id> <scale>
```

Successful responses start with `ok` and include a complete settings snapshot:

```text
ok test false background-moves-with-canvas true monitor primary 1920 1080 1 0 0 Primary monitor secondary 1920 1080 1 1920 0 Secondary
```

Error responses start with `err`:

```text
err unknown monitor id: missing
```

Tokens that may contain spaces or punctuation are percent-encoded in IPC responses. This applies to monitor IDs and names. The config file itself stores the daemon's formatted values, not the IPC encoding.

All normal clients should use the `hearthspace-ipc` helper functions instead of constructing protocol lines directly:

```text
client_get_settings()
client_set_test(test)
client_set_background_moves_with_canvas(value)
client_set_monitor_position(id, x, y)
client_set_monitor_scale(id, scale)
```

These helpers resolve the socket path, validate client-side inputs where appropriate, send the request, parse the response, and return a full `Settings` value.

## Settings App

`hearthspace-settings` is a normal Xilem Wayland app. It is launched through the app catalog like other desktop applications, not embedded into the compositor.

The source-tree desktop entry is:

```text
data/applications/dev.hearthspace.Settings.desktop
```

During development sessions, `hearthspace-session` also generates a session-local desktop entry that runs the settings app through `scripts/dev-run-app`. The session-local entry is placed ahead of the source-tree data directory through `XDG_DATA_DIRS`, so the launcher finds the dev-aware command first.

The shell preserves session environment variables when launching apps, including `HEARTHSPACE_SETTINGS_SOCKET`. This is what connects `hearthspace-settings` to the daemon for the current session.

On startup, the settings app:

- Calls `client_get_settings()`.
- Falls back to `Settings::default()` if the daemon is unavailable.
- Probes live Wayland outputs through `wl_output` globals.
- Merges live output geometry with saved monitor placement and scale.
- Displays the resolved config path from `hearthspace_common::config_path()`.

The app never writes `config.toml` directly. User actions update local UI state optimistically, then call the daemon through `hearthspace-ipc`:

- The `test` checkbox calls `client_set_test()`.
- The `Move background with canvas` checkbox calls `client_set_background_moves_with_canvas()`.
- Dragging a monitor commits positions through `client_set_monitor_position()` for each monitor.
- Changing monitor scale calls `client_set_monitor_scale()` and then saves recalculated monitor positions.

On success, the app replaces its local state with the daemon's returned settings snapshot, merged again with live output data. On failure, it restores the previous local state where needed and shows the error in the status label.

## Monitor Layout

Saved monitor settings are shared between the settings app and compositor.

The settings app uses live Wayland outputs as the physical source of truth for monitor name, size, and current output scale. It uses persisted settings for user-controlled placement and preferred scale when a saved monitor can be matched by output ID, output name, or role ID.

Role IDs provide stable placeholders before real hardware identities are known:

```text
primary
secondary
secondary-2
secondary-3
```

If the saved monitor list is still the default placeholder list, the first two live outputs keep the `primary` and `secondary` role IDs. Otherwise, live output IDs are preferred for newly discovered outputs.

The compositor loads monitor layout through `client_get_settings()` and converts the returned monitors into an `OutputLayout`. On the native udev backend, it refreshes the settings-backed output layout periodically and applies changes without restarting the compositor. When output geometry changes, the compositor reconfigures shell bars, reconciles the pointer, and schedules a redraw.

Layout application follows these rules:

- Match saved monitors by output name or saved monitor name.
- Use role fallback when appropriate.
- Apply saved positive scales before computing placement.
- Place outputs with saved positions first.
- Put any unplaced outputs to the right of the placed group.
- Normalize the resulting layout so the minimum x/y starts at zero.
- Repair overlapping active monitor layouts by falling back to edge-to-edge placement.

If the daemon is unavailable, the compositor logs the error and uses its default output layout.

## Failure Behavior

Settings should fail soft for clients and fail explicit for the session supervisor.

The daemon returns `err` responses for malformed requests, invalid values, and unknown monitor IDs. Failed `set` requests do not write the config file.

The settings app remains usable as a UI shell when the daemon is unavailable, but changes cannot be saved. It reports backend errors in its status label.

The compositor can start without settingsd and falls back to default output layout. In a normal supervised session this should be rare because `hearthspace-session` waits for settingsd readiness before starting the compositor.

## Adding Settings

New settings should pass through the same ownership boundary:

- Add the field to `hearthspace_ipc::Settings` and its default.
- Add a `SettingsRequest` variant and client helper if the setting is mutable at runtime.
- Extend request parsing and response formatting in `hearthspace-ipc`.
- Extend response parsing while preserving compatibility with older responses when needed.
- Extend `hearthspace-settingsd` parsing and formatting for `config.toml`.
- Handle get/set logic in `hearthspace-settingsd` so writes stay atomic and centralized.
- Update `hearthspace-settings` to call the IPC helper instead of writing the file.
- Update compositor or shell consumers to read through `hearthspace-ipc`.
- Add tests in the IPC crate and settings daemon for defaults, parsing, saving, and errors.

Do not make UI clients responsible for file migrations or partial writes. If the persisted format changes, `hearthspace-settingsd` should own compatibility because every settings consumer already talks through it.
