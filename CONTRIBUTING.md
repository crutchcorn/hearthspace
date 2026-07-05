# Contributing

Hearthspace is early, and the architecture is still changing quickly.

Contributions and design discussions are welcome, especially around:

* Wayland compositor development
* Rust desktop infrastructure
* Shell UI architecture
* Infinite-canvas interaction design
* Workspace persistence
* Local-first AI systems

For larger changes, opening an issue or discussion first is recommended.

## Architecture Documentation

As this project grows, you can find technical documentation in the [docs/](./docs/) folder. As these are not intended to be user-facing, there is no website or rendered version of the docs.

## Setup

This project targets modern Linux systems with Wayland as the native display
protocol. X11 applications are supported through XWayland when the `Xwayland`
runtime is installed. Hearthspace is a Rust workspace with separate crates for
the session supervisor, compositor, shell, settings daemon, settings app, and
shared IPC/common code.

Ubuntu 26.04 LTS is our development and runtime baseline, and it is expected to have the oldest supported version of most packages.

### Development Environment Notes

I develop Hearthspace on Ubuntu 26.04 LTS, and I run it in a nested Wayland compositor inside a GNOME session. I use Parallels Desktop for macOS to run the Linux VM on an M4 Max 16" MacBook Pro.

As such, while my CPU, GPU, and RAM are all quite spec'd up, the VM is not a perfect representation of a real Linux system. Native DRM/KMS testing currently runs through a virtual Parallels/virgl stack, so real-hardware validation is still required for GPU, KMS, hotplug, and VT behavior. I also cannot test Wayland gestures reliably in the VM, so I have to run with `--scroll-zooms` to test zooming the canvas using the scroll wheel without a modifier key.

### Required Packages

I develop Hearthspace on Ubuntu 26.04 LTS, and the following packages are required to build and run the compositor and shell:

```sh
sudo apt-get install -y build-essential cargo rustc rustfmt pkg-config clang libclang-dev libwayland-dev wayland-protocols wayland-utils libinput-dev libxkbcommon-dev libxkbcommon-x11-dev libudev-dev libseat-dev libgbm-dev libegl1-mesa-dev libgles2-mesa-dev libdrm-dev libsystemd-dev
```

#### E2E Testing Dependencies

For E2E testing, the WayDriver adapter depends on the published `waydriver` crate, which
links GStreamer even when the Hearthspace backend overrides screenshot capture.
Install the development packages before building tests that include
`waydriver-hearthspace`. Those crates are gated behind the Cargo feature `e2e`,
so normal builds and CI do not require GStreamer:

```sh
sudo apt-get install -y libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
```

### Optional Dependencies

The settings app uses `rfd` for file selection. On Linux, `rfd` is built with
its XDG Desktop Portal backend and falls back to `zenity` if the portal request
fails. Hearthspace gives session children a private `XDG_RUNTIME_DIR` and strips
the host `DBUS_SESSION_BUS_ADDRESS` from shell-launched apps so file pickers do
not accidentally discover GNOME's session bus and open through GNOME's portal on
another TTY. Until Hearthspace has its own session bus and portal backend setup,
install `zenity` for the fallback path:

```sh
sudo apt-get install -y zenity
```

The file chooser portal is an external desktop service, not code inside the
Hearthspace compositor. If we later provide a Hearthspace-owned portal session,
the compositor still needs to support the dialog window created by the portal
backend.

X11 app support is provided by Smithay's XWayland integration. Install the
runtime when testing X11-only applications:

```sh
sudo apt-get install -y xwayland
```

For testing, `foot` is installed as a small Wayland-native terminal for server-side decoration testing.

```
sudo apt-get install -y foot
```

Cursor assets are stored as standard Xcursor files under `assets/cursors/`, with
editable SVG/PNG/config sources under `assets/cursors/source/`. Install
`xcursorgen` from `x11-apps` and ImageMagick when editing cursor assets:

```sh
sudo apt-get install -y x11-apps imagemagick
```

Regenerate the resize cursor assets from the workspace root with:

```sh
magick assets/cursors/source/ns-resize.svg assets/cursors/source/ns-resize.png
magick assets/cursors/source/ew-resize.svg assets/cursors/source/ew-resize.png
magick assets/cursors/source/nwse-resize.svg assets/cursors/source/nwse-resize.png
magick assets/cursors/source/nesw-resize.svg assets/cursors/source/nesw-resize.png
(cd assets/cursors/source && xcursorgen ns-resize.cursor ../ns-resize && xcursorgen ew-resize.cursor ../ew-resize && xcursorgen nwse-resize.cursor ../nwse-resize && xcursorgen nesw-resize.cursor ../nesw-resize)
```

