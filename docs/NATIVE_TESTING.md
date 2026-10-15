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
`focus-next`, `focus-previous`, `focus-launcher`, and `reset-view` for navigation
without an ID. `fullscreen ID --off` and `maximize ID --off` leave one state;
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
cargo build -p hearthspace-session -p hearthspace-compositor -p hearthspace-shell -p hearthspace-settingsd --features hearthspace-compositor/udev
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

`scripts/overview-smoke --session /tmp/hearthspace-overview-1` checks four-contact
touchpad and four/five-contact touchscreen inward and outward pinches. It pairs independent
libinput traces with live thumbnail captures, checks cancellation and unchanged
window/canvas geometry, then selects windows by pointer and touchscreen. Its
temporary windows include overlapping, off-canvas and fullscreen content.
Five-contact touchpad gestures are a documented libinput backend limitation and
are recorded separately from passing cases.

Agents can use `scripts/hearthspace-agent --session PATH overview on` (also
`off`/`toggle`) and `overview-select WINDOW_ID`. Unlocked `status` includes an
`overview` object with `active`, `animating`, `selected_window_id`, `output` and thumbnail
`items`. Card and preview bounds use global output-logical coordinates. Selection
exits overview and reveals the window; dismissal preserves the canvas view.
Wait for `animating: false` before asserting exact thumbnail colors, then capture
a fresh presented frame.

Overview acceptance on 2026-10-14 passed on the physical GPD laptop, AMD Radeon
890M (`amdgpu`), kernel `7.0.0-38-generic`, eDP-1 at 2560×1600 and scale 2.
`/tmp/hearthspace-overview-native-5` retains the complete successful run: native
page-flip captures, independent libinput traces, unchanged viewport/presentation
assertions, all three distinct client markers in their previews, and pointer/touch
selection. Inputs came from temporary uinput devices through the native input
backend. This is single-output acceptance, not a manual physical-finger test.

Earlier `hearthspace-overview-native-1` through `-4` and
`hearthspace-overview-touchscreen-1` remain under `/tmp` as failed attempts.
They exposed fixture movement below libinput's recognition threshold, captures
during the appearance fade, and a four-to-five-contact cancellation that libinput
did not report as cancelled. The final test uses four-to-three contacts and waits
for independently observed cancellation. `/tmp/hearthspace-overview-touchscreen-2`
also passed independently. All these runs restored tty2 and removed their owned
sessions and runtimes, including failures.

`/tmp/hearthspace-overview-pinch-regression-2` passed the existing three-finger
touchpad/touchscreen zoom, two-finger rejection, cancellation, removal, and
single-finger drag/tap checks. Its first attempt was interrupted after identifying
an obsolete assertion that expected no recognizer after a fourth contact; the
updated check requires restoration of the original three-finger canvas transform
and no overview opening from a subthreshold four-finger movement.

The final workspace checks passed: 484 Rust tests with E2E/accessibility enabled,
259 Python tests with E2E enabled, rustfmt, Clippy with warnings denied, Ruff, and
the full native/test-target build. `scripts/install-session install` built and
installed optimized release `178a39a6fa45`; its installed compositor, shell and
session hashes match `target/release`. GNOME remained active on tty2 with display
power on, and the background T3/remote services remained running.

`scripts/touch-drag-smoke --session /tmp/hearthspace-touch-drag-1` tests one-finger
window movement through the same uinput/libinput touchscreen path. It uses a
fresh native Firefox profile and a decorated Foot terminal, checking continued
movement, drop/repeat, client edge resizing, zoomed coordinates, second-contact
cancellation and device removal. Window geometry and captured landmarks must
agree. It requires the native Firefox package and touch-fixture dependencies
documented in [contributing](../CONTRIBUTING.md), plus one output at least
960×650 logical pixels. It owns and cleans up its VT session and temporary apps.

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
