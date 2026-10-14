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

Start builds the session supervisor, native compositor, shell, and settings
daemon before changing VTs. `start --no-build` explicitly uses existing debug
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
the canvas zoom. Use screenshot metadata to act in captured pixel coordinates:

```sh
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 launch foot
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 click 200 180
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 type 'hello from the agent'
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 key enter
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 drag 300 100 350 150 --save /tmp/hearthspace-run-1/moved.png
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 scroll -15
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 click 400 360 --units pixels --metadata /tmp/hearthspace-run-1/desktop.json
```

`type` supports the CLI's US-layout key mappings, including printable ASCII,
newline, and tab. Unsupported characters fail before typing. Key chords use
names such as `ctrl+a`; input cleanup releases held keys and pointer buttons on
errors. Synthetic scrolling follows the same compositor zoom rules as physical
scrolling, including Super and `--scroll-zooms`.

Input operations wait for a fresh capture by default. `--wait change` waits for
different image pixels; `--wait none` reports command acceptance only. A cursor
move or animation can change pixels, so use a targeted assertion, window status,
or accessibility result to verify the intended action. For explicit waits:

```sh
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 --timeout 10 wait
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 --timeout 10 wait --change-from /tmp/hearthspace-run-1/desktop.png
```

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
five-second presentation deadline. Paused sessions, removed outputs, and failed
presentation return errors rather than stale success.

## Stop and Recover

```sh
scripts/hearthspace-agent --session /tmp/hearthspace-run-1 stop
```

Stop is idempotent. Recovery restores the recorded original VT, terminates the
owned service/session descendants, and removes its runtime directory. Screenshot
artifacts, manifests, and logs remain available. The independent systemd watchdog
uses the same recovery path after a hung compositor or interrupted agent.
Privileged recovery uses the root-owned state under `/run/hearthspace-native/`.

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

The terminal CLI is sufficient for an agent to operate this workflow. An MCP
wrapper and a Wayland image-copy-capture frontend can reuse these boundaries when
needed; neither is required for native testing.
