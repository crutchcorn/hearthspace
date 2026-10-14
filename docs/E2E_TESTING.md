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
- Use the headless backend for deterministic regression tests.
- Keep native seat, KMS, cursor-plane, and VT recovery tests in a separate,
  explicitly invoked suite.
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

Hearthspace has three complementary E2E layers:

- Headless control-socket tests: low-level smoke tests for compositor startup,
  synthetic input commands, screenshot readback, optional test-app spawning, and
  shutdown.
- WayDriver tests: higher-level automation tests that use the published
  WayDriver traits for compositor lifecycle, input, capture, app sessions, and
  accessibility/XPath lookup.
- Native smoke tests: full sessions on a spare VT, covering presentation through
  KMS, hardware/software cursor capture, output geometry, and recovery after
  startup failure, compositor failure, watchdog expiry, or agent interruption.

Keep these layers distinct. The control-socket tests prove our direct test
surface works. The WayDriver tests prove an external automation harness can drive
Hearthspace like a compositor under test. Native tests prove the host-dependent
display and session boundaries that an offscreen framebuffer cannot exercise.

## Headless Compositor

Deterministic E2E automation should use headless mode. Headless mode runs the real
Smithay compositor state and shared renderer, but presents into an offscreen
EGL/GLES target instead of a real or host-managed display.

Headless mode is the right E2E default because it is:

- Deterministic: framebuffer size and scale are explicit.
- Isolated: each run can use its own `XDG_RUNTIME_DIR` and Wayland socket.
- Fast: it does not need a VT, KMS device, or nested host window.
- Close enough to production compositor behavior to exercise Wayland clients,
  input routing, rendering, and screenshots.

Run headless regressions with the `e2e` feature:

```sh
cargo test -p hearthspace-compositor --features e2e --test headless_control
cargo test -p hearthspace-compositor --features e2e --test waydriver_hearthspace
```

Add `test-apps` when exercising the optional GTK client. Pure transport,
coordinate mapping, input cleanup, and timeout tests belong below the native
suite so they do not require a display device or a VT switch.

