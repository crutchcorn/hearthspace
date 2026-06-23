# Setup

This project targets modern Linux systems with Wayland only. The initial proof-of-concept is planned as a nested Wayland compositor built with Rust and Smithay.

## Ubuntu 26.04 Packages

The following packages were installed on the development VM:

```sh
sudo apt-get update
sudo apt-get install -y build-essential cargo rustc rustfmt pkg-config clang libclang-dev libwayland-dev wayland-protocols wayland-utils libinput-dev libxkbcommon-dev libxkbcommon-x11-dev libudev-dev libseat-dev libgbm-dev libegl1-mesa-dev libgles2-mesa-dev libdrm-dev libsystemd-dev foot
```

`foot` is installed as a small Wayland-native terminal for early spawn testing.

`libxkbcommon-x11-dev` is required by the published `gpui` crate's Linux stack, even when Hearthspace uses GPUI as a Wayland shell client.

## Verified Versions

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
foot: 1.25.0
```

Note: the pkg-config module for xkbcommon is `xkbcommon`, not `libxkbcommon`.

## Smithay Probe

Smithay 0.7.0 is current on crates.io and requires Rust 1.80.1 or newer.

The Smithay nested compositor example was check-built successfully with the feature set we expect to start from:

```sh
CARGO_TARGET_DIR="/tmp/opencode/hearthspace-smithay-target" cargo check --manifest-path "/home/crutchcorn/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/smithay-0.7.0/Cargo.toml" --example minimal --no-default-features --features backend_winit,renderer_gl,wayland_frontend
```

This validates that the installed system packages are sufficient for a nested Smithay compositor using the winit backend and GLES rendering.
