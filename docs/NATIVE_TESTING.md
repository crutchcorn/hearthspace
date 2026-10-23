# Native Testing

Native tests exercise seat ownership, DRM/KMS presentation, and recovery on a
real Linux virtual terminal (VT). Headless tests remain the default for
deterministic application and compositor regressions. A native capture proves
that the captured output completed a page flip; application-specific assertions
must still establish that the UI responded correctly.

The agent harness consists of a session launcher, a control CLI, and an acceptance
test. It runs the desktop as the current user while a transient system service
owns privileged VT setup and independent recovery. The caller can remain in a
background user service, including T3 Code, throughout the test.

## Host Requirements

Follow [CONTRIBUTING.md](../CONTRIBUTING.md#native-agent-testing) for system
dependencies and permissions. Run these checks before starting:

```sh
sudo -n true
cat /sys/class/tty/tty0/active
loginctl list-sessions
```

The harness needs systemd, logind with `pam_systemd` in the login PAM stack, an
available DRM device on `seat0`, and a spare VT among `tty7` through `tty15`.
The caller needs noninteractive sudo. A normal SSH or background user service
does not acquire a seat merely by changing the active VT; the launcher creates
the separate PAM/logind session and controlling TTY.

The agent and remote networking must live outside the desktop session being
tested. User lingering is useful for a persistent background agent. The launcher
does not install or reconfigure the agent, its remote access, or the display
manager.

## Start and Inspect a Session

Use a new artifact directory for every run:

```sh
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 --timeout 45 start --watchdog 300
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 status
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 screenshot /tmp/hearthspace-run-1/desktop.png
```

Start builds the session supervisor, native compositor, shell, settings and
notification daemons, and accessibility CLI before changing VTs.
`start --no-build` explicitly uses existing debug
binaries; use it only after building the required native features. `binaries.json`
records binary paths and hashes. The session manifest records runtime/socket
paths, VT identity, process IDs, and output metadata. Readiness requires
responsive control and settings sockets, a mapped shell surface, and a successful
native capture.

The harness preserves the invoking user's `XDG_DATA_DIRS` across sudo and PAM,
restoring it before the private D-Bus session starts. Desktop entries and icons
exported by Snap or Flatpak are discoverable when their export directories are
in that search path; the harness does not add package-specific directories or
source shell profiles. Unset or empty `XDG_DATA_DIRS` uses
`/usr/local/share:/usr/share`. A supplied value must be a colon-separated list of
nonempty absolute paths without control characters. The manifest records it as
`xdg_data_dirs`. With `--login-session`, the selected release's `share` directory
is prepended so its installed assets take precedence.

For a clean demo or settings test, add `start --isolated-profile`. This opt-in
creates fresh `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `XDG_DATA_HOME`, and
`XDG_STATE_HOME` beneath the artifact directory's `profile/` before starting the
private D-Bus session. Settings, compositor, and application profiles start fresh,
including the default dotted background; the real user identity and `HOME` are
preserved, as is the caller's `XDG_DATA_DIRS` search path for exported applications.
The manifest records `isolated_profile`. Profiles remain with the run's other
artifacts after cleanup; remove the artifact directory when they are no longer
needed. The supervisor's runtime remains empty until startup and is removed on
stop. Omit the flag to keep existing behavior.

The watchdog is an absolute maximum session lifetime in seconds, independent of
agent activity. Choose a lifetime long enough for the test. It deliberately ends
the session even if the caller vanishes. Startup has a separate absolute deadline.

For custom binaries or failure injection, the lower-level launcher accepts
`--binary-dir` with `--no-build` and `--compositor`:

```sh
scripts/hearthspace-native start --session /tmp/hearthspace-run-2 --no-build --watchdog 120
scripts/hearthspace-native status --session /tmp/hearthspace-run-2
scripts/hearthspace-native stop --session /tmp/hearthspace-run-2
```

## Observe and Act

Commands return JSON. Screenshots produce a PNG and adjacent JSON metadata file.
Pointer coordinates default to global output-logical coordinates, independent of
the canvas zoom. Use screenshot metadata to act in captured pixel coordinates.
`status` combines launcher lifecycle information with a live compositor query;
startup metadata is not a substitute for current window or output state.

Use `apps` to discover desktop-entry IDs and `windows` for live titles, stable
window IDs, focus, and geometry. Window IDs belong to the current compositor
process. The `focus`, `close`, `move`, `resize`, `maximize`, `fullscreen`, and
`restore` commands target those IDs; see [window management](./WINDOW_MANAGEMENT.md)
for canvas coordinates, keyboard shortcuts, and restoration behavior.

```sh
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 apps
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 windows
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 focus 3
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 maximize 3
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 restore 3
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 launch foot
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 click 200 180
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 type 'hello from the agent'
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 key enter
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 paste 'Café 日本語 🦀' --shortcut ctrl+shift+v
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 drag 300 100 350 150 --save /tmp/hearthspace-run-1/moved.png
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 scroll -15
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 click 400 360 --units pixels --metadata /tmp/hearthspace-run-1/desktop.json
```

Replace the example window ID with one returned by `windows`. Use
`focus-next`, `focus-previous`, and `focus-launcher` for navigation without an ID.
`key super` toggles Apps List; `focus-launcher` explicitly opens it.
`key ctrl+k` opens [canvas search](./CANVAS_SEARCH.md).
`fullscreen ID --off` and `maximize ID --off` leave one state;
`restore ID` clears both. Launch replies confirm that a process started, so wait
for its mapped window and application-specific readiness before typing.

`type` supports the CLI's US-layout key mappings, including printable ASCII,
newline, and tab. Unsupported characters fail before typing. Key chords use
names such as `ctrl+a`; input cleanup releases held keys and pointer buttons on
errors. Synthetic scrolling follows the same compositor zoom rules as physical
scrolling, including Super and `--scroll-zooms`.

`clipboard-set TEXT` replaces the session clipboard with UTF-8 text;
`paste TEXT` also sends Ctrl+V, or the requested `--shortcut` (terminals commonly
use `ctrl+shift+v`). Both accept `--file PATH` instead of text and validate the
entire payload before acting: at most 16 KiB of UTF-8 with no NUL bytes. Use these
commands for Unicode text and verify the destination content afterward.

Input operations wait for a fresh capture by default. `--wait change` waits for
different image pixels; `--wait none` reports command acceptance only. A cursor
move or animation can change pixels, so use a targeted assertion, window status,
or accessibility result to verify the intended action. For explicit waits:

```sh
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 --timeout 10 wait
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 --timeout 10 wait --change-from /tmp/hearthspace-run-1/desktop.png
```

`lock` instead waits for `locked` and `lock_confirmed`: protected frames must be
presented before it reports success. `lock --wait none` confirms acceptance only.
While locked, status is redacted and capture, input, clipboard, app/window queries,
and accessibility are unavailable. The user authenticates through the real input
devices; recovery for an abandoned owned session is `stop`. See
[session locking](SESSION_LOCK.md).

Select a monitor with `--output NAME`, taking names from `status`. Capture JSON
includes output dimensions, scale, logical origin/bounds, transform, frame ID,
cursor policy, and presentation type. Images are upright; convert pixels with
`logical_x + pixel_x / scale` and `logical_y + pixel_y / scale`. Rounded logical
bounds do not change that scale. Frame IDs are per output and per compositor
process; never compare IDs across sessions.

Native images contain the software/client cursor already in the framebuffer, or
an overlay sampled from the actual hardware cursor image and position during
rendering. `cursor_source` distinguishes those paths. Hardware cursor movement is
independent of KMS page flips, so `cursor_sample: render` identifies that sampling
boundary. Captures are limited to 16 megapixels and four pending requests, with a
five-second completion deadline. Native GPU readback runs on the compositor
thread; PNG compression runs on one CPU worker with at most two jobs, including
completed images awaiting presentation. A reply requires both compression and
the captured frame's page flip. Input and other control clients remain responsive
during compression. Paused sessions, removed outputs, locks, and failed
presentation discard outstanding captures and return errors.

## Stop and Recover

```sh
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 stop
```

Stop is idempotent. Recovery restores the recorded original VT, terminates the
owned service/session descendants, and removes its runtime directory. Screenshot
artifacts, manifests, and logs remain available. The independent systemd watchdog
uses the same recovery path after a hung compositor or interrupted agent.
Privileged recovery uses the root-owned state under `/run/hearthspace-native/`.

Before systemd terminates the private bus wrapper, its stop hook gives the
session supervisor up to six seconds to exit normally. It verifies the unit's
current main process and its direct supervisor child by executable, UID, and
private runtime argument, then signals through a pidfd to avoid PID-reuse races.
This lets the compositor save the isolated run's `layout.json`. A missing,
unresponsive, or mismatched supervisor falls through to the existing bounded
forced cleanup; saving layout is only guaranteed after a successful normal exit.
The hook also applies when the watchdog stops a running unit.

Inspect `session.log`, `session.json`, and the unit named in that manifest when
startup or cleanup fails. `journalctl -u UNIT` includes systemd/PAM errors that
precede compositor logging. Do not start another run while cleanup is incomplete;
resolve it with the recorded session's `stop` command first.

## Acceptance Tests

Build before invoking the smoke suite, which deliberately uses `--no-build`:

```sh
cargo build --locked --workspace --all-targets --features udev,e2e,test-apps
scripts/native-smoke --session /tmp/hearthspace-smoke-success --case success --timeout 90
scripts/native-smoke --session /tmp/hearthspace-smoke-startup --case startup-failure
scripts/native-smoke --session /tmp/hearthspace-smoke-crash --case compositor-failure
scripts/native-smoke --session /tmp/hearthspace-smoke-watchdog --case watchdog
scripts/native-smoke --session /tmp/hearthspace-smoke-interrupted --case agent-interruption
```

The success case launches `foot`, types a visible marker, tracks its pixel
position after moving the window, and verifies that resizing reveals the
previously captured background. It checks movement of the terminal's software
cursor and the desktop cursor in targeted image regions, and captures available
outputs. Recovery cases inject failed startup,
compositor death, a frozen compositor (`SIGSTOP`) recovered by the watchdog, and
an owning agent killed with `SIGKILL` after readiness. Each run preserves commands, results,
screenshots, and metadata under its artifact directory and checks VT/runtime
restoration. Record GPU, driver, kernel, display scale, and whether the run used
a VM or real hardware when reporting results.

The startup-failure case requires a fresh artifact directory and evidence that
the injected failing compositor actually ran in its allocated PAM/VT session.
A preflight failure, such as missing sudo access, fails the test. Run native
validation separately from GPU-intensive headless suites so concurrent workload
does not obscure capture and presentation failures.

The [gesture acceptance tests](../CONTRIBUTING.md#optional-native-pinch-testing)
use temporary uinput touchpad and touchscreen devices to exercise libinput input
and verify rendered three-finger canvas panning, zoom and combined movement.
Run `scripts/pan-smoke` and `scripts/pinch-smoke` separately from the smoke cases
above.

`scripts/window-switcher-smoke --session /tmp/hearthspace-window-switcher-1`
checks held Alt+Tab/Alt+Shift+Tab and left/right swipes with four touchpad contacts
and four/five touchscreen contacts. It observes intermediate preview geometry
during the grid animation, verifies unchanged focus and viewport while held,
checks each client's colored live pixels, and captures the revealed window after
release. Input ownership,
Escape rollback, stale gesture events, and a touchscreen contact held across Alt
release are checked too. A temporary keyboard is unplugged with Alt held; the
test checks overlay cleanup and types a file-writing command to prove modifier
recovery. Independent libinput traces establish the delivered
gesture type, direction, and contact count. Run it separately from headless GPU
tests; it owns and cleans up its native session and temporary input devices.
Pinch cases use four touchpad contacts and four/five touchscreen contacts. They
check that successful inward completion leaves the switcher open after release,
outward completion dismisses it, and subthreshold inward gestures leave it closed.
Four-contact cases also test cancellation; subthreshold outward pinches preserve
the open switcher. They check that pinches do not change a keyboard-owned session and
repeating an inward pinch does not cycle the selection. Focused-view cases open
with inward pinch, dismiss outward without leaving focused view, then reopen and
commit with Enter, leaving the previous app bounded on the canvas.
Preview clicks/taps and Escape/background interaction need the additional acceptance
steps below. These cases do not establish physical finger feel on the hardware.

Unlocked `status.window_switcher` describes selection separately from presentation:

| Field | Meaning |
| --- | --- |
| `active` | Whether a switcher selection session is open. |
| `source` | `"keyboard"` for Alt+Tab, `"gesture"` for swipe or pinch, or `null` when inactive; it does not identify the device type or distinguish held and persistent sessions. |
| `selected_window_id` | Candidate window ID, or `null` when inactive; previewing does not focus it. |
| `window_ids` | Ordered IDs participating in the current session. |
| `output` | Output name, or `null` when inactive. |
| `items` | Selectable previews with `window_id`, `title`, current `x`/`y`/`width`/`height`, equivalent `preview_*` bounds, `scale`, and `canonical`/`target` rectangles. |
| `scene` | Animated poses with `window_id`, `canonical` and current `display` rectangles, `opacity`, and `matches`; includes non-result windows. |
| `animating` | Whether preview motion is still running, including the exit transition after `active` becomes `false`. |
| `version`, `revision`, `session` | Schema version, state revision, and interaction ID; `session` is `0` when inactive. |
| `title`, `index`, `count`, `caption` | Selected title, one-based position, window count, and nullable title-badge bounds with `max_width`. |

Rectangles use global output-logical coordinates. `canonical` is the normal
screen-space presentation, not the stored canvas position; `target` is the
destination preview rectangle. All selectable windows fit into the centered
grid. Wait for `animating: false`, then capture a fresh presented frame before
asserting preview colors. Settled previews do not close the switcher: release
Alt or the swipe to commit a held session, or explicitly select or dismiss a
persistent session opened by a pinch. The full status omits `window_switcher`
while locked. The separate shell-facing `window-switcher-status` command omits
`scene`, `items`, and `window_ids`; use full `status` for preview inspection.

Window-switcher acceptance on the physical GPD laptop, AMD Radeon 890M (`amdgpu`),
kernel `7.0.0-38-generic`, eDP-1 at 2560×1600 and scale 2, passed in
`/tmp/hearthspace-window-switcher-native-2`: held Alt+Tab and reverse selection,
four-contact touchpad swipes, four/five-contact touchscreen swipes in both
directions, intermediate animated viewport positions, live preview pixel proof,
released-window captures, Escape rollback, and input ownership. A touchscreen
contact held across Alt release did not click the revealed client's close button.
The keyboard-unplug trace shows libinput synthesizing Alt release before device
removal; the selected window remained focused, the overlay closed, and a typed
file-writing command proved modifier recovery. The native fixture uses temporary
uinput devices and does not establish physical finger feel or multi-output behavior.

`/tmp/hearthspace-window-switcher-native-1` also passed before the text alpha fix
and keyboard-unplug fixture were added. Final screenshots in the second run
verify clean text blending.

`/tmp/hearthspace-window-switcher-pan-regression-2` passed three-finger cardinal
and diagonal panning, combined zoom, cancellation, ignored two-contact input,
maximized-window restoration, and fullscreen preservation on both device types.
The first pan run was deliberately interrupted after review found its obsolete
requirement that every recognizer stop after a fourth contact. The corrected test
requires the original canvas transform and no open overlay while allowing a fresh
many-finger candidate. Both attempts retain their evidence and completed cleanup.
All runs restored tty2 and removed their owned sessions, app scopes, and runtimes.

The optimized session release `6f679aab710b` is installed; compositor, shell, and
session hashes match `target/release`. Final host checks show GNOME on tty2,
Mutter display power on, and both background T3 services active.
Final switcher checks passed: 513 Rust tests with E2E/accessibility enabled,
269 Python tests with E2E enabled, rustfmt, Clippy with warnings denied, Ruff, and
the full native/test-target build.

The spatial switcher refresh passed all 12 native smoke steps on 2026-10-21 in
`/tmp/hearthspace-window-switcher/native-vt-kdgury8l`, on the same AMD/amdgpu laptop
with kernel `7.0.0-38` and eDP-1 at 2560×1600, scale 2. The updated proof checks
fixed focus, viewport, and saved window geometry while held; stable MRU order;
intermediate live preview poses; and activation only on release. Keyboard,
touchpad, four/five-contact touchscreen, cancellation, competing input, late touch
release, and keyboard unplug recovery passed. Page-flip captures retain the live
client markers and selected title badge. Cleanup restored tty2 and removed the
owned service and runtime. Separate native-shell screenshot reviews covered
light/dark themes, photo backgrounds, 1×/2× scales, a 700×600 output, and reduced
motion under `/tmp/hearthspace-window-switcher/native-review-3`,
`native-compact-1`, and `native-reduced-1`.

`scripts/touch-drag-smoke --session /tmp/hearthspace-touch-drag-1` tests one-finger
window movement through the same uinput/libinput touchscreen path. It uses a
fresh native Firefox profile and a decorated Foot terminal, checking continued
movement, drop/repeat, client edge resizing, zoomed coordinates, second-contact
cancellation and device removal. Window geometry and captured landmarks must
agree. It requires the native Firefox package and touch-fixture dependencies
documented in [contributing](../CONTRIBUTING.md), plus one output at least
960×650 logical pixels. It owns and cleans up its VT session and temporary apps.

[GTK touchscreen acceptance](GTK_TOUCH_TESTING.md) covers native GTK 3/4
headerbar movement, release, repeated drags, canvas zoom, and content taps with
`scripts/gtk-touch-smoke`.

The [physical suspend acceptance test](POWER_TESTING.md) uses `scripts/power-smoke`
with explicit opt-in, a checked RTC wake alarm, a protocol-owned test lock, and
the independent native watchdog. It verifies lock presentation across real
sleep/resume, preserved applications, post-resume input and page flips, and
original-VT recovery. Read its prerequisites before attempting physical sleep.

For a capture timeout, use `--log-level
'info,hearthspace_compositor::compositor::capture=debug'` when starting the owned
session. Correlated capture IDs show request acceptance, pixel staging, PNG
encoding time, page-flip confirmation and reply delivery. Timeout warnings name
the unfinished phase. PNG encoding uses the fastest lossless preset; the capture
deadline and bounded encoding queue still apply.

Run automated regressions without taking a VT:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --features udev,e2e,test-apps -- -D warnings
HEARTHSPACE_E2E=1 HEARTHSPACE_REQUIRE_ATSPI=1 cargo test --workspace --all-targets --features udev,e2e,test-apps -- --test-threads=1
cargo build --workspace --all-targets --features udev,e2e,test-apps
HEARTHSPACE_E2E=1 python3 -m unittest discover -s scripts/tests -v
```

### Persistent switcher acceptance

The pinch smoke cases cover gesture recognition and ownership. The following
additional native acceptance steps verify interaction after a successful inward
pinch; they are not established by the historical held-switcher records above.
Use an owned session following the [native control skill](../.agents/skills/hearthspace-native-control/SKILL.md)
and record status plus fresh presented captures for each result.

1. Open several distinguishable windows, including one outside the current view.
   Complete a supported inward pinch and verify the centered live grid remains
   open with the next most recently used window selected. Focus, viewport, and
   saved window geometry must remain unchanged while previews are shown.
2. Click a preview, then repeat with a touchscreen tap. Each must activate and
   reveal the chosen window without delivering the interaction to its contents.
3. Reopen with a pinch and press Enter. Verify activation of the selected window.
4. Reopen for each dismissal method: Escape, a background click, and a background
   tap. Each must restore the original presentation without activating a preview.

## External Automation and Accessibility

For bidirectional copy/paste between real Wayland and X11 editors, use the
[clipboard acceptance workflow](CLIPBOARD.md#regression-workflow). Its
`scripts/clipboard-smoke` command retains exact-text assertions alongside native
captures and verifies launcher cleanup.

The WayDriver adapter can attach to the manifest's `runtime_dir`,
`wayland_display`, and `command_socket` without owning the native process:

```rust
let mut compositor = HearthspaceCompositor::attach(runtime_dir, wayland_display, command_socket);
compositor.start(None, None).await?;
// Use its state with HearthspaceInput and HearthspaceCapture.
compositor.stop().await?; // Detaches; launcher stop owns native teardown.
```

The launcher creates a private D-Bus session. Use its recorded
`dbus_session_bus_address` for `SessionAccessibility`; semantic targeting must not
fall back to the host desktop's accessibility bus. Call `enable(timeout)` once
before discovery to activate the accessibility bridges on that private bus.
`snapshot(timeout)` returns XML,
`find(xpath, timeout)` returns matches, and `activate(xpath, timeout)` requires
exactly one match. For example, `//*[@name='Save']` can locate an exposed Save
control. Verify the action through the resulting UI as with pointer input.

The agent CLI also exposes these semantic operations directly:

```sh
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 a11y snapshot
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 a11y find "//*[@name='Save']"
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 a11y activate "//PushButton[@name='Save']"
```

The harness builds and records `hearthspace-a11y` alongside the session binaries.
These commands require a live session manifest with a private bus address;
`--socket` alone cannot select accessibility. Activation requires exactly one
match and obtains a fresh capture afterward. Acceptance is not proof of an
application-specific result; inspect the resulting state as usual.

Snapshots resolve numeric AT-SPI roles when a toolkit such as AccessKit omits
role-name methods, and normalize duplicate toolkit attributes without replacing
the canonical object identity. Enabling a bridge does not synchronously publish
its tree; retry discovery within a deadline when an application has just appeared.
Use the observed role and accessible name when building a
selector; different toolkits may call a button `Button` or `PushButton`.
Accessibility bounds are toolkit-relative; use capture metadata for pointer
coordinates instead of treating those bounds as screen coordinates.

The terminal CLI is sufficient for an agent to operate this workflow. An MCP
wrapper and a Wayland image-copy-capture frontend can reuse these boundaries when
needed; neither is required for native testing.

## CSD Titlebar Regression Coverage

The headless `csd_` cases drive real Wayland and XWayland clients through touch
frames and client move/resize requests. They check focus handoff, release draining,
mixed mouse/touch ownership, cancellation and device removal, unrelated window
teardown, X11 root-coordinate synchronization, and delayed resize buffers.
Run them without a native session active:

```sh
cargo test --locked -p hearthspace-compositor --features udev,e2e,test-apps \
  --test headless_control csd_ -- --test-threads=1
```

The native Firefox/Foot smoke additionally verifies movement and rendered
content through libinput at canvas scales 1.0 and 0.8, including second-contact
cancellation and touchscreen unplug:

```sh
scripts/touch-drag-smoke --session /tmp/hearthspace-touch-drag
```

For Steam, use an owned native session and the optional
[Steam setup](../CONTRIBUTING.md#native-steam-touch-testing). Check repeated CSD
titlebar drags at both canvas scales, a physical mouse click while touch remains
held, second-contact cancellation, and touchscreen removal. After each sequence,
verify that movement stops on release and another drag works. Also exercise
Steam content and text input: a responsive compositor alone cannot prove that
the client recovered. The sign-in window can cover this without submitting
credentials, but does not establish behavior in signed-in or game windows.

Preserve app stderr, input traces, screenshots, geometry, hardware/output scale,
and session cleanup evidence. XWM tracing can distinguish `_NET_WM_MOVERESIZE`
from client-directed configure requests; do not assume every CSD toolkit uses
the same path. Keep run-specific results with the session artifacts.
