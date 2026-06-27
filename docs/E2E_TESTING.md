# E2E Testing

Hearthspace's end-to-end tests run the real compositor, real Wayland clients,
real input routing, real framebuffer capture, and real AT-SPI accessibility
trees. The current harness is built around
[WayDriver](https://crates.io/crates/waydriver), with a Hearthspace-specific
backend adapter in this repository.

This document is the evergreen architecture reference for the E2E harness. Keep
it updated when changing headless runtime flags, the command socket protocol,
WayDriver adapter behavior, accessibility exposure, or E2E test layout.

## Goals

- Start Hearthspace without a physical display.
- Launch real Wayland clients inside the compositor.
- Drive keyboard, pointer, button, and scroll input through Smithay's seat.
- Capture screenshots from the compositor framebuffer.
- Locate and interact with UI through AT-SPI/XPath where possible.
- Keep test runtime state isolated from the developer's desktop session.

## Components

### Headless Compositor

`hearthspace --headless` starts the same Smithay compositor state used by the
nested winit backend, but renders to a surfaceless EGL/GLES offscreen target.
It advertises a synthetic Smithay `Output` and opens the deterministic Wayland
socket `wayland-99` inside `XDG_RUNTIME_DIR`.

Runtime flags:

```sh
hearthspace --headless
hearthspace --headless --headless-size 1280x720
hearthspace --headless --headless-scale 2
hearthspace --headless --no-shell
```

`--headless-size WIDTHxHEIGHT` configures the physical framebuffer size.
`--headless-scale INTEGER` configures the advertised Wayland scale. `--no-shell`
skips the Xilem shell client, which is useful for app-focused tests.

The implementation lives in `src/compositor/mod.rs`; CLI parsing lives in
`src/main.rs`; shared defaults and names live in `src/config.rs`.

### Control Socket

The compositor exposes a Unix stream control socket named
`hearthspace-shell.sock` in `XDG_RUNTIME_DIR`. Shell clients receive this path in
`HEARTHSPACE_COMMAND_SOCKET`, and tests can connect to it directly.

Requests are newline-terminated UTF-8 commands. Replies are:

```text
ok\n
err <message>\n
ok <byte-count>\n<PNG bytes>
```

Supported test-driving commands:

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

Keyboard commands currently accept Linux evdev key codes. The compositor adds
Smithay's expected XKB offset internally. Pointer button commands use Linux input
button codes, for example `272` (`0x110`) for the left mouse button.

Screenshots are direct GLES framebuffer readbacks encoded as PNG and returned as
`ok <byte-count>\n<PNG bytes>`. Continuous video is not implemented; screenshot
capture is the supported E2E capture path.

The parser and command model live in `src/shell/command.rs`; socket handling and
command execution live in `src/compositor/shell_integration.rs`; synthetic input
lives in `src/compositor/input.rs`.

### WayDriver Adapter

The crate `crates/waydriver-hearthspace` implements the published WayDriver
traits:

- `HearthspaceCompositor`: implements `CompositorRuntime` by spawning the
  `hearthspace` binary with `--headless`, an isolated `XDG_RUNTIME_DIR`, and the
  requested resolution/scale.
- `HearthspaceInput`: implements `InputBackend` by translating WayDriver calls
  into control-socket input commands.
- `HearthspaceCapture`: implements `CaptureBackend` by overriding screenshot
  capture to call the control socket instead of PipeWire/GStreamer.

The adapter defaults to `--no-shell` so app-focused sessions only contain the
client under test. Use `HearthspaceCompositor::with_shell()` for tests that need
the Xilem shell chrome.

WayDriver accepts X11 keysyms at the trait boundary. The adapter currently maps
common ASCII keys and a small set of control keysyms to evdev key codes before
sending control-socket commands. Add mappings in the adapter when a test needs
more keys, or add compositor-side keysym handling if that becomes preferable.

## Accessibility Strategy

WayDriver locates elements through AT-SPI and XPath. It does not locate through
the compositor protocol. This is intentional: E2E tests double as accessibility
regression tests for both client applications and shell chrome.

### Client Apps

Client applications launched by `Session::start` use the WayDriver-provided
Wayland display/runtime directory and the current D-Bus session bus. The
WayDriver GTK smoke test runs under a private D-Bus/AT-SPI session so minimal CI
images do not need a pre-existing user accessibility bus. It launches the
in-repo GTK test app (`--gtk-test-app`), waits for its AT-SPI application root
(`hearthspace-gtk-test-app`), locates the `Research Workspace` heading by XPath,
clicks it, and captures a screenshot.

GTK exposes the test app's AT-SPI application root using `argv[0]`, not the
window title or application id. The current root name is
`hearthspace-gtk-test-app`.

### Xilem Shell

The shell is a Xilem/Masonry Wayland client. Masonry emits an AccessKit tree,
and AccessKit's Unix bridge exposes that tree on AT-SPI. On Unix, AccessKit only
registers with AT-SPI while `org.a11y.Status.ScreenReaderEnabled` is active.

The AT-SPI WayDriver tests avoid touching the developer's host accessibility
state by starting a private `dbus-daemon --session`, temporarily setting
`DBUS_SESSION_BUS_ADDRESS` for the serialized test scope, enabling
`ScreenReaderEnabled` inside that private bus, and then launching headless
Hearthspace and any target client. The private bus is killed and the environment
is restored when each test ends.

Under AccessKit, the shell's AT-SPI application root is the executable name
`hearthspace`. The smoke test locates the `LEFT` shell control by XPath and
asserts that it has a non-empty bounding box.

## Test Files

`tests/headless_control.rs` is a lower-level integration smoke test for the
control socket. It starts `hearthspace --headless`, sends input commands,
captures a screenshot, and quits. With `--features test-apps`, it also asks the
compositor to spawn the GTK test app and drives input/screenshot commands
against it. This test intentionally exercises the compositor-side protocol
without using WayDriver.

`tests/waydriver_hearthspace.rs` exercises the WayDriver adapter and full
WayDriver `Session` path:

- `waydriver_backends_drive_input_capture_and_teardown` starts headless
  Hearthspace, drives WayDriver input calls, captures a PNG through
  `HearthspaceCapture`, and tears down.
- `waydriver_session_locates_xilem_shell_by_xpath` starts headless Hearthspace
  with the Xilem shell under a private D-Bus/AT-SPI session and locates the shell
  `LEFT` control by XPath.
- `waydriver_session_locates_real_client_by_xpath` is gated by `test-apps`; it
  launches the GTK test app through WayDriver under a private D-Bus/AT-SPI
  session, locates `Research Workspace` by XPath, clicks it, and captures a
  screenshot.

The E2E test targets require the Cargo feature `e2e`, which keeps them out of
normal `cargo test --all-targets` and CI runs. Tests are serialized inside each
test binary with a static lock because they share deterministic socket names
and, for the AT-SPI tests, temporarily modify process environment variables.

## Running Tests

Normal tests and lint:

```sh
cargo test
cargo clippy --all-targets
```

Headless control socket smoke tests:

```sh
cargo test --features e2e --test headless_control
cargo test --features e2e,test-apps --test headless_control
```

WayDriver smoke tests:

```sh
cargo test --features e2e --test waydriver_hearthspace
cargo test --features e2e,test-apps --test waydriver_hearthspace
```

When debugging AT-SPI discovery, enable WayDriver logs:

```sh
RUST_LOG=waydriver=debug cargo test --features e2e,test-apps --test waydriver_hearthspace -- --nocapture
```

## System Dependencies

The published `waydriver` crate links GStreamer even though the Hearthspace
adapter overrides screenshot capture. Install the development packages before
building tests that include `waydriver-hearthspace`. Those crates are gated
behind the Cargo feature `e2e`, so normal builds and CI do not require
GStreamer:

```sh
sudo apt-get install -y libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
```

The GTK test app requires GTK 4 development headers and the `test-apps` cargo
feature:

```sh
sudo apt-get install -y libgtk-4-dev
cargo test --features test-apps
```

The AT-SPI WayDriver tests require `dbus-daemon` and a working `org.a11y.Bus`
D-Bus service, normally provided by `dbus` and `at-spi2-core` packages on desktop
Linux systems.

## Current Limits

- Continuous video capture is not implemented for Hearthspace's adapter. Use PNG
  screenshots for assertions and artifacts.
- The control socket protocol is intentionally small and line-oriented. It is not
  a general binary RPC protocol.
- Keyboard input maps only the keysyms currently needed by tests. Add mappings as
  tests require them.
- E2E tests require surfaceless EGL support and the Cargo feature `e2e`. They
  are not part of the default `cargo test` run.
