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

This project targets modern Linux systems with Wayland only. The initial proof-of-concept is planned as a nested Wayland compositor built with Rust and Smithay.

Ubuntu 26.04 LTS is our development and runtime baseline, and it is expected to have the oldest supported version of most packages.

### My Environment

I develop Hearthspace on Ubuntu 26.04 LTS, and I run it in a nested Wayland compositor inside a GNOME session. I use Parallels Desktop for macOS to run the Linux VM on an M4 Max 16" MacBook Pro.

As such, while my CPU, GPU, and RAM are all quite spec'd up, the VM is not a perfect representation of a real Linux system. For example, the VM appears to only provide LavaPipe software rendering for OpenGL, so I cannot test GPU acceleration. I also cannot test Wayland gestures in the VM, so I have to run with `--scroll-zooms` to test zooming the canvas using the scroll wheel without a modifier key.

### Required Packages

I develop Hearthspace on Ubuntu 26.04 LTS, and the following packages are required to build and run the compositor and shell:

```sh
sudo apt-get install -y build-essential cargo rustc rustfmt pkg-config clang libclang-dev libwayland-dev wayland-protocols wayland-utils libinput-dev libxkbcommon-dev libxkbcommon-x11-dev libudev-dev libseat-dev libgbm-dev libegl1-mesa-dev libgles2-mesa-dev libdrm-dev libsystemd-dev
```

#### E2E Testing Dependencies

For E2E testing, the WayDriver adapter depends on the published `waydriver` crate, which
links GStreamer even when the Hearthspace backend overrides screenshot capture.
Install the development packages before building tests that include
`waydriver-hearthspace`:

```sh
sudo apt-get install -y libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
```

### Optional Dependencies

For testing, `foot` is installed as a small Wayland-native terminal for server-side decoration testing.

```
sudo apt-get install -y foot
```

### Optional Test Apps

Built-in test apps are gated behind the Cargo feature `test-apps` so normal compositor and shell builds do not require their dependencies.

The GTK accessibility test app requires GTK 4 development headers:

```sh
sudo apt-get install -y libgtk-4-dev
```

Build or run with the feature when you need the GTK test app or the shell's `A11yTest` spawn action:

```sh
cargo run --features test-apps -- --gtk-test-app
cargo test --features test-apps
```

Without `test-apps`, `--gtk-test-app` and the shell `A11yTest` action report that the binary must be rebuilt with `--features test-apps`.

### Runtime And Test Flags

Common compositor flags:

```sh
cargo run -- --scroll-zooms
cargo run -- --headless
cargo run -- --headless --headless-size 1280x720
cargo run -- --headless --headless-scale 2
cargo run -- --headless --no-shell
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
the published `waydriver` crate. Its ignored smoke tests can be run with:

```sh
cargo test --test waydriver_hearthspace -- --ignored
cargo test --features test-apps --test waydriver_hearthspace -- --ignored
```

The full WayDriver `Session` XPath test is compiled with `test-apps` but only
exercises AT-SPI when `HEARTHSPACE_REQUIRE_ATSPI=1` is set, because the VM can
render the GTK test client without exposing it as an AT-SPI application root.

### Xilem Fork (git dependency)

The shell UI is built with [Xilem](https://github.com/linebender/xilem). Stock
Xilem cannot set a Wayland `app_id` on its windows, which Hearthspace needs so
the compositor can recognize the shell surface and render it without window
chrome. We therefore depend on a fork branch via git in `Cargo.toml`:

```toml
masonry = { git = "https://github.com/crutchcorn/xilem", branch = "wayland-app-id", features = ["testing"] }
xilem = { git = "https://github.com/crutchcorn/xilem", branch = "wayland-app-id" }
```

The branch backs upstream PR
[linebender/xilem#1830](https://github.com/linebender/xilem/pull/1830), which
adds `xilem::WindowOptionsExtLinux::with_name(general, instance)` — forwarding to
winit's `WindowAttributesExtWayland::with_name` (Wayland `app_id`) and
`WindowAttributesExtX11::with_name` (X11 `WM_CLASS`). Once the PR merges, repoint
these dependencies to an upstream `linebender/xilem` release/rev. No local clone
is required; Cargo fetches the branch automatically.

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