### Optional Test Apps

Built-in test apps are gated behind the Cargo feature `test-apps` so normal compositor and shell builds do not require their dependencies.

The GTK accessibility test app requires GTK 4 development headers:

```sh
sudo apt-get install -y libgtk-4-dev
```

Build or run with the feature when you need the GTK test app or the shell's `A11yTest` spawn action:

```sh
cargo run -p hearthspace-compositor --bin hearthspace --features test-apps -- --gtk-test-app
cargo test -p hearthspace-compositor --features test-apps
```

Without `test-apps`, `--gtk-test-app` and the shell `A11yTest` action report that the binary must be rebuilt with `--features test-apps`.

### Workspace Packages

The root `Cargo.toml` is a virtual workspace. Its default member is `hearthspace-session`, so `cargo run` starts the session supervisor rather than the compositor directly.

Common packages:

```text
crates/hearthspace-session      session supervisor; root cargo run target
crates/hearthspace-compositor   Smithay compositor; binary: hearthspace-compositor
crates/hearthspace-shell        Xilem shell client; binary: hearthspace-shell
crates/hearthspace-settingsd    settings daemon; binary: hearthspace-settingsd
crates/hearthspace-settings     settings app; binary: hearthspace-settings
crates/hearthspace-ui-storybook UI component gallery app; binary: hearthspace-ui-storybook
crates/ui-components            shared Hearthspace Xilem UI components/theme
crates/xilem-storybook          generic Storybook-like state model for Xilem apps
crates/hearthspace-ipc          settings IPC protocol/types
crates/hearthspace-common       shared environment/path constants
crates/waydriver-hearthspace    WayDriver backend adapter
```

### Runtime And Test Flags

For a normal development session, run the session supervisor from the workspace root:

```sh
cargo run -- --dev
cargo run -- --dev --no-watch-apps
cargo run -- --dev --dry-run --print-env
```

The supervisor starts `hearthspace-settingsd`, `hearthspace-compositor --no-shell`, `hearthspace-shell`, and optionally a settings-app build watcher. It creates per-session sockets under `$XDG_RUNTIME_DIR/hearthspace/session-<pid>/` and passes them to child processes through environment variables.

From a real Linux TTY/VT, the same dev-session command launches the full native session:

```sh
cargo run -- --dev --no-watch-apps
```

In dev mode, the supervisor runs its compositor child as `cargo run -p hearthspace-compositor --features udev -- --no-shell --tty` from real TTY sessions and as `cargo run -p hearthspace-compositor --features udev -- --no-shell --winit` inside GNOME or another graphical host session. The explicit backend flag prevents stale host display variables from changing the intended session mode.

Use direct compositor commands when you want to test only the compositor backend:

Common compositor flags:

```sh
cargo run -p hearthspace-compositor -- --scroll-zooms
cargo run -p hearthspace-compositor -- --headless
cargo run -p hearthspace-compositor -- --headless --headless-size 1280x720
cargo run -p hearthspace-compositor -- --headless --headless-scale 2
cargo run -p hearthspace-compositor -- --headless --no-shell
cargo run -p hearthspace-compositor -- --winit
cargo run -p hearthspace-compositor --features udev -- --tty
cargo run -p hearthspace-compositor --no-default-features --features udev -- --tty --no-shell
cargo run -p hearthspace-compositor --features udev -- --tty --no-shell --exit-after-ms 10000
```

`--scroll-zooms` makes vertical scroll events zoom the canvas without holding
Super. This is mainly for nested compositor and VM testing where host gestures or
modifier routing are unreliable.

`--headless` starts Hearthspace with a surfaceless EGL/GLES renderer and an
offscreen virtual output instead of a host winit window. It still opens the
deterministic Wayland socket `wayland-99` in `XDG_RUNTIME_DIR` and starts the
shell as a normal Wayland client.

`--headless-size WIDTHxHEIGHT` overrides the headless virtual output size. The
default is `1280x720`; both `--headless-size 800x600` and
`--headless-size=800x600` are accepted.

`--headless-scale INTEGER` overrides the Wayland output scale advertised by the
headless backend. The default is `1`; both `--headless-scale 2` and
`--headless-scale=2` are accepted.

