# Application tray

Hearthspace's bar displays application tray icons using the freedesktop
StatusNotifierItem protocol on the desktop session's private D-Bus connection.
Applications can keep background status and controls available without leaving
their main window open. The shell hosts `org.kde.StatusNotifierWatcher` and
renders icons and exported `com.canonical.dbusmenu` menus as ordinary accessible
Wayland shell surfaces.

## Using tray apps

- Click an icon to activate its application. Menu-only indicators open their menu.
- Right-click to open an application's exported menu. Applications without an
  exported menu receive the protocol's context-menu request.
- Middle-click and scroll are delivered to applications that support those actions.
- The **Application tray** button offers menu access with a touchscreen or keyboard.
- Exported menus support nested items, checkmarks, radio choices, separators,
  disabled actions, live updates, and keyboard focus. Escape and clicking outside
  dismiss the popup.

Passive indicators are hidden. Active and attention-requesting indicators remain
visible. App exit removes its icon and closes an open menu belonging to it.
Restarting an app registers a fresh item without leaving a duplicate behind.
The bar keeps space for desktop controls and moves excess icons into the
Application tray list. Named PNG/SVG icons, private app icon themes, and supplied
pixel images are supported. Standard status names fall back to their symbolic
theme variants when needed. Attention icons are supported; separate overlay
icons and animated icon movies are not currently composed.

The supported interface is StatusNotifierItem (also used by AppIndicator and
Ayatana libraries). The legacy X11 system-tray window-embedding protocol is not
implemented. A tray host cannot create an icon for an app that exports neither
interface. Icons and menus remain inside the owning desktop session; the shell
does not attach to an unrelated GNOME or SSH session bus.

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
menus, checkbox state, live menu replacement, Escape/outside dismissal, the
touch-friendly menu launcher, pixmap updates and same-filename SVG replacement,
passive/attention transitions, alias release and duplicate registration,
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
tray list, open an app menu, and activate an action. It verifies the app received
that action, records real libinput touch frames, and removes the virtual device.

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
