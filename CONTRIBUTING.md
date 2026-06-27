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
sudo apt-get install -y build-essential cargo rustc rustfmt pkg-config clang libclang-dev libwayland-dev wayland-protocols wayland-utils libinput-dev libxkbcommon-dev libxkbcommon-x11-dev libudev-dev libseat-dev libgbm-dev libegl1-mesa-dev libgles2-mesa-dev libdrm-dev libsystemd-dev foot
```

`foot` is installed as a small Wayland-native terminal for server-side decoration testing.

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
