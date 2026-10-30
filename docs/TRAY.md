# Application tray

Hearthspace's bar displays application tray icons using the freedesktop
StatusNotifierItem protocol on the selected session D-Bus connection.
Applications can keep background status and controls available without leaving
their main window open. The shell hosts `org.kde.StatusNotifierWatcher` and
renders icons and exported `com.canonical.dbusmenu` menus as ordinary accessible
Wayland shell surfaces.

## Using tray apps

- Click an icon to activate its application. Menu-only indicators open their menu.
- Right-click to open an application's exported menu. Applications without an
  exported menu receive the protocol's context-menu request.
- Middle-click is delivered to applications that support it. Scrolling moves an
  overflowing strip; when all icons fit, it reaches the application instead.
- Tap an app icon on a touchscreen to activate it; hold the icon to open its
  menu. With keyboard focus on the icon, press Menu or Shift+F10 to open it.
- Exported menus support nested items, checkmarks, radio choices, separators,
  disabled actions, live updates, and keyboard focus. Escape and clicking outside
  dismiss the popup.

Passive indicators are hidden. Active and attention-requesting indicators remain
visible. App exit removes its icon and closes an open menu belonging to it.
Restarting an app registers a fresh item without leaving a duplicate behind.
The bar shows the app icons directly, without an extra tray button or list.
By default, application tray icons remain in the top system group even when the same app is
running or pinned in the bottom dock. Each tray icon retains its original artwork,
status, and menu; the dock icon launches the app or focuses its windows.

All active icons remain in the strip. Detached strips stay within the output and
support wheel scrolling, touch dragging, and keyboard focus reveal; grouped strips
use the frame group's scrolling. Named PNG/SVG icons, private app icon themes,
and supplied pixel images are supported. Standard status names fall back to their symbolic
theme variants when needed. Attention icons are supported; separate overlay
icons and animated icon movies are not currently composed.

The supported interface is StatusNotifierItem (also used by AppIndicator and
Ayatana libraries). The legacy X11 system-tray window-embedding protocol is not
implemented. Apps must export StatusNotifierItem to appear in Hearthspace's tray.
Installed native sessions use the standard user bus. Native development and the
private-bus harness isolate their icons and menus from another running desktop.
A shell that finds an existing StatusNotifierWatcher registers with it without
stealing its name.

The pinned AccessKit AT-SPI adapter incorrectly advertises disabled menu items
as `Enabled`/`Sensitive`. Hearthspace sets the underlying disabled state, excludes
these rows from keyboard focus, and rejects pointer and semantic activation.
[AccessKit PR 788](https://github.com/AccessKit/accesskit/pull/788) addresses the adapter;
adopting it requires a compatible Xilem and AccessKit upgrade from the pinned
0.18.1 adapter. Acceptance records this
limitation explicitly and verifies disabled actions never reach the app, with
an enabled action immediately afterwards proving that the transport works.

## Deterministic acceptance

Build the real shell, compositor, agent helpers, and independent tray peer:

```sh
cargo build --locked --workspace --all-targets --features udev,e2e,test-apps
HEARTHSPACE_E2E=1 HEARTHSPACE_TRAY_EVIDENCE="$PWD/artifacts/tray-headless-1" \
  python3 -m unittest discover -s scripts/tests -p test_tray_headless.py -v
```

Use a fresh evidence directory for each run. The headless test starts a private
D-Bus daemon, compositor, notification service, and shell. The example
`crates/hearthspace-shell/examples/tray_fixture.rs` implements the application
side independently of the tray backend. Its explicit private control socket
changes app state, while a JSON-lines log records actual D-Bus calls received.

The workflow checks red icon pixels, delivery of primary/middle/scroll actions,
exported-menu accessibility and activation, disabled-item suppression, nested
menus, checkbox state, live menu replacement, Escape/outside dismissal, direct
keyboard access from each app icon, pixmap updates and same-filename SVG
replacement, passive/attention transitions, alias release and duplicate registration,
menu-only activation, context-menu fallback, and process exit/restart. It checks
both semantic state and screenshots rather
than treating an accepted input request as evidence of app behavior.

When the host provides GTK 3 and `libayatana-appindicator3`, the workflow also
starts `scripts/tests/fixtures/ayatana_tray.py`, which calls the deployed
libraries directly. It checks a named-icon indicator and delivery from the
exported menu back to GTK, then removes the owned app. The named-icon assertion
rejects the generic fallback glyph and retains the exported SNI properties.
Missing optional libraries are recorded explicitly; they do not silently turn
into a compatibility pass. Set `HEARTHSPACE_REQUIRE_TRAY_LIBRARY=1` to require
this deployed-library check: missing GTK 3 or `libayatana-appindicator3` then
fails acceptance with the missing library names. CI sets this flag and installs
both libraries. Local and native runs keep the optional behavior unless the
flag is set.

The same workflow runs on an owned native VT:

```sh
scripts/tray-smoke --session "$PWD/artifacts/tray-native-1"
```

Read [native control](../.agents/skills/hearthspace-native-control/SKILL.md) and
[native testing](NATIVE_TESTING.md) first. Run native acceptance serially, after
the relevant build and without concurrent GPU-heavy suites. The native harness
records the original VT, owns its compositor session, retains page-flip capture
metadata, and restores the original VT on success or failure. Check the retained
manifest's cleanup fields and stopped unit, not only the command's exit status.
Native acceptance also uses a temporary owned uinput touchscreen to tap the
app icon, hold it to open its menu, and activate an action. It verifies the app
received that action, records real libinput touch frames, and removes the virtual device.

Use `--overflow --timeout 240 --total-timeout 900` to include many-icon standalone
trays on the top and left edges, or `--overflow-only` to run just those checks.
They verify wheel scrolling, Tab and Shift-Tab focus reveal, and touchscreen
dragging without app activation or canvas movement.

Artifacts include `tray-actions-*.jsonl`, fixture logs, icon/menu PNG and metadata
pairs, accessibility snapshots, workflow `results.json`, and the session manifest.
The fixture process is terminated in a `finally` path, including failed assertions;
the outer harness also tears down the owned desktop session.

<a id="repeated-popup-gpu-hang--2026-10-22"></a>

## Finite menu geometry

Tray menus depend on the [Masonry scrollbar geometry patch](../patches/README.md#masonry-finite-scrollbar-geometry).
Focus reveal at an exact fit must leave finite coordinates when a later resize
exposes a scrollbar. Invalid geometry can reach the renderer during repeated
popup creation. Keep unregister/register, repeated opening, and overflow checks
when updating the UI dependencies; a single successful menu opening does not
exercise that lifecycle.
