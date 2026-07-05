# E2E Testing

This document explains Hearthspace's end-to-end testing model: what an E2E test
is allowed to prove, why the harness runs a real compositor and real clients,
and which boundaries contributors should preserve.

The policy is:

- Test user-visible behavior through real compositor boundaries whenever
  possible.
- Prefer accessibility-driven discovery over compositor-private test hooks for
  UI semantics.
- Keep test runtime state isolated from the developer's desktop session.
- Use the headless backend for deterministic automation, not the nested or native
  display backends.
- Keep low-level control-socket tests separate from WayDriver session tests.

## The Mental Model

E2E tests sit above unit tests and integration tests. They should exercise the
same boundaries a user or external automation tool would cross:

- A compositor process starts.
- A Wayland display exists.
- A real Wayland client connects.
- Input is routed through the compositor seat.
- Frames are rendered into a framebuffer.
- Screenshots are captured from compositor output.
- UI semantics are discovered through AT-SPI accessibility trees where possible.

Do not use E2E tests to assert small internal implementation details. Use them to
prove that whole systems still compose correctly: compositor startup, client
launch, input delivery, rendering, screenshot capture, accessibility exposure,
and teardown.

## Test Layers

Hearthspace has two E2E layers:

- Headless control-socket tests: low-level smoke tests for compositor startup,
  synthetic input commands, screenshot readback, optional test-app spawning, and
  shutdown.
- WayDriver tests: higher-level automation tests that use the published
  WayDriver traits for compositor lifecycle, input, capture, app sessions, and
  accessibility/XPath lookup.

Keep these layers distinct. The control-socket tests prove our direct test
surface works. The WayDriver tests prove an external automation harness can drive
Hearthspace like a compositor under test.

## Headless Compositor

E2E automation should use headless mode. Headless mode runs the real
Smithay compositor state and shared renderer, but presents into an offscreen
EGL/GLES target instead of a real or host-managed display.

Headless mode is the right E2E default because it is:

- Deterministic: framebuffer size and scale are explicit.
- Isolated: each run can use its own `XDG_RUNTIME_DIR` and Wayland socket.
- Fast: it does not need a VT, KMS device, or nested host window.
- Close enough to production compositor behavior to exercise Wayland clients,
  input routing, rendering, and screenshots.

## Runtime Isolation

E2E tests must not depend on or mutate the developer's live desktop session.

Preserve these isolation rules:

- Use a temporary `XDG_RUNTIME_DIR` per compositor run.
- Use deterministic or test-owned Wayland display names inside that runtime dir.
- Keep command sockets inside the test runtime dir.
- Prefer private D-Bus session buses for AT-SPI tests.
- Restore process environment variables after tests that must change them.
- Serialize tests that share deterministic socket names or mutate process-wide
  environment.

The WayDriver adapter uses `wayland-99` and `hearthspace-shell.sock` inside its
temporary runtime directory. Lower-level tests may override the command socket
path with `HEARTHSPACE_COMMAND_SOCKET` when they need direct control over the
socket location.

## Control Socket

The control socket is the compositor-owned test and shell command surface. It is
useful for tests because it crosses a process boundary while remaining small and
deterministic.

The default socket name is `hearthspace-shell.sock` under `XDG_RUNTIME_DIR`.
Shell clients receive the full path through `HEARTHSPACE_COMMAND_SOCKET`; tests
can connect to it directly.

Requests are newline-terminated UTF-8 commands. Replies are:

```text
ok\n
err <message>\n
ok <byte-count>\n<PNG bytes>
```

Useful commands for E2E and smoke tests include:

```text
key-down <evdev-keycode>
key-up <evdev-keycode>
pointer-motion-abs <x> <y>
pointer-motion-rel <dx> <dy>
pointer-button-down <linux-button-code>
pointer-button-up <linux-button-code>
axis <horizontal> <vertical>
screenshot
quit
```

Feature-gated or shell-oriented commands can also be useful in targeted tests:

```text
spawn a11y-test
spawn foot
launch-app <app-id>
pan-left
pan-right
pan-up
pan-down
zoom-in
zoom-out
reload-settings
log-a11y-tree
```

Keyboard commands use Linux evdev key codes at the socket boundary. The
compositor applies Smithay's expected XKB offset internally. Pointer button
commands use Linux input button codes, for example `272` (`0x110`) for the left
mouse button.

Screenshots are direct compositor framebuffer readbacks encoded as PNG and
returned as `ok <byte-count>\n<PNG bytes>`. Continuous video is not part of the
Hearthspace adapter contract; use PNG screenshots for E2E assertions and
artifacts.

## WayDriver Adapter

WayDriver is the higher-level automation boundary. The adapter maps WayDriver's compositor, input, and capture
traits onto Hearthspace's headless runtime and control socket.

The adapter owns these responsibilities:

- Start Hearthspace headless with an isolated runtime directory.
- Choose the headless resolution and integer output scale.
- Optionally start the Xilem shell.
- Translate WayDriver pointer and keyboard input into control-socket commands.
- Capture screenshots through the control socket instead of PipeWire/GStreamer.
- Stop the compositor through the `quit` command and kill it if it does not exit.

The adapter defaults to have no shell launched so app-focused tests start with only the
client under test. Use the adapter's shell-enabled mode for tests that need
Hearthspace shell chrome.

WayDriver accepts X11 keysyms at its trait boundary. The Hearthspace adapter maps
the keysyms needed by tests to evdev key codes before sending socket commands.
Add mappings when a test needs more keyboard coverage; do not bypass the input
backend just to set internal compositor state.

## Accessibility Strategy

E2E tests should discover UI semantics through AT-SPI when possible. This makes
tests double as accessibility regressions for both client applications and shell
chrome.

WayDriver locates elements through AT-SPI and XPath. It does not locate by
peeking into Hearthspace compositor internals. Preserve that boundary.

Client applications launched by WayDriver use the test compositor's Wayland
display/runtime directory. They may still need a D-Bus session bus for AT-SPI.
Tests that need reliable accessibility discovery should create a private session
bus and avoid using the developer's host bus.

The Xilem shell is a Wayland client. Masonry emits an AccessKit tree, and
AccessKit's Unix bridge exposes that tree on AT-SPI. On Unix, AccessKit registers
with AT-SPI only while `org.a11y.Status.ScreenReaderEnabled` is active, so tests
that validate shell accessibility should enable that status on a private bus.

Important accessible root names:

- The Xilem shell appears as `hearthspace`.
- The GTK test app appears as `hearthspace-gtk-test-app` because GTK uses
  `argv[0]` for the AT-SPI application root.

## Test App Strategy

The in-repo GTK test app exists to give E2E tests a real Wayland client with a
known accessibility tree. It should be used when a test needs to prove client
launch, client rendering, AT-SPI discovery, input, and screenshot capture across
the whole compositor/client boundary.

The GTK test app is behind the `test-apps` Cargo feature. Keep it optional so
normal builds do not require GTK development headers.

When a test needs only compositor control-socket behavior, do not launch the GTK
app. Use the lower-level headless control tests instead.

## Contributor Rules

Use these rules when changing E2E tests or harness code:

- Use E2E tests for cross-boundary behavior, not small internal logic.
- Keep headless control-socket tests and WayDriver session tests conceptually
  separate.
- Keep runtime directories, command sockets, and AT-SPI buses isolated.
- Prefer AT-SPI/XPath for semantic UI lookup.
- Use screenshots as output artifacts and visual assertions; do not introduce a
  video dependency unless Hearthspace actually needs video capture semantics.
- Add keysym mappings in the WayDriver adapter when tests need more keyboard
  coverage.
- Keep optional clients behind `test-apps`.
- Update this document when changing headless flags, control-socket protocol,
  WayDriver adapter behavior, accessibility setup, or E2E test layout.