`--no-shell` skips spawning the Xilem shell client. This is useful for headless
harnesses that want to launch only the client under test.

`--exit-after-ms INTEGER` stops the compositor from inside its event loop after
the requested duration. This is primarily for native VT smoke tests where an
external `timeout` can terminate the process before DRM/KMS state is restored.

`--winit` forces the nested development backend. `--tty` selects the native
DRM/KMS backend path; it currently requires `--features udev`. Backend flags are
mutually exclusive: `--headless`, `--winit`, and `--tty` cannot be combined.

Native backend runs can be exited with `Ctrl+Alt+Backspace`, `Ctrl+Alt+Esc`,
`SIGINT`, `SIGTERM`, or `--exit-after-ms`. For repeatable native VT smoke tests
and log interpretation, see [docs/NATIVE_TESTING.md](./docs/NATIVE_TESTING.md).
For native backend architecture, see [docs/DRM.md](./docs/DRM.md).

Settings runs use separate packages rather than compositor runtime flags. Start the backend daemon first, then launch the Xilem frontend:

```sh
cargo build -p hearthspace-settings
cargo run -p hearthspace-settingsd
cargo run -p hearthspace-settings
```

The settings daemon writes `~/.config/hearthspace/config.toml` and listens on
`$XDG_RUNTIME_DIR/hearthspace-settingsd.sock`. The settings frontend is exposed
to the Hearthspace shell through
`data/applications/dev.hearthspace.Settings.desktop`; during source-tree runs,
the app catalog includes `data/` as a low-priority XDG data directory. Build the
settings frontend once with `cargo build -p hearthspace-settings` before
launching it from shell search so `target/debug/hearthspace-settings` exists.

The daemon's focused tests can be run with:

```sh
cargo test -p hearthspace-settingsd
```

### Shared UI Components

Reusable Hearthspace Xilem UI lives in `crates/ui-components`. Shell and
settings surfaces should prefer these components and theme helpers over raw
Xilem buttons, panels, sidebar rows, and section wrappers when the behavior is
not surface-specific. This keeps the shell, settings app, and future rich UI
clients visually consistent.

When adding or changing a reusable component:

- keep the generic visual primitive in `crates/ui-components`;
- use the existing component from shell/settings code instead of recreating the
  styling locally;
- add or update a story in `crates/hearthspace-ui-storybook` so the component
  can be inspected and tuned interactively;
- run the focused package checks before the full workspace pass:

```sh
cargo run -p hearthspace-ui-storybook
cargo check -p hearthspace-ui-components -p hearthspace-ui-storybook
cargo clippy -p hearthspace-ui-components -p hearthspace-ui-storybook --all-targets -- -D warnings
```

The storybook app is exposed to the Hearthspace shell through
`data/applications/dev.hearthspace.UIStorybook.desktop`. During `cargo run --
--dev`, the session supervisor also generates a session-local desktop entry that
launches it through `scripts/dev-run-app`, so it can be found from shell search
as "Hearthspace UI Storybook".

`crates/xilem-storybook` is intentionally Hearthspace-agnostic. It owns
storybook state, selected-story tracking, and typed live controls, but it does
not depend on `hearthspace-ui-components` or impose a theme. Other Xilem
projects should be able to reuse it with their own component library and view
layer.

### Tracing Logs

Hearthspace initializes `tracing` automatically. No Cargo feature flags are
required to enable logs; set `RUST_LOG` when running the binary.

Common examples:

```sh
RUST_LOG=info cargo run
RUST_LOG=hearthspace_compositor=debug cargo run -p hearthspace-compositor
RUST_LOG=hearthspace_compositor=trace cargo run -p hearthspace-compositor
RUST_LOG=hearthspace_compositor=debug,smithay=warn cargo run -p hearthspace-compositor
```

For noisy paths, target a narrower module:

```sh
RUST_LOG=hearthspace_compositor::compositor::input=trace,hearthspace_compositor=info cargo run -p hearthspace-compositor
```

Backend-specific examples:

```sh
RUST_LOG=hearthspace_compositor=debug cargo run -p hearthspace-compositor -- --headless
RUST_LOG=hearthspace_compositor=debug cargo run -p hearthspace-compositor --features udev -- --tty
```

