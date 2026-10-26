# E2E Testing

This document explains Hearthspace's end-to-end testing model: what an E2E test
is allowed to prove, why the harness runs a real compositor and real clients,
and which boundaries contributors should preserve.

Tests cross the compositor's process, Wayland, input, rendering, and accessibility
boundaries. Use them to verify that a real client starts, receives input, paints,
and shuts down correctly. Small internal decisions belong in unit tests.
Prefer AT-SPI accessibility discovery for UI semantics, private runtimes for
isolation, and headless rendering for deterministic regression checks.

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
  The [daily workflow](DAILY_SMOKE.md) adds keyboard launch/search, Unicode,
  window-state recovery, bounded settings I/O, and semantic notifications.

Keep these layers distinct. The control-socket tests prove our direct test
surface works. The WayDriver tests prove an external automation harness can drive
Hearthspace like a compositor under test. Native tests exercise display and
session boundaries that an offscreen framebuffer cannot cover. Their results are specific to the tested configuration;
none of these layers alone establishes daily-driver reliability.

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

Run these commands from the repository root in a dedicated test shell, so the
software renderer variables do not carry into a later native session. Set
`VK_DRIVER_FILES` to the installed Lavapipe ICD; the path below is the Ubuntu
26.04 package layout. Build the intended checkout before running Python fixtures.

Run headless regressions with the `e2e` feature:

```sh
export TMPDIR="$(mktemp -d -t hs-e2e.XXXXXX)"
export LIBGL_ALWAYS_SOFTWARE=1 GSK_RENDERER=cairo
export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json
cargo test --locked -p hearthspace-compositor --features e2e --test headless_control -- --test-threads=1
cargo test --locked -p hearthspace-compositor --features e2e --test waydriver_hearthspace -- --test-threads=1
```

Keep `TMPDIR` short: fixtures append private runtime and socket names, which must
fit Linux's Unix-socket path limit. A long artifact-directory name can make a
child exit before accessibility registration. Store descriptive run logs outside
that short temporary root if needed.

Add `test-apps` when exercising the optional GTK client. Pure transport,
coordinate mapping, input cleanup, and timeout tests belong below the native
suite so they do not require a display device or a VT switch.

