# Native Testing

Native tests exercise seat ownership, DRM/KMS presentation, and recovery on a
real Linux virtual terminal (VT). Headless tests remain the default for
deterministic application and compositor regressions. Use native tests to evaluate
Hearthspace for everyday use on a particular display and input setup. A native capture proves that the captured output completed a page
flip; application-specific assertions must still establish that the UI responded
correctly. A smoke pass is not a long-term reliability guarantee.

The agent harness consists of a session launcher, a control CLI, and an acceptance
test. It runs the desktop as the current user while a transient system service
owns privileged VT setup and independent recovery. The caller can remain in a
background user service, including T3 Code, throughout the test.

## Host Requirements

Follow [dependency setup](DEPENDENCIES.md) for system dependencies and permissions. Run these checks before starting:

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

Run commands from the repository root. Use a new artifact directory for every
run, and keep its path for inspection and cleanup:

```sh
native_run="$(mktemp -d -t hearthspace-native.XXXXXX)"
scripts/hearthspace-agent --session "$native_run" --timeout 45 start --watchdog 300
scripts/hearthspace-agent --session "$native_run" status
scripts/hearthspace-agent --session "$native_run" screenshot "$native_run/desktop.png"
```

Start builds the runtime binaries in the
[artifact inventory](../scripts/runtime-artifacts.json) before changing VTs.
`start --no-build` explicitly uses existing debug binaries; build the required
native features first. `binaries.json`
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

For a fresh desktop or settings test, add `start --isolated-profile`. This opt-in
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
custom_run="$(mktemp -d -t hearthspace-native-custom.XXXXXX)"
scripts/hearthspace-native start --session "$custom_run" --no-build --watchdog 120
scripts/hearthspace-native status --session "$custom_run"
scripts/hearthspace-native stop --session "$custom_run"
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
scripts/hearthspace-agent --session "$native_run" apps
scripts/hearthspace-agent --session "$native_run" windows
scripts/hearthspace-agent --session "$native_run" focus 3
scripts/hearthspace-agent --session "$native_run" maximize 3
scripts/hearthspace-agent --session "$native_run" restore 3
scripts/hearthspace-agent --session "$native_run" launch foot
scripts/hearthspace-agent --session "$native_run" click 200 180
scripts/hearthspace-agent --session "$native_run" type 'hello from the agent'
scripts/hearthspace-agent --session "$native_run" key enter
scripts/hearthspace-agent --session "$native_run" paste 'Café 日本語 🦀' --shortcut ctrl+shift+v
scripts/hearthspace-agent --session "$native_run" drag 300 100 350 150 --save "$native_run/moved.png"
scripts/hearthspace-agent --session "$native_run" scroll -15
scripts/hearthspace-agent --session "$native_run" click 400 360 --units pixels --metadata "$native_run/desktop.json"
```

Replace the example window ID with one returned by `windows`. Use
`focus-next`, `focus-previous`, and `focus-launcher` for navigation without an ID.
`key super` toggles Apps List; `focus-launcher` explicitly opens it.
`key super+k` opens [canvas search](./CANVAS_SEARCH.md).
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
scripts/hearthspace-agent --session "$native_run" --timeout 10 wait
scripts/hearthspace-agent --session "$native_run" --timeout 10 wait --change-from "$native_run/desktop.png"
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

Native captures include the software/client cursor or overlay the hardware cursor
once. `cursor_source` identifies the path and `cursor_sample: render` identifies
the hardware cursor sampling boundary, which is independent of KMS page flips.
Captures allow at most 16,777,216 pixels and four pending requests, with a
five-second deadline. GPU readback stays on the compositor thread; PNG encoding
uses one worker with two bounded jobs. Replies require both encoding and the
captured frame's page flip. Locking, output loss, inactivity or failed presentation
invalidates pending captures. See the [capture contract](E2E_TESTING.md#control-socket).

## Stop and Recover

```sh
scripts/hearthspace-agent --session "$native_run" stop
```

Stop is idempotent. Recovery restores the recorded original VT, terminates the
owned service/session descendants, and removes its runtime directory. Screenshot
artifacts, manifests, and logs remain available. The independent systemd watchdog
uses the same recovery path after a hung compositor or interrupted agent.
Privileged recovery uses the root-owned state under `/run/hearthspace-native/`.

The launcher records ownership before allocating the private runtime. Stop
serializes with service submission, cancels the exact unit even when its start
is still queued, and waits for the unit and pending job to finish before
recovering resources. It releases the allocation lock while waiting so
`ExecStopPost` can perform recovery. An interrupted startup or uncertain stop
keeps ownership incomplete and blocks another allocation; a timeout is not
evidence that the session never started.

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
cargo build --locked --target-dir target --workspace --all-targets --features udev,e2e,test-apps
smoke_runs="$(mktemp -d -t hearthspace-smoke.XXXXXX)"
scripts/native-smoke --session "$smoke_runs/success" --case success --timeout 90
scripts/native-smoke --session "$smoke_runs/startup" --case startup-failure
scripts/native-smoke --session "$smoke_runs/crash" --case compositor-failure
scripts/native-smoke --session "$smoke_runs/watchdog" --case watchdog
scripts/native-smoke --session "$smoke_runs/interrupted" --case agent-interruption
```