The shell/control socket is `hearthspace-shell.sock` in `XDG_RUNTIME_DIR`. Shell
clients receive its full path through `HEARTHSPACE_COMMAND_SOCKET`, but tests can
connect to it directly. The protocol is line-oriented for requests. Successful
commands reply `ok\n`; screenshots reply `ok <byte-count>\n<PNG bytes>`; parsed
commands that fail reply `err <message>\n`.

Useful control commands for headless smoke tests:

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

Keyboard commands currently take Linux evdev key codes, not XKB keysyms. Pointer
button commands use Linux input button codes, for example `272` (`0x110`) for the
left mouse button.

The WayDriver backend adapter lives in `crates/waydriver-hearthspace` and uses
the published `waydriver` crate. Its E2E smoke tests can be run with:

```sh
cargo test -p hearthspace-compositor --features e2e --test waydriver_hearthspace
cargo test -p hearthspace-compositor --features e2e,test-apps --test waydriver_hearthspace
```

The feature-gated WayDriver `Session` test launches the GTK test app under a
private D-Bus/AT-SPI session, locates its `Research Workspace` heading by XPath,
clicks it, and captures a screenshot.

The non-`test-apps` WayDriver suite also verifies the Xilem shell's
Masonry/AccessKit tree under a private `dbus-daemon --session`. The tests point
Hearthspace and WayDriver at that throwaway bus and enable
`org.a11y.Status.ScreenReaderEnabled` because AccessKit's Unix bridge registers
with AT-SPI only while screen-reader status is active.

See [docs/E2E_TESTING.md](./docs/E2E_TESTING.md) for the evergreen technical
architecture of the headless and WayDriver E2E harness.

### Xilem Git Dependency

The shell UI, settings app, UI storybook, and shared UI components are built
with [Xilem](https://github.com/linebender/xilem). We depend on upstream Xilem
from git rather than the latest published crates because crates.io is still
behind APIs Hearthspace uses, including
`xilem::WindowOptionsExtLinux::with_name(general, instance)` for setting
Wayland `app_id` / X11 `WM_CLASS`.
Keep these dependencies pinned to an explicit commit until intentionally
updating Xilem and rerunning the full workspace checks.

```toml
masonry = { git = "https://github.com/linebender/xilem", rev = "c5950bcb03d4f3d187a20d1159f6aa276fd056bf", features = ["testing"] }
xilem = { git = "https://github.com/linebender/xilem", rev = "c5950bcb03d4f3d187a20d1159f6aa276fd056bf" }
```

No local Xilem clone is required for normal development; Cargo fetches the git
dependency automatically. A local clone at `~/git/linebender/xilem` is useful
for source search when working through Xilem/Masonry behavior or API questions.

### Linting

`cargo clippy` is run as part of CI (`.github/workflows/ci.yml`) and locally. On the development VM, clippy is provided by the apt package rather than `rustup`:

```sh
sudo apt-get install -y rust-clippy
```

### Verified Versions

The development VM currently has:

```text
rustc: 1.93.1
cargo: 1.93.1
rustfmt: 1.8.0
clang: 21.1.8
wayland-server: 1.24.0
wayland-protocols: 1.47
wayland-info: 1.3.0
libinput: 1.31.1
xkbcommon: 1.13.1
libseat: 0.9.2
gbm: 26.0.3-1ubuntu1
gtk4: 4.22.4
foot: 1.25.0
```

Note: the pkg-config module for xkbcommon is `xkbcommon`, not `libxkbcommon`.

### GNOME Host Session Tweaks

By default, GNOME uses dynamic workspaces and scroll-to-switch-workspace gestures. These interfere with the Hearthspace compositor, so tweaking GNOME's built-in settings may help fix the problem:

```sh
gsettings set org.gnome.mutter dynamic-workspaces false
gsettings set org.gnome.desktop.wm.preferences num-workspaces 1
gsettings set org.gnome.mutter overlay-key ''
gsettings set org.gnome.shell.extensions.dash-to-dock scroll-action 'do-nothing'
gsettings set org.gnome.shell.extensions.dash-to-dock scroll-switch-workspace false
```

To restore GNOME defaults:

```sh
gsettings reset org.gnome.mutter dynamic-workspaces
gsettings reset org.gnome.desktop.wm.preferences num-workspaces
gsettings reset org.gnome.mutter overlay-key
gsettings reset org.gnome.shell.extensions.dash-to-dock scroll-action
gsettings reset org.gnome.shell.extensions.dash-to-dock scroll-switch-workspace
```
