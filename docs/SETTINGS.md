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

The daemon creates the socket parent directory, checks whether an existing listener is already running, removes stale socket files, and accepts one-line requests on a Unix domain socket. A second daemon instance exits cleanly when a live listener exists, including when its connection backlog is full.

Connections are nonblocking and bounded: unfinished clients do not hold up other requests. The daemon accepts at most 32 simultaneous clients and expires incomplete exchanges after 250 ms. Requests must end with a newline and fit within 16 KiB; responses fit within 64 KiB. Settings updates remain serialized so concurrent changes cannot overwrite one another's fields.

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
background_mode = repeat-with-canvas
background_image_path = default
color_mode = light
accent_color = orange

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

Unknown top-level keys and unknown monitor keys are ignored. Invalid booleans, invalid appearance choices, invalid background modes, invalid integers, invalid positive integers, and malformed key/value lines make parsing fail for that request. Monitor entries without an `id` are dropped. If no valid monitors remain, the daemon restores the default monitor list.

## Settings Model

The shared settings model lives in `hearthspace-ipc`:

```text
Settings
- test: bool
- appearance: Appearance (color_mode: Light | Dark, accent_color: Orange | Rose | Violet | Blue | Teal | Olive)
- background_mode: BackgroundMode
- background_image_path: Option<String>
- monitors: Vec<MonitorConfig>

BackgroundMode
- repeat-with-canvas
- stretch
- contains
- cover

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
background_mode = repeat-with-canvas
background_image_path = default
color_mode = light
accent_color = orange
primary: 1920x1080 scale 1 at 0,0
secondary: 1920x1080 scale 1 at 1920,0
```

Monitor `x` and `y` are logical desktop positions. `scale` must be positive. Monitor IDs cannot be empty or contain whitespace when sent through the IPC client helpers.

Monitor layout and background settings are active compositor consumers today. `background_mode` and `background_image_path` are persisted, returned through IPC, exposed by the settings app, and reloaded by the compositor when settings change. Legacy config files with `background_moves_with_canvas = true` load as `repeat-with-canvas`; `false` loads as `cover`.

## IPC Protocol

The wire protocol is intentionally small and line-oriented. Clients send one request line, shut down the write side of the socket, and read through the response's terminating newline. Connecting, writing, and reading share one absolute deadline, so trickled data or a full socket backlog cannot extend the exchange indefinitely. Settings reads have a 50 ms budget to limit disruption to compositor refresh; writes and the settings app's compositor reload request have a 500 ms budget. A timeout reports failure and leaves the consumer's previous settings available.

Supported requests are:

```text
get
set test <true|false>
set color-mode <light|dark>
set accent-color <orange|rose|violet|blue|teal|olive>
set background-mode <repeat-with-canvas|stretch|contains|cover>
set background-image-path <default|percent-encoded-path>
set monitor <id> <x> <y>
set monitor-scale <id> <scale>
```

Successful responses start with `ok` and include a complete settings snapshot:

```text
ok test false background-mode repeat-with-canvas background-image-path default monitor primary 1920 1080 1 0 0 Primary monitor secondary 1920 1080 1 1920 0 Secondary color-mode light accent-color orange
```

Error responses start with `err`:

```text
err unknown monitor id: missing
```

Tokens that may contain spaces or punctuation are percent-encoded in IPC responses. This applies to monitor IDs and names. The config file itself stores the daemon's formatted values, not the IPC encoding.

All normal clients should use the `hearthspace-ipc` helper functions instead of constructing protocol lines directly:

```text
client_get_settings()
client_get_settings_at(path)
client_set_color_mode(mode)
client_set_accent_color(accent)
client_set_test(test)
client_set_background_mode(mode)
client_set_background_image_path(path)
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

The app never writes `config.toml` directly. User actions update local UI state optimistically, then call the daemon through `hearthspace-ipc`:

- Appearance mode and accent choices call `client_set_color_mode()` and `client_set_accent_color()`.
- The background fit dropdown calls `client_set_background_mode()`.
- The background image controls call `client_set_background_image_path()`.
- Dragging a monitor commits positions through `client_set_monitor_position()` for each monitor.
- Changing monitor scale calls `client_set_monitor_scale()` and then saves recalculated monitor positions.

On success, the app merges the returned settings with live output data. Appearance saves update only the appearance fields, preserving pending edits elsewhere in Settings. On failure, it restores the previous local state where needed and shows the error in the status label.

The default appearance remains light/orange. Old files and older IPC responses
without appearance fields receive that default. The shell and open Settings
windows refresh appearance on a background task every 500 ms, retaining the
last palette on errors. A generation check prevents an in-flight read from
undoing a newer local save. Saving in Settings also requests compositor reload.

## Linux Appearance Publication

Native sessions publish their saved light/dark preference and accent through the
[Settings portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html).
`org.freedesktop.appearance::color-scheme` is `1` for dark and `2` for light.
`accent-color` is an `(ddd)` tuple of normalized sRGB components. Applications
choose how to interpret these preferences; a toolkit may map the accent to its
own palette.

The session supervisor owns `org.freedesktop.impl.portal.desktop.hearthspace` on
the selected session bus. A dedicated worker reads the explicit settings socket every
250 ms with the usual 50 ms IPC budget, updates the published snapshot, and emits
`SettingChanged` only for changed keys. Unavailable settingsd reads retain the
last snapshot. The worker starts before the public portal, releases its bus name
at shutdown, and has two restart attempts after failure. A restarted worker
reannounces its owned preferences once so existing clients recover missed
changes. Nested sessions and `--no-desktop-services` do not run the appearance
portal; XSettings still publishes to the compositor's own XWayland display.

The runtime portal configuration selects Hearthspace for Settings and GTK for
other portal interfaces. Hearthspace forwards unrelated GTK settings and their
change signals, with a 500 ms call bound. It filters GTK's competing appearance
keys: the installed portal 1.21.1
[forwards every backend's signals](https://github.com/flatpak/xdg-desktop-portal/blob/1.21.1/src/settings.c#L220)
even when a higher-priority backend supplied the value returned by `ReadAll`.
Filtering in the owned backend keeps application caches consistent with reads.

GTK 3 Wayland clients receive `GTK_USE_PORTAL=1` and compatible
`org.gnome.desktop.interface` aliases for `gtk-theme` and `color-scheme`.
The GTK backend itself opts out to avoid recursive settings lookup.
XWayland uses Smithay's existing XSettings owner to publish `Net/ThemeName` as
`Adwaita` or `Adwaita-dark`. A [small Smithay patch](../patches/README.md#smithay-xsettings-publication)
flushes property updates immediately, including when the X11 client is idle.
GTK's
[XSettings mapping](https://github.com/GNOME/gtk/blob/gtk-3-24/gdk/x11/gdksettings.c)
has no equivalent interoperable accent setting; portal-aware applications can
still consume the standard accent preference.

This writes no host GSettings/dconf values or user theme files. Normal native
startup separately publishes the shared activation environment and refreshes
display-dependent portals; explicit private sessions leave that shared
environment unchanged. Application theme overrides and application-specific
support continue to determine an application's final appearance.

The portal integration test exercises the installed public portal on a private
bus, including live changes and unrelated GTK settings. The compositor's
`xsettings_publishes_saved_startup_theme_and_idle_live_changes` regression checks
saved startup preferences and live X11 property notifications without unrelated
X11 requests. These complement real GTK 3 and libadwaita application checks.

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