The [clipboard regression](CLIPBOARD.md#regression-workflow) runs GTK editors on
both Wayland and XWayland and checks exact pasted text through ordinary keyboard
shortcuts. Its Python headless test shares the same assertions with the native
clipboard smoke command.

## CI Coverage and Evidence

CI runs the ordinary workspace and Python tests separately from the full E2E
configuration. The E2E job enables `udev,e2e,test-apps`, runs Rust tests one at a
time, and sets `HEARTHSPACE_E2E=1`, `HEARTHSPACE_REQUIRE_ATSPI=1`,
`HEARTHSPACE_REQUIRE_TRAY_LIBRARY=1`, `LIBGL_ALWAYS_SOFTWARE=1`,
`GSK_RENDERER=cairo`, and `VK_DRIVER_FILES` pointing to Mesa's Lavapipe ICD for
both Rust and Python. Required accessibility or deployed
Ayatana libraries cannot silently skip their coverage. Native VT/device tests
remain a separate hardware workflow.

Build and test the intended checkout. The adapter
[attachment tests](../crates/waydriver-hearthspace/tests/attached_session.rs) and
several Python fixtures accept `HEARTHSPACE_BINARY` as an explicit executable
path. Set it to the built binary when using those fixtures with a shared target
directory. This is not a workspace-wide override: other Rust tests use Cargo
executable paths, and some Python fixtures require `target/debug/hearthspace`.

`GSK_RENDERER=cairo` keeps GTK fixtures on software-rendered shared-memory
buffers while the compositor uses Mesa's software GLES renderer. Setting only
`LIBGL_ALWAYS_SOFTWARE=1` can leave GTK using Vulkan and submitting DMA-BUFs that
this software compositor cannot import reliably. The shell also uses Vulkan;
`VK_DRIVER_FILES` keeps it on the software device. The path above is provided
by Ubuntu 26.04's `mesa-vulkan-drivers`; adjust it to your installed Lavapipe ICD
on other distributions. Use these settings for local headless suites, including
on machines with a physical GPU. Keep them scoped to the test shell: exporting them into a native session would
change its renderer selection. Native GPU and DMA-BUF validation remains part of
the hardware workflow.

GTK fixtures must work without an inherited desktop session bus. The shell-bar
fixture sets an explicit program-name fallback and verifies its outgoing Wayland
app ID with the host bus removed. Render checks wait for visible client pixels;
mapped/maximized state alone does not establish that a buffer has been painted.

The [E2E job](../.github/workflows/e2e.yml) uses a dedicated temporary artifact
root and uploads `headless-e2e-evidence` for seven days on both success and failure. It includes test logs, Python workflow
screenshots/accessibility snapshots, and retained Rust failure directories.
Artifacts are limited to test-created log, JSON/JSONL, PNG, XML, and text files;
ordinary session sockets and the host desktop are excluded.

## Native Validation

The native suite is opt-in because it temporarily takes an unused VT and a real
seat. The launcher owns allocation, readiness, an independent watchdog, and
restoring the original VT. A test must preserve artifacts even if its assertions
fail, and teardown must release all owned desktop processes and runtime files.

Use the [native testing workflow](NATIVE_TESTING.md) for host prerequisites,
`scripts/native-smoke` cases, recovery checks, and artifact interpretation.
Record the GPU/driver, output configuration, cursor presentation path, and
session setup alongside results. Keep environment coverage in
[tested environments](TESTED_ENVIRONMENTS.md), and retain detailed assertions with
the run artifacts. A headless pass does not establish native presentation or recovery.

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
touch-down <device> <slot> <logical-x> <logical-y>
touch-motion <device> <slot> <logical-x> <logical-y>
touch-up <device> <slot>
touch-frame <device>
touch-cancel <device> [slot]
touch-remove <device>
screenshot
capture [output-name]
status
apps
windows
focus-window <id>
close-window <id>
move-window <id> <canvas-x> <canvas-y>
resize-window <id> <content-width> <content-height>
fullscreen-window <id> true|false
maximize-window <id> true|false
restore-window <id>
focus-next-window
focus-previous-window
focus-launcher
pan-to <canvas-x> <canvas-y>
zoom-to <scale>
lock-session
quit
```

Touch commands use the production touch recognizer, UI routing and pointer
handoff. Coordinates are global output-logical units; down/motion/up are batched
until `touch-frame`. Cancel and removal flush their cleanup immediately. Injected
device names are separated from hardware IDs. This permits deterministic delayed
client requests and frame-order tests, while native uinput tests additionally
cover libinput and device discovery.

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

`layout-tool` opens Change layout on the pointer's output. Editor actions use
`layout-command JSON` with the current `session` and `output` from `layout-status`,
an `action`, and a `target` such as `{"kind":"widget","id":"wifi"}`.
For example, `place` accepts `{"surface":"canvas"}` or
`{"surface":"frame","edge":"right","position":0.5}` in `placement`.
Session IDs prevent a stale native editor from applying actions to a new session.
`frame-reveal WINDOW_ID` reveals an attached live window without panning.
`frame-settings {"restore_tools":true}` restores Canvas tools to their default
frame placement. These are private compositor-control commands, not public
Wayland protocols. Use real pointer/keyboard input as well as commands when
validating the [layout interaction](SHELL.md#change-layout).

Keyboard commands use Linux evdev key codes at the socket boundary. The
compositor applies Smithay's expected XKB offset internally. Pointer button
commands use Linux input button codes, for example `272` (`0x110`) for the left
mouse button.

Pointer coordinates and deltas use global logical output coordinates, including
output offsets. Axis amounts use the control socket's scroll-distance units;
synthetic and physical scrolling share the compositor's Super modifier and
`--scroll-zooms` policy.

The `status` response also includes `viewport` with canvas offset `x`/`y`,
`scale`, and `pinch_active` (also true during a three-finger swipe).
Native [gesture tests](PINCH_ZOOM.md) inspect this
alongside window geometry and captures, while injecting temporary touch devices
through uinput rather than synthesizing gesture commands on the control socket.

Payloads depend on the request:

| Request | Payload |
| --- | --- |
| `screenshot` | PNG bytes for the default output; preserves the existing client contract. |
| `capture [output-name]` | One UTF-8 JSON metadata line, then PNG bytes. The length counts both. |
| `status` | UTF-8 JSON describing backend, activity, outputs/frame IDs, shell readiness, and mapped window geometry. |
| `windows` | Versioned JSON window metadata, including stable IDs, canvas and screen coordinates, focus, visibility, and presentation state. |
| `apps` | Versioned JSON catalog of launchable desktop entries. |
| `layout-status` | Version 1 frame layout, measured widget/group bounds, pinned-window state, editor session/selection, confirmation, and animation state. Redacted while locked or inactive. |
| `frame-settings-status` | Version 1 Canvas tools hiding preference. |

Capture metadata includes a version, output name, pixel dimensions, integer
scale, logical origin and bounds, transform, frame ID, cursor policy/source, and
presentation type. Captured images are upright. Convert screenshot pixels into
input coordinates with `logical_x + pixel_x / scale` and
`logical_y + pixel_y / scale`; rounded logical bounds do not replace the scale.
Frame IDs are scoped to one output in one compositor process.

Native capture reads the normal output render target and replies after that
frame's page flip and PNG completion, reporting `presentation: "page-flip"`.
PNG encoding runs off the native event-loop thread with two bounded jobs;
GPU readback remains on that thread. A late encoding result cannot release a
capture invalidated by locking, output loss, or its deadline. Headless and nested
captures report `presentation: "offscreen"`. Native capture includes the
software/client cursor in the framebuffer or overlays the hardware cursor once;
`cursor_source` identifies the path and `cursor_sample: "render"` identifies when
the independently moving hardware cursor was sampled.

`lock-session` acknowledges the locker request; tests must wait for both
`locked` and `lock_confirmed` in status before claiming secure presentation.
Locked status redacts application/view metadata and other desktop automation
commands are rejected. Protocol lock fixtures exercise isolation without
claiming password-authentication coverage; see [session locking](SESSION_LOCK.md).

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
[Native recording](NATIVE_RECORDING.md) documents bounded recording built on
individual captures and the evidence it can provide.

## WayDriver Adapter

WayDriver is the higher-level automation boundary. The adapter maps its
compositor, input and capture traits onto Hearthspace's runtime and control socket.

The adapter owns these responsibilities:

- Start an owned Hearthspace headless session with an isolated runtime directory,
  or attach to an explicitly selected existing session.
- Choose the headless resolution and integer output scale.
- Optionally start the Xilem shell.
- Translate WayDriver pointer and keyboard input into control-socket commands.
- Capture screenshots through the control socket instead of PipeWire/GStreamer.
- Stop an owned compositor through `quit` and kill it if it does not exit.

The adapter starts without a shell by default, so app-focused tests launch only
the client under test. Use the adapter's shell-enabled mode for tests that need
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

Discover root names from a fresh snapshot. Native shell fixtures target
`hearthspace-shell`; the optional GTK test app uses `hearthspace-gtk-test-app`.
Toolkit and executable identity affect these names, so do not reuse a selector
without checking the current tree.

## Test App Strategy

The in-repo GTK test app exists to give E2E tests a real Wayland client with a
known accessibility tree. It should be used when a test needs to prove client
launch, client rendering, AT-SPI discovery, input, and screenshot capture across
the whole compositor/client boundary.

The GTK test app is behind the `test-apps` Cargo feature. A compositor-only build
can omit it and its GTK dependency. Full workspace and desktop builds still need
GTK development headers for `hearthspace-lock`; see [dependencies](DEPENDENCIES.md).

When a test needs only compositor control-socket behavior, do not launch the GTK
app. Use the lower-level headless control tests instead.

## Maintaining coverage

Keep attachment separate from ownership: detaching must never stop an externally
managed session. Preserve private runtimes, sockets, and accessibility buses;
serialize tests that share process-wide environment or deterministic names.
Add keysym mappings in the WayDriver adapter when tests need more keyboard
coverage, and keep optional GTK clients behind `test-apps`.

Require both targeted application assertions and page-flip evidence for native
presentation claims. Retain failed assertions and cleanup results alongside
screenshots. Update this guide when changing the control protocol, adapter
lifecycle, accessibility setup, or test entrypoints.

The [compact shell acceptance](SHELL.md#acceptance) runs icon, search, notification,
and power-menu checks headlessly and on an owned native VT.