The [clipboard regression](CLIPBOARD.md#regression-workflow) runs GTK editors on
both Wayland and XWayland and checks exact pasted text through ordinary keyboard
shortcuts. Its Python headless test shares the same assertions with the native
clipboard smoke command.

## Native Validation

The native suite is opt-in because it temporarily takes an unused VT and a real
seat. The launcher owns allocation, readiness, an independent watchdog, and
restoring the original VT. A test must preserve artifacts even if its assertions
fail, and teardown must release all owned desktop processes and runtime files.

Use the [native testing workflow](NATIVE_TESTING.md) for host prerequisites,
`scripts/native-smoke` cases, recovery checks, and artifact interpretation.
Record the GPU/driver, output configuration, cursor presentation path, and
session setup alongside results. A passing VM run establishes coverage for that
virtual DRM stack; record real-hardware validation separately. A headless pass
does not certify native presentation or recovery.

## Runtime Isolation

Headless E2E tests must not depend on or mutate the developer's live desktop
session. Native runs temporarily switch away from it and must restore it.

Preserve these isolation rules:

- Use a private `XDG_RUNTIME_DIR` per compositor run.
- Use deterministic or test-owned Wayland display names inside that runtime dir.
- Keep command sockets inside the test runtime dir.
- Use a private D-Bus session bus for AT-SPI tests; never enable accessibility on
  the calling agent's host bus by accident.
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
ok <byte-count>\n<payload bytes>
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
capture [output-name]
status
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

Pointer coordinates and deltas use global logical output coordinates, including
output offsets. Axis amounts use the control socket's scroll-distance units;
synthetic and physical scrolling share the compositor's Super modifier and
`--scroll-zooms` policy.

The `status` response also includes `viewport` with canvas offset `x`/`y`,
`scale`, and `pinch_active`. Native [pinch tests](PINCH_ZOOM.md) inspect this
alongside window geometry and captures, while injecting temporary touch devices
through uinput rather than synthesizing pinch commands on the control socket.

Payloads depend on the request:

| Request | Payload |
| --- | --- |
| `screenshot` | PNG bytes for the default output; preserves the existing client contract. |
| `capture [output-name]` | One UTF-8 JSON metadata line, then PNG bytes. The length counts both. |
| `status` | UTF-8 JSON describing backend, activity, outputs/frame IDs, shell readiness, and mapped window geometry. |

Capture metadata includes a version, output name, pixel dimensions, integer
scale, logical origin and bounds, transform, frame ID, cursor policy/source, and
presentation type. Captured images are upright. Convert screenshot pixels into
input coordinates with `logical_x + pixel_x / scale` and
`logical_y + pixel_y / scale`; rounded logical bounds do not replace the scale.
Frame IDs are scoped to one output in one compositor process.

Native capture reads the normal output render target and replies after that
frame's page flip, reporting `presentation: "page-flip"`. Headless and nested
captures report `presentation: "offscreen"`. Native capture includes the
software/client cursor in the framebuffer or overlays the hardware cursor once;
`cursor_source` identifies the path and `cursor_sample: "render"` identifies when
the independently moving hardware cursor was sampled.

An input `ok` proves command acceptance. A page flip proves display-pipeline
progress. Neither proves that an application has finished responding. Verify
actions with a targeted screenshot or accessibility assertion; an arbitrary
sleep or any pixel change can be satisfied by unrelated animation or the cursor.

Commands on one connection execute in order, including deferred captures.
Clients must read the exact payload length and handle fragmented headers and
partial payloads. Invalid commands return `err`; output loss, inactive sessions,
and capture deadlines also fail explicitly. Queues and operation lifetimes are
bounded, so clients must set deadlines and handle disconnects rather than assume
an incomplete image succeeded. Continuous video is outside this capture contract.

## WayDriver Adapter

WayDriver is the higher-level automation boundary. The adapter maps WayDriver's compositor, input, and capture
traits onto Hearthspace's runtime and control socket.

The adapter owns these responsibilities:

- Start an owned Hearthspace headless session with an isolated runtime directory,
  or attach to an explicitly selected existing session.
- Choose the headless resolution and integer output scale.
- Optionally start the Xilem shell.
- Translate WayDriver pointer and keyboard input into control-socket commands.
- Capture screenshots through the control socket instead of PipeWire/GStreamer.
- Stop an owned compositor through `quit` and kill it if it does not exit.

The adapter defaults to have no shell launched so app-focused tests start with only the
client under test. Use the adapter's shell-enabled mode for tests that need
Hearthspace shell chrome.

Ownership is explicit. `HearthspaceCompositor::new(binary)` owns its headless
process and temporary runtime. `HearthspaceCompositor::attach(runtime_dir,
wayland_display, command_socket)` borrows an existing session; call
`start(None, None)` to check its control connection. Attached sessions reject
resolution/scale overrides. Stopping or dropping an attached adapter only
detaches it: the native launcher remains responsible for shutdown, VT recovery,
and runtime cleanup. Use `ownership()` when a harness needs to distinguish these
lifecycles.

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

For an existing Hearthspace session, read `dbus_session_bus_address` from the
launcher's `session.json` and construct
`waydriver_hearthspace::accessibility::SessionAccessibility` with that explicit
address. Call `enable(timeout).await` before discovery; this sets
`ScreenReaderEnabled` on the selected private bus. The helper's `snapshot`,
`find`, and `activate` operations also take deadlines and never consult the host
session environment. `activate` requires exactly one XPath match. An accepted
accessibility action still needs an application-specific assertion or fresh
capture to verify its visible result.

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
  separate, and invoke native validation separately from deterministic CI.
- Keep attachment distinct from ownership; detaching must never terminate an
  externally managed session.
- Require page-flip evidence for native presentation claims, and retain recovery
  results as well as screenshots.
- Keep runtime directories, command sockets, and AT-SPI buses isolated.
- Prefer AT-SPI/XPath for semantic UI lookup.
- Use screenshots as output artifacts and visual assertions; do not introduce a
  video dependency unless Hearthspace actually needs video capture semantics.
- Add keysym mappings in the WayDriver adapter when tests need more keyboard
  coverage.
- Keep optional clients behind `test-apps`.
- Update this document when changing headless flags, control-socket protocol,
  WayDriver adapter behavior, accessibility setup, or E2E test layout.
