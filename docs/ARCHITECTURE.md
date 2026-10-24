# Architecture and ownership

Hearthspace keeps the compositor responsible for input, placement, and presentation.
The shell and Settings remain ordinary Wayland clients. They describe user intent
through typed requests; the compositor decides whether that intent is currently
allowed and which output or window owns it.

## Shared packages

| Package | Owns | Does not own |
| --- | --- | --- |
| `hearthspace-common` | Pure geometry and domain policies, stable names and paths | Wayland connections or UI state |
| `hearthspace-ipc` | Control and settings wire models, validation, bounded transports | Labels, views, or compositor execution |
| `hearthspace-desktop` | App catalog, favorites persistence, artwork resolution, app/file activation services | Shell panels and desktop composition |
| `hearthspace-wayland-client` | Cancellable output observation and coherent `wl_output.done` snapshots | Primary-output policy or Settings drafts |
| `ui-components` | Reusable Xilem widgets, theme properties, gesture recording | Launch authority or application state |

The combined compositor executable retains its deliberate `--shell` entry point.
That packaging dependency does not make the shell the service library for Settings
or compositor launching. `GesturePad` receives samples and callbacks; the Settings
recording path cannot launch applications.

## Compositor state

`App` remains Smithay's protocol and seat integration root. `windows::State` owns
window ID issuance, storage, and stack mutation. Feature state retains stable IDs;
indices are local to synchronous traversal. Raising a transient family or changing
a window's classification cannot change its identity.

`windows::lifecycle` distinguishes null-buffer suspension from actual retirement.
Wayland and X11 buffer suspension retain the record. XWM withdrawal and destruction
remove it. Retirement cancels only that owner's geometry operation, preserves the
Smithay grab until held releases drain, and restores focus without panning.

Frame preferences keep canonical app-slot targets. Live bindings resolve those
slots to managed windows for geometry, input, status, and deliberate canvas drops.
An absent application has no visible frame geometry, but its preference survives.
Confirmed pinned-window close deliberately removes the preference. See [window management](WINDOW_MANAGEMENT.md).

`input::policy` evaluates action, source, and admission phase. New navigation and a
client's authenticated continuation have different requirements. It does not replace
Smithay's serial validation, feature recognizers, or release bookkeeping. Named
search, switcher, and seat-reset transitions preserve their distinct focus and
cancellation rules. Never sample pointer-grab state from a pointer-grab callback.

Presentation returns one immutable projection with surface origin, visible bounds,
scale, and opacity. Canvas preview reads use the sampled animation time; rendering
and inverse input transforms use that same projection. Canonical canvas homes remain
independent of temporary preview, frame, and output presentation. See [rendering](RENDERING.md).

## Settings and outputs

settingsd owns transactions and committed snapshots. A persistent advisory lock
serializes shared-profile writers through read, validation, and atomic replacement.
A committed snapshot carries a revision and content fingerprint; cursors compare by
equality, including after valid external edits. Consumers reconnect by fetching the
latest snapshot and apply only their owned projections. Local Settings previews
remain separate from observed and persisted state. See [settings](SETTINGS.md).

Wayland observations describe connected outputs, modes, and logical geometry.
Saved monitor preferences also retain disconnected connectors. Shared pure identity
resolution gives explicit connector identities precedence over legacy roles; the
compositor supplies the authoritative primary connector because `wl_output` has no
primary flag. Shell per-output identities remain stable across metadata updates.

## Session and runtime artifacts

Normal shutdown and crash recovery share the owned app-scope stop barrier before
removing the runtime. Independent watchdogs and generation-aware activation cleanup
remain separate owners. Borrowed user services are never adopted for termination.
See [login sessions](LOGIN_SESSION.md) and [applications](APPS.md).

`scripts/runtime-artifacts.json` is the concrete package/binary/native-feature
inventory used by installation, the native harness, and development bootstrap.
Development resolves Cargo's reported executables and refreshes required helpers
before startup. Installed sessions use one immutable release without a checkout.
See [development setup](../CONTRIBUTING.md).
