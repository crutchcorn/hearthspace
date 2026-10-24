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
Application tray icons remain in the top system group even when the same app is
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
implemented. A tray host cannot create an icon for an app that exports neither
interface. Normal native sessions use the standard user bus. The explicit private-bus
harness isolates its icons and menus from another running desktop. A shell that
finds an existing StatusNotifierWatcher registers with it without stealing its name.

The pinned AccessKit AT-SPI adapter incorrectly advertises disabled menu items
as `Enabled`/`Sensitive`. Hearthspace sets the underlying disabled state, excludes
these rows from keyboard focus, and rejects pointer and semantic activation.
[AccessKit PR 788](https://github.com/AccessKit/accesskit/pull/788) fixes the adapter
in `accesskit_atspi_common` 0.21.0; adopting it requires upgrading the pinned Xilem
and AccessKit family from the current 0.18.1 adapter. Acceptance records this
limitation explicitly and verifies disabled actions never reach the app, with
an enabled action immediately afterwards proving that the transport works.

## Deterministic acceptance

Build the real shell, compositor, agent helpers, and independent tray peer:

```sh
cargo build --locked --workspace --all-targets --features udev,e2e,test-apps
HEARTHSPACE_E2E=1 HEARTHSPACE_TRAY_EVIDENCE=/tmp/hearthspace-tray-headless \
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
scripts/tray-smoke --session /tmp/hearthspace-tray-native
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

## Validation record — 2026-10-15

- `cargo fmt --all -- --check`, workspace/all-target Clippy with `-D warnings`,
  and workspace/all-target builds passed with `udev,e2e,test-apps` enabled.
- All **615 Rust tests** passed with `HEARTHSPACE_E2E=1`,
  `HEARTHSPACE_REQUIRE_ATSPI=1`, and `--test-threads=1`.
- All **322 Python tests** passed with `HEARTHSPACE_E2E=1`; Ruff check/format and
  `git diff --check` passed. Final headless tray evidence is under
  `/tmp/hearthspace-tray-headless-final`, with the other shell workflows under
  `/tmp/hearthspace-tray-shell-final`.
- `scripts/tray-smoke --session /tmp/hearthspace-tray-native-1` passed on the
  physical GPD laptop: kernel `7.0.0-38-generic`, AMD Radeon 880M/890M with
  `amdgpu`, eDP-1 at 2560×1600 and scale 2. All 17 retained native captures
  completed page flips. Mouse, keyboard, private AT-SPI actions, and owned
  uinput touchscreen taps reached the independent app. The installed
  Ayatana 0.5.94/GTK 3.24.52 libraries exported a real named icon and working menu.
- The harness used VT7 and restored the VT recorded at launch, **TTY3** (an
  earlier preflight observed TTY2). Cleanup reports complete, the runtime and
  temporary touchscreen are gone, and the owned unit has `MainPID=0` and an
  empty control group. The unit retains an `exit-code` failure record because
  shutdown tried to stop an already absent app-scope guard; the compositor
  exited normally and all cleanup checks passed. Its logs remain available.

Earlier headless attempts are retained separately: attempt 1 exposed the fixed
DBusMenu array signature; attempt 2 was interrupted for that fix; attempt 3
exposed the documented upstream disabled-state flag; attempts 4/5 passed before
strengthening icon assertions; attempt 6 verified the corrected Ayatana glyph
but was interrupted for GPU test coordination. Only the complete final suite
and native run above are the final acceptance record.

The optimized build was installed with `scripts/install-session install` as
release `8c62c1b1dc73`. Installed binary hashes were compared against the release
build and the installation manifest was verified. The user's existing desktop
session was not restarted. No additional OS packages were needed.

## Repeated popup GPU hang — 2026-10-22

On the physical ThinkPad T480s, repeated tray-menu creation hung the Intel UHD
620 (`8086:5917`, i915, kernel `7.0.0-38-generic`). A first opening succeeded;
another opening after unregister/register could hang the shell GPU context.
Checked Vello shaders did not prevent it. CPU preprocessing instead exposed a
NaN assertion, and an instrumented diagnostic captured non-finite coordinates
already in the input path. Its evidence is retained in
`/tmp/hearthspace-gpu-capture.vfc1gr61/flatten-nan.txt`.

The [Masonry Portal repair](../patches/README.md#masonry-finite-scrollbar-geometry)
keeps scrollbar progress finite when focus reveal occurs at an exact fit and a
later resize exposes the hidden bar. A renderer-free regression fails on the
original dependency and passes with this repair. Production Vello and shader
settings remain unchanged.

The targeted native comparison passed first opening and unregister/register
followed by another opening in the same shell process, with normal GPU
rendering. Evidence is in `/tmp/hearthspace-gpu-portal.mwspkdj0`, with exact
binary provenance in `target/gpu-portal/provenance.json`. An initial harness
attempt lacked its copied tray fixture and stopped before interaction; the
retained `single-ready` and `reregister` attempts both passed. No GPU loss or
panic appeared in the session or kernel logs. Output eDP-1 was 1920×1080 at
scale 1. Cleanup restored tty2, stopped the session and app scopes, removed the
runtime, and left the owned unit inactive with `MainPID=0`.

The later `/tmp/hearthspace-tray-portal.c687y_3p` run passed the complete ordinary
tray workflow, including real touchscreen actions and the required installed
Ayatana indicator library. Its additional standalone-overflow phase stopped
during setup with `tray frame widget is not mapped`, before overflow input was
tested. Evidence and complete cleanup are retained; this run does not count as
overflow acceptance.

Standalone overflow subsequently passed in
`/tmp/hearthspace-tray-overflow-verified.f9Q5om`: 59 registered icons on both the
top and left edges, wheel navigation to both endpoints, every Tab/Shift+Tab
focus-and-reveal step, reverse entry at the trailing icon, and physical touch
dragging. Pixel checks verified the revealed icons; no application callbacks,
menus, or canvas movement leaked from scrolling. Session/kernel logs contain no
GPU loss or panic. Cleanup restored tty2, removed the runtime and temporary
touchscreen devices, and left the owned service inactive with `MainPID=0`.

The intervening `/tmp/hearthspace-tray-overflow-final.UpeyFQ` attempt exhausted
its initial-readiness deadline because it required every fixture in one AT-SPI
snapshot. AccessKit intentionally omits offscreen Portal children. Readiness now
accepts the observed clipped subset while rejecting duplicate/unknown names and
requiring actual clipping. Registration still accounts for all 59 fixtures, and
the complete forward/reverse traversal proves each is reachable. Both failed
attempts retain their evidence and completed cleanup.

The final software-rendered headless suite reached the Ayatana menu snapshot
with only 0.046 seconds left in its shared 120-second budget. The complete
unchanged workflow then passed in 123.83 seconds under a diagnostic budget;
workflow-only time was 120.95 seconds. It included 240 AT-SPI subprocesses,
26 captures, and nine PNG decodes. Those categories overlap and are not additive.
Evidence is retained in `/tmp/hearthspace-icon-final-e2e.rpc4my38/tray` and
`/tmp/hearthspace-tray-final-timed-180`. The headless test now allows 180 seconds
in total; all delivery, removal, pixel and menu assertions, and individual IPC
deadlines, remain unchanged.