The success case launches `foot`, types a visible marker, tracks its pixel
position after moving the window, and verifies that resizing reveals the
previously captured background. It checks movement of the terminal's software
cursor and the desktop cursor in targeted image regions, and captures available
outputs. Recovery cases inject failed startup,
compositor death, a frozen compositor (`SIGSTOP`) recovered by the watchdog, and
an owning agent killed with `SIGKILL` after readiness. Each run preserves commands, results,
screenshots, and metadata under its artifact directory and checks VT/runtime
restoration. Record GPU, driver, kernel, display scale and virtualization status
with the run artifacts. The [tested environments](TESTED_ENVIRONMENTS.md) page
centralizes environment coverage; a passing run applies only to the assertions
and configuration it exercised.

The startup-failure case requires a fresh artifact directory and evidence that
the injected failing compositor actually ran in its allocated PAM/VT session.
A preflight failure, such as missing sudo access, fails the test. Run native
validation separately from GPU-intensive headless suites so concurrent workload
does not obscure capture and presentation failures.

For displays attached to different GPUs, compare connected DRM connectors with
the harness's live `status` outputs before testing interaction. Capture each
output separately, move one real application between them, and verify its visible
content after resizing, panning, and zooming. Open Settings → Displays and check
that every live connector is available there too. A connector discovered in
sysfs without a corresponding live output is a failure, not successful hotplug.

