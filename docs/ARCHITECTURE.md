# Architecture and ownership

Hearthspace's compositor owns input routing, window placement, and presentation.
The shell and Settings are Wayland clients: they draw their interfaces and send
requests, while the compositor decides whether an action is allowed and which
window or output owns it. This guide records those boundaries for contributors.

## Shared packages

| Package | Owns | Does not own |
| --- | --- | --- |
| `hearthspace-common` | Shared geometry and domain policies, stable names and paths | Wayland connections or UI state |
| `hearthspace-ipc` | Control and settings wire models, validation, bounded transports | Labels, views, or compositor execution |
| `hearthspace-desktop` | App catalog, favorites persistence, artwork resolution, app/file activation services | Shell panels and desktop composition |
| `hearthspace-wayland-client` | Cancellable output observation and coherent `wl_output.done` snapshots | Primary-output policy or Settings drafts |
| `hearthspace-ui-components` | Reusable Xilem/Masonry widgets, theme properties, gesture recording | Launch authority or application state |

The managed session launches `hearthspace-compositor` and `hearthspace-shell` as
separate executables. The combined `hearthspace` development executable also
retains a `--shell` entry point. That packaging dependency does not make the shell
the service library for Settings or application launching. Gesture recording
receives samples and callbacks; recording in Settings does not launch an
application. See [UI boundaries](UI.md) for client and compositor rendering
responsibilities.

## Window identity and lifetime

The compositor's `App` is Smithay's protocol and seat integration root. Window
state owns stable ID issuance, storage, and stacking. Features retain those IDs;
indices are local to synchronous traversal. Raising a transient family or changing
a window's classification cannot change its identity.

Buffer suspension and retirement are different events. A null-buffer unmap
suspends presentation and input ownership while retaining the window record.
XWM withdrawal and destruction remove it. Retirement cancels that owner's geometry
operation, preserves held-release bookkeeping, and restores focus without moving
the viewport. Unrelated popup or shell teardown must not cancel another window's
operation.

Each managed window keeps at most one syntax-cache entry for its current optional
raw application ID, recognizing frame widgets, groups, grips, and desktop items.
Every lookup compares the complete current role-derived ID exactly, distinguishing
missing and empty IDs and observing app-ID changes and XDG unmap resets.
X11 identity follows current class, then instance, then remembered executable fallback.
The cache stores parsed syntax only; output resolution, window classification,
authorization, placement, and visibility remain live decisions.

Frame preferences keep canonical application slots. Live bindings resolve them to
managed windows for geometry, input, status, and deliberate canvas drops. An absent
application has no visible frame geometry, but its preference survives. Confirmed
pinned-window close deliberately removes the preference. See
[window management](WINDOW_MANAGEMENT.md).

## Input admission and sequence ownership

Input policy evaluates the action, source, and admission phase. Starting canvas
navigation and continuing a client's authenticated window operation have different
requirements. This policy complements Smithay's serial validation, feature
recognizers, and release bookkeeping; it does not replace them.

Once an interaction owns a sequence, cancellation must consume its remaining
events without redirecting a release into another control. Search, switching,
layout editing, lock, and seat reset have distinct transitions for focus and
cancellation. Never sample pointer-grab state inside a pointer-grab callback:
Smithay already holds the relevant lock.

Touch-capable Wayland clients receive direct touch with a stable surface target.
A valid client move/resize request can transfer that touch sequence to the
compositor without synthesizing a mouse press. Clients without that path,
including XWayland, retain pointer emulation. Device, surface, output, and focus
changes can revoke ownership; late input must not reactivate it. See
[gestures](PINCH_ZOOM.md).

## Presentation and outputs

A window's displayed projection contains its surface origin, visible bounds,
scale, and opacity. Rendering and inverse input transforms share that projection.
Preview animation reads use the sampled time. Canonical canvas homes remain
independent of temporary previews, frame attachment, and output presentation.
See [rendering](RENDERING.md).

Output records own client-visible geometry; the backend owns scanout resources.
The native backend owns scanout resources on each seat GPU, with per-output
surfaces and client pacing. One canonical GPU renders the scene; the backend
transfers completed images to outputs attached to other GPUs. Desktop placement
uses logical dimensions derived from the physical mode and output scale. See
[DRM/KMS](DRM.md) for transfer and recovery boundaries and [size units](SIZE_UNITS.md).

## Settings and observations

`hearthspace-settingsd` owns preference transactions and committed snapshots. A
persistent advisory lock serializes cooperating writers through read, validation,
and atomic replacement. A snapshot carries a revision and content fingerprint;
watch cursors compare by equality, including after valid external edits.
Consumers reconnect by fetching the latest snapshot and apply only the state they
own. Local Settings previews remain separate from observed and persisted state.
See [settings](SETTINGS.md).

Wayland observations describe connected outputs, modes, scale, and placement.
Saved monitor preferences also retain disconnected connectors. Shared identity
resolution gives explicit connector identities precedence over legacy roles. The
compositor supplies the authoritative primary connector because `wl_output` has
no primary flag. Shell per-output identities remain stable across metadata updates.

## Session, lock, and runtime ownership

The session manager coordinates desktop processes and the runtime they use.
Normal shutdown and crash recovery share an owned app-scope stop barrier before
runtime removal. Independent watchdogs and generation-aware activation cleanup
retain their own lifetimes. Borrowed user services are never adopted for
termination. See [login sessions](LOGIN_SESSION.md) and [applications](APPS.md).

Session locking splits authentication from isolation. The GTK lock client owns
password entry and fixed PAM authentication; the compositor owns the
`ext-session-lock-v1` role, input isolation, opaque fallback, and output
presentation barrier. Losing the client never unlocks the desktop. Lock surfaces
are separate from ordinary application windows. See [session locking](SESSION_LOCK.md).

The runtime artifact manifest defines the packages, binaries, and native features
used by installation, the native harness, and development bootstrap. Development
resolves Cargo's reported executables and refreshes required helpers before
startup. Installed sessions use an immutable release without a source checkout.
See [development](DEVELOPMENT.md).
