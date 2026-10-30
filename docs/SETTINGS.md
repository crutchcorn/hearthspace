# Settings

Open **Hearthspace Settings** from Apps List to adjust the desktop:

- **Appearance:** choose Light or Dark and an Orange, Rose, Violet, Blue, Teal,
  or Olive accent. The default is Light with Orange. Application artwork keeps
  its own colors; applications decide how to use the published theme preference.
- **Displays:** arrange connected outputs and choose an integer scale from
  100–400%. Resolution and refresh rate are reported values, not mode selectors.
- **Background:** choose a local photo or restore Canvas dots. Repeat with
  canvas tiles and moves the photo; Stretch fills the output, Contain shows the
  whole image, and Cover fills it while cropping. The latter three stay still
  while the canvas moves. A photo does not change window positions or grid snapping.
- **Gestures:** record [drawn app shortcuts](FAVORITE_APPS.md).
- **Keyboard shortcuts:** look up the fixed canvas-tool bindings.
- **AI:** explicitly enable or remove the local model for
  [gathering similar windows](WINDOW_GROUPS.md#optional-local-model).
- **System:** open [Change layout](SHELL.md#change-layout), allow Canvas tools
  to be hidden, or restore them.

Changes to appearance, background, and display placement save through the session
daemon and update connected consumers. Errors remain visible in Settings; a local
preview is not proof that a change was saved. Disconnected monitor preferences
are retained for their return. Narrow windows expose categories through the
**Settings categories** button.

## Keyboard shortcuts

The **❖** symbol represents **Meta**. Hold it while pressing a tool's key.
Meta is also called Super or the Windows key; on an Apple keyboard it is usually
Command. Releasing Meta leaves the selected tool active. These shortcuts are
read-only for now.

| Shortcut | Action |
| --- | --- |
| ❖ + V | Select desktop items |
| ❖ + H | Pan canvas |
| ❖ + K | Select Search and open its field |
| ❖ + G | Select window group |
| ❖ + A | Open Change layout |
| ❖ + O | Open or close the canvas map (Overview) |

The shortcuts work while an application has focus. A lock screen, modal dialog,
or an input operation can prevent switching tools; finish or cancel that
interaction first. Bare H, V, and `/`, and Ctrl+K, remain available to apps.
Fullscreen (❖ + F), maximize (❖ + M), and lock (❖ + L) keep their existing keys.
If Hearthspace runs inside another desktop, that desktop may intercept Meta.

## Ownership

`hearthspace-settingsd` owns the appearance, background, and monitor preferences
in `config.toml`. Clients should use the daemon rather than write that file
directly. Favorites and gestures use their own atomic store; frame layout belongs
to the compositor, and model installation belongs to `hearthspace-ai`. Those
features are presented by Settings but are not fields in the daemon's model.

The daemon's boundary is:

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
$HEARTHSPACE_SESSION_DIR/settings.sock
```

The supervisor normally creates that session directory beneath the user's
original runtime directory. Clients should use the explicit socket variable:
their `XDG_RUNTIME_DIR` may differ from the supervisor's. If
`HEARTHSPACE_SETTINGS_SOCKET` is not set, `hearthspace-ipc` falls back to:

```text
$XDG_RUNTIME_DIR/hearthspace-settingsd.sock
```

The daemon creates the socket parent directory, checks whether an existing listener is already running, removes stale socket files, and accepts one-line requests on a Unix domain socket. A second daemon instance exits cleanly when a live listener exists, including when its connection backlog is full.

Connections are nonblocking and bounded: the daemon accepts 32 clients, with at most 16 waiting subscriptions and eight incomplete requests. Incomplete exchanges expire after 250 ms. Requests end with a newline and fit within 16 KiB; responses fit within 64 KiB. Waiting subscribers leave capacity for setters.

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

A missing file is not an error; the daemon returns `Settings::default()` without creating it. Directory notifications wake waiting subscribers after atomic replacement, including changes made by another session daemon or valid hand edits.

A stable private `config.toml.lock` advisory lock spans loading, mutation, validation, revision allocation, and saving, including across daemon processes sharing a profile. Do not remove this lock file while a daemon may use it. Acquisition is bounded; an unavailable lock reports failure without partially applying the request.

Writes are atomic at the file level. The daemon writes the complete formatted settings to a temporary file next to the config file, syncs that temporary file, and renames it over `config.toml`. Failed renames clean up the temporary file when possible.

The file is valid, versioned TOML, parsed and serialized by the `toml` crate.
Strings are quoted and escaped, including paths containing `#`, quotes, Unicode,
or newlines. An unset image path is omitted:

```toml
version = 1
revision = 1
test = false
background_mode = "repeat-with-canvas"
color_mode = "light"
accent_color = "orange"

[[monitor]]
id = "primary"
name = "Primary"
width = 1920
height = 1080
scale = 1
x = 0
y = 0
```

Legacy unquoted settings are still readable. Reads leave the file untouched;
the next successful update validates the entire candidate and migrates it with
an atomic replacement. Malformed data, invalid monitor values, unknown fields,
and unsupported versions return an error rather than overwriting the file with
defaults. Keep the daemon as the compatibility boundary for persistence.

Committed snapshots contain a monotonically allocated revision and a SHA-256
fingerprint of the canonical settings payload. The optional TOML revision defaults
to zero for existing files. The lock file retains the allocation high-water mark;
failed writes may leave revision gaps but publish no change. Valid hand edits can
change the fingerprint without changing the revision. Compare cursors by equality,
not numeric ordering. Subscriptions coalesce to the latest state; they are not an
event history.

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

Default monitor rows provide placeholders before live outputs are resolved:

```text
test = false
background_mode = repeat-with-canvas
background_image_path = default
color_mode = light
accent_color = orange
primary: 1920x1080 scale 1 at 0,0
secondary: 1920x1080 scale 1 at 1920,0
```

Monitor `x` and `y` are logical desktop positions. `scale` must be positive. Legacy per-monitor setters reject empty IDs or whitespace. Complete display-layout transactions accept whitespace within IDs but reject blank IDs and NUL bytes.

Monitor layout and background settings are active compositor consumers today. `background_mode` and `background_image_path` are persisted, returned through IPC, exposed by the settings app, and reloaded by the compositor when settings change. Legacy config files with `background_moves_with_canvas = true` load as `repeat-with-canvas`; `false` loads as `cover`.

## IPC Protocol

The wire protocol is line-oriented. Connecting, writing, and reading share one
absolute deadline. Legacy reads use 50 ms, snapshot reads 500 ms, and writes allow 1 second,
including display commits returning a cursor. A watch may
wait up to 30 seconds plus 500 ms transport slack.
UI and compositor subscriptions run on workers with shorter waits for bounded
cancellation. Errors retain the last committed state; reconnect starts with a fresh
snapshot. Acknowledged commits do not require a writer-issued compositor reload.

Supported requests are:

```text
get
get snapshot
watch settings {"version":1,"after":{"revision":1,"fingerprint":"<sha256>"},"wait_ms":500}
commit set display-layout {"version":1,"monitors":[...]}
set test <true|false>
set color-mode <light|dark>
set accent-color <orange|rose|violet|blue|teal|olive>
set background-mode <repeat-with-canvas|stretch|contains|cover>
set background-image-path <default|percent-encoded-path>
set monitor <id> <x> <y>
set monitor-scale <id> <scale>
set display-layout {"version":1,"monitors":[...]}
```

The display-layout request contains complete `MonitorConfig` rows. Validation
precedes every write, and all submitted scales, positions, dimensions, and new
connector identities commit together. Saved disconnected monitors are retained;
other preference domains are unchanged. Settings sends these transactions through
one background worker above the page switcher. Old completions cannot replace a
newer drag preview or unrelated appearance/background edits. The committed
response includes its cursor. Local observation generations and cursor equality
identify acknowledgments that overlap another observation, including an identical
reconnect read or a hand edit that resets the persisted revision. Ambiguous
acknowledgments retain the local layout until a post-commit read reconciles it;
that read cannot erase a newer local edit. Only submitted connector
drafts are acknowledged; unsaved disconnected drafts survive another monitor's
successful save. Compositor availability does not change save success.

Legacy `get` and `set` responses start with `ok` and include the complete settings:

```text
ok test false background-mode repeat-with-canvas background-image-path default monitor primary 1920 1080 1 0 0 Primary monitor secondary 1920 1080 1 1920 0 Secondary color-mode light accent-color orange
```

Snapshot reads, watches, and `commit set display-layout` return
`ok snapshot {"version":1,"snapshot":...}`. The snapshot contains `revision`,
`fingerprint`, and `settings`; an unchanged watch expiry returns `snapshot: null`.
The daemon validates both supported response encodings before committing, so a
large accumulated configuration cannot commit and then fail its response limit.

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
client_set_display_layout(monitors)
settings_snapshot::client_set_display_layout_snapshot_at(path, monitors)
settings_snapshot::client_get_snapshot()
settings_snapshot::client_wait_for_change(cursor, wait)
```

These helpers resolve the socket path, validate client-side inputs where appropriate, send the request, and parse the response. Ordinary getters/setters return `Settings`. Snapshot reads and cursor-bearing display commits return `SettingsSnapshot`; watches return `Option<SettingsSnapshot>`, with `None` indicating an unchanged expiry.

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
- Starts the shared cancellable Wayland output observer.
- Keeps live output observations, saved monitor preferences, and local drafts separate.
- Watches committed snapshots for changes made by other Settings windows or sessions.

The app never writes `config.toml` directly. User actions update local UI state optimistically, then call the daemon through `hearthspace-ipc`:

- Appearance mode and accent choices call `client_set_color_mode()` and `client_set_accent_color()`.
- The background fit dropdown calls `client_set_background_mode()`.
- The background image controls call `client_set_background_image_path()`.
- Dragging a monitor or changing scale queues one complete display-layout transaction.
- Live hotplug/mode observations update the open Displays page while retaining disconnected preferences and pending edits.

On success, the app merges the returned settings with live output data. Appearance saves update only the appearance fields, preserving pending edits elsewhere in Settings. On failure, it restores the previous local state where needed and shows the error in the status label.

The default appearance remains light/orange. Old files and older IPC responses
without appearance fields receive that default. The shell and open Settings
windows watch committed snapshots on background tasks, retaining the last palette
on errors. A generation check prevents an in-flight observation from undoing a
newer local save. Background-picker completion refetches deferred changes even
when selection was cancelled.

## Linux Appearance Publication

Native sessions publish their saved light/dark preference and accent through the
[Settings portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html).
`org.freedesktop.appearance::color-scheme` is `1` for dark and `2` for light.
`accent-color` is an `(ddd)` tuple of normalized sRGB components. Applications
choose how to interpret these preferences; a toolkit may map the accent to its
own palette.

The session supervisor owns `org.freedesktop.impl.portal.desktop.hearthspace` on
the selected session bus. A dedicated worker watches the explicit settings socket,
updates the published snapshot, and emits
`SettingChanged` only for changed keys. Unavailable settingsd reads retain the
last snapshot. The worker starts before the public portal, releases its bus name
at shutdown, and has two restart attempts after failure. A restarted worker
reannounces its owned preferences once so existing clients recover missed
changes. Nested sessions and `--no-desktop-services` do not run the appearance
portal; XSettings still publishes to the compositor's own XWayland display.

The runtime portal configuration selects Hearthspace for Settings and GTK for
other portal interfaces. Hearthspace forwards unrelated GTK settings and their
change signals, with a 500 ms call bound. It filters GTK's competing appearance
keys: portal versions such as 1.21.1
[forward every backend's signals](https://github.com/flatpak/xdg-desktop-portal/blob/1.21.1/src/settings.c#L220)
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

The shared resolver reserves matching connector IDs/names before applying legacy roles, so a saved row cannot bind two connected outputs. The compositor reports its selected primary connector through `output-primary`; registry enumeration order is not a primary flag. Legacy roles use that primary and sorted secondary connectors. Primary metadata retries independently of hardware changes; temporary errors retain a known connected primary, while a successful empty result clears it. Newly discovered outputs receive connector identities; untouched placeholder rows do not displace saved connector preferences. After resolution, compositor projection uses exact connector IDs rather than applying aliases a second time.

The compositor loads initial settings and subscribes to committed snapshots. Its worker performs transport; Calloop applies the resulting `OutputLayout` without restarting the compositor. When output geometry changes, the compositor reconfigures shell bars, reconciles the pointer, and schedules a redraw.

Layout application follows these rules:

- Match saved monitors by output name or saved monitor name.
- Use role fallback when appropriate.
- Apply saved positive scales before computing placement.
- Place outputs with saved positions first.
- Put any unplaced outputs to the right of the placed group.
- Normalize the resulting layout so the minimum x/y starts at zero.
- Repair overlapping active monitor layouts by falling back to edge-to-edge placement.

If the daemon is unavailable at startup, the compositor uses its default layout. Later failures retain the last committed layout; reconnection applies the current snapshot.

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