Keep a separate movement interval without captures or recordings for flashing
and frame-pacing measurements. Capture readback can synchronize outstanding GPU
work and hide a scanout race. This excludes harness readbacks, not internal
readback required by a cross-GPU CPU-copy fallback; report that fallback when it
is active. Use the same binary profile, output arrangement, and workload
for comparisons; report added outputs or changed resolution as a changed workload.
The [rendering guide](RENDERING.md#investigating-slow-frames) describes callback
traces and the limits of screenshot evidence. A successful multi-GPU session
does not establish unplug/replug, suspend, or compatibility with other drivers.

The [gesture acceptance tests](PINCH_ZOOM.md)
use temporary uinput touchpad and touchscreen devices to exercise libinput input
and verify rendered three-finger canvas panning, zoom and combined movement.
Run `scripts/pan-smoke` and `scripts/pinch-smoke` separately from the smoke cases
above.

For the mouse tool, run:

```sh
canvas_pan_run="$(mktemp -d -t hearthspace-canvas-pan.XXXXXX)"
scripts/canvas-pan-smoke --session "$canvas_pan_run"
```

This uses an isolated profile and the visible Canvas tools chooser. It checks
viewport movement and presented terminal markers at 80%, 100%, and 200% zoom,
Escape cancellation with held buttons, ❖ + H/❖ + V shortcuts, and normal client
input and titlebar movement while Pan remains selected. It also checks temporary
❖ + left-drag from empty canvas, selected-tool retention, both modifier/button
release orders, and ordinary dragging afterward. It uses the compositor's
agent input path; three-finger libinput coverage remains in `scripts/pan-smoke`.

For switcher acceptance, allocate a fresh run:

```sh
switcher_run="$(mktemp -d -t hearthspace-switcher.XXXXXX)"
scripts/window-switcher-smoke --session "$switcher_run"
```

It checks held Alt+Tab/Alt+Shift+Tab and left/right swipes with four touchpad
contacts or four/five touchscreen contacts. Focus, viewport and stored geometry
must stay fixed while live previews animate. Captured client markers and the
revealed window establish rendering, alongside input-ownership and cancellation
checks. A contact held across Alt release must not click the revealed client;
unplugging an Alt-held keyboard must clear the overlay and permit exact input.

Pinch cases use the same contact counts. Inward completion leaves the switcher
open; outward completion dismisses it. Subthreshold gestures preserve the prior
open/closed state. Cancellation, repeated inward gestures, keyboard-owned sessions
and focused-view transitions have separate assertions. Independent libinput
traces record gesture type, direction and contact count. The script owns its
session and devices; it does not establish physical finger feel. Use the
[persistent switcher steps](#persistent-switcher-acceptance) for preview activation
and background dismissal.

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

`scripts/touch-drag-smoke` tests one-finger window movement through the same uinput/libinput touchscreen path. It uses a
fresh native Firefox profile and a decorated Foot terminal, checking continued
movement, drop/repeat, client edge resizing, zoomed coordinates, second-contact
cancellation and device removal. Window geometry and captured landmarks must
agree. It requires the native Firefox package and touch-fixture dependencies
documented in [dependency setup](DEPENDENCIES.md), plus one output at least
960×650 logical pixels. It owns and cleans up its VT session and temporary apps.

[GTK touchscreen acceptance](GTK_TOUCH_TESTING.md) covers native GTK 3/4
headerbar movement, release, repeated drags, canvas zoom, and content taps with
`scripts/gtk-touch-smoke`.

The [physical suspend acceptance test](POWER_TESTING.md) uses `scripts/power-smoke`
with explicit opt-in, a checked RTC wake alarm, a protocol-owned test lock, and
the independent native watchdog. It verifies lock presentation across real
sleep/resume, preserved applications, post-resume input and page flips, and
original-VT recovery. Its real-locker mode separately checks automatic locking
and, with local user participation, password unlock. Read its prerequisites
before attempting physical sleep.

For a capture timeout, use `--log-level
'info,hearthspace_compositor::compositor::capture=debug'` when starting the owned
session. Correlated capture IDs show request acceptance, pixel staging, PNG
encoding time, page-flip confirmation and reply delivery. Timeout warnings name
the unfinished phase. PNG encoding uses the fastest lossless preset; the capture
deadline and bounded encoding queue still apply.

Run deterministic regressions using the software renderer configuration in
[E2E testing](E2E_TESTING.md#headless-compositor). Keep that GPU workload separate
from native presentation tests.

### Persistent switcher acceptance

The pinch smoke cases cover gesture recognition and ownership. The following
additional native acceptance steps verify interaction after a successful inward
pinch. Record them separately from the automated gesture-recognition checks.
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

Use the manifest's `dbus_session_bus_address` for `SessionAccessibility`; never
fall back to the host desktop bus. Call `enable(timeout)` before discovery.
`snapshot(timeout)` returns XML, `find(xpath, timeout)` returns matches, and
`activate(xpath, timeout)` requires exactly one match. Verify the resulting UI;
accepted actions are not application-specific success evidence.

The agent CLI also exposes these semantic operations directly:

```sh
scripts/hearthspace-agent --session "$native_run" a11y snapshot
scripts/hearthspace-agent --session "$native_run" a11y find "//*[@name='Save']"
scripts/hearthspace-agent --session "$native_run" a11y activate "//PushButton[@name='Save']"
```

These commands require the manifest's private bus address; `--socket` alone
cannot select accessibility. Activation requires one match and obtains a fresh
capture. A newly enabled bridge may need bounded retries before its tree appears.
Snapshots resolve numeric AT-SPI roles and normalize duplicate attributes without
replacing object identity. Use observed names and roles (`Button` and `PushButton`
can differ by toolkit). Accessibility bounds are toolkit-relative; use capture
metadata to establish pointer coordinates.

The terminal CLI provides native automation without an MCP wrapper. For bounded
screen recordings and their capture limitations, see [native recording](NATIVE_RECORDING.md).

## CSD Titlebar Regression Coverage

The headless `csd_` cases drive real Wayland and XWayland clients through touch
frames and client move/resize requests. They check focus handoff, release draining,
mixed mouse/touch ownership, cancellation and device removal, unrelated window
teardown, X11 root-coordinate synchronization, and delayed resize buffers.
Use the [headless renderer environment](E2E_TESTING.md#headless-compositor) and
run them without a native session active:

```sh
cargo test --locked -p hearthspace-compositor --features udev,e2e,test-apps \
  --test headless_control csd_ -- --test-threads=1
```

The native Firefox/Foot smoke additionally verifies movement and rendered
content through libinput at canvas scales 1.0 and 0.8, including second-contact
cancellation and touchscreen unplug:

```sh
touch_run="$(mktemp -d -t hearthspace-touch-drag.XXXXXX)"
scripts/touch-drag-smoke --session "$touch_run"
```

For Steam, use an owned native session and the optional
[Steam dependencies](DEPENDENCIES.md). Check repeated CSD
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
