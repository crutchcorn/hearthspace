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

Agent-driven native tests also run on a physical GPD host with AMD Strix
Radeon 880M/890M graphics, the `amdgpu` driver, and Linux `7.0.0-38-generic`.
The Rust 1.99/dependency upgrade passed desktop and clipboard acceptance there
on `eDP-1` at 2560×1600 with scale 2, using a spare VT and restoring the original
desktop after each run. See [Native Testing](./docs/NATIVE_TESTING.md).

### Required Packages

I develop Hearthspace on Ubuntu 26.04 LTS, and the following packages are required to build and run the compositor and shell:

```sh
sudo apt-get install -y build-essential pkg-config clang libclang-dev libwayland-dev wayland-protocols wayland-utils libinput-dev libxkbcommon-dev libxkbcommon-x11-dev libudev-dev libseat-dev libgbm-dev libegl1-mesa-dev libgles2-mesa-dev libdrm-dev libsystemd-dev
```

### Rust Toolchain

Use [rustup](https://rust-lang.org/tools/install/) for Rust. The repository's
`rust-toolchain.toml` pins Rust **1.99.0** and installs Clippy, rustfmt,
rust-analyzer, and the standard-library sources. CI uses the same pin.
Ubuntu's Rust packages can be older than the project and its Xilem dependency.

On the development host, rustup was updated to **1.29.1** and its user-wide
default changed to **1.99.0** with:

```sh
rustup toolchain install 1.99.0 --profile minimal --component clippy,rustfmt,rust-analyzer,rust-src
rustup default 1.99.0
```

The existing `~/.profile` and `~/.bashrc` source `~/.cargo/env`, putting rustup's
commands ahead of `/usr/bin`. Distro-managed Rust packages remain installed;
normal user commands now use the rustup toolchain, including outside this
checkout. No system package files were overwritten. Verify with `rustc
--version`, `cargo --version`, and `rustup show`.

#### E2E Testing Dependencies

For E2E testing, the WayDriver adapter depends on the published `waydriver` crate, which
links GStreamer even when the Hearthspace backend overrides screenshot capture.
Install the development packages before building tests that include
`waydriver-hearthspace`. The compositor gates its adapter behind the Cargo
feature `e2e`; full workspace checks and CI also build the adapter crate itself
and therefore require GStreamer:

```sh
sudo apt-get install -y libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
```

### Native Agent Testing

The native automation harness and its host requirements are documented in
[docs/NATIVE_TESTING.md](./docs/NATIVE_TESTING.md). In addition to the native
build dependencies, it uses the Ubuntu systemd/logind login PAM stack and these
runtime tools:

```sh
sudo apt-get install -y python3 dbus util-linux sudo foot at-spi2-core
sudo -n true
```

The agent must be able to invoke sudo without an interactive password. Configure
that through the machine administrator's sudo policy before unattended native
tests; the harness does not change sudoers. This development machine's
passwordless-sudo policy was restored for native automation. A persistent agent
user service may also require `sudo loginctl enable-linger "$USER"`; disable it
with `sudo loginctl disable-linger "$USER"` only when that service no longer needs
to survive logout.

Each run creates a transient `hearthspace-native-*.service`, a PAM/logind session,
root-owned recovery records under `/run/hearthspace-native/`, and a private runtime
under `/run/user/UID/hearthspace-native-ID/`. The unit switches to an unused VT,
runs the desktop as the user, and has an independent maximum lifetime and cleanup
hook. No persistent service units or PAM configuration are installed. Stop a run
through the harness to restore its original VT and remove the private runtime;
recovery records under `/run` disappear on reboot. User-selected artifact
directories retain logs and screenshots until explicitly removed.

Native supervisor launches automatically create a private `dbus-run-session`
before starting desktop components. The harness already supplies that bus and
uses `--private-session-bus` to reuse it. Both routes explicitly provide
`HEARTHSPACE_SESSION_BUS_ADDRESS` to children so app launch and AT-SPI stay on the
Hearthspace bus. Before desktop startup, `dbus-update-activation-environment`
updates only the private bus with Hearthspace's display, runtime, and settings
sockets. It does not update the shared systemd user manager or the host desktop.
The bootstrap clears host display/socket and accessibility overrides before
creating its bus. Normal nested sessions continue to exclude the host bus from
shell-launched apps; use a dedicated `dbus-run-session` with
`--private-session-bus` when testing desktop service activation there.

Private sessions share the user's existing audio service. Unless explicitly
overridden, the supervisor exports `PULSE_SERVER=unix:$HOST_RUNTIME/pulse/native`
and `PIPEWIRE_REMOTE=$HOST_RUNTIME/pipewire-0`, with `PIPEWIRE_RUNTIME_DIR` pointing
to the original runtime directory. Here `$HOST_RUNTIME` is
`HEARTHSPACE_HOST_XDG_RUNTIME_DIR`, or the inherited `XDG_RUNTIME_DIR` when no host
override exists. Explicit PulseAudio/PipeWire server and runtime variables are
preserved. This keeps audio clients connected while Wayland and desktop D-Bus
remain private; it does not start another audio daemon. The endpoint variables
follow the [PipeWire native protocol](https://docs.pipewire.org/page_module_protocol_native.html)
and [PulseAudio server selection](https://wiki.freedesktop.org/www/Software/PulseAudio/FAQ/).

Accessibility test buses must inherit the test's private `XDG_RUNTIME_DIR`, not
the host runtime. Otherwise D-Bus activation can replace the host AT-SPI socket.
During enablement, the existing host `at-spi-dbus-bus.service` was restarted to
repair such a stale socket; the test fixtures now isolate bus activation as well
as Wayland. This was a transient service restart, with no configuration change.

### Optional Native Pinch Testing

The native pinch acceptance script creates temporary touchpads and touchscreens
through Linux uinput, so gestures and raw touch frames pass through udev and
libinput before reaching Hearthspace.
Install its optional test dependencies (installed on the development host):

```sh
sudo apt-get install -y python3-evdev libinput-tools
```

Ubuntu also installs `python3-libevdev` and `python3-pyudev` as dependencies. The
test uses noninteractive sudo to access `/dev/uinput`; it does not change device
permissions, install udev rules, or leave a virtual device behind. The injector
requires its owned native session's VT to be active and closes the device before
the native harness restores the original VT. Build the native binaries first,
then run this separately from other native or GPU tests:

```sh
cargo build --locked --workspace --all-targets --features udev,e2e,test-apps
scripts/pinch-smoke --session "$(mktemp -d /tmp/hearthspace-pinch.XXXXXX)"
scripts/pan-smoke --session "$(mktemp -d /tmp/hearthspace-pan.XXXXXX)"
```

The default tests both device types. Add `--device touchpad` or
`--device touchscreen` for a focused run; each run still owns a fresh VT session.

The session artifacts retain libinput's gesture trace, viewport/window assertions,
and PNG/metadata pairs for three-finger zoom in/out, ignored two-finger gestures,
fourth-finger cancellation, and device removal. Touchscreen cases additionally
verify the initial touch chord cannot click the close button, completed gestures
suppress remaining contacts, and single-finger taps/drags still work. Cancellation
restores the initial view. Removal follows the recorded final event: cancellation
or removal of an active gesture restores its initial view; a normal end/lift retains
the completed zoom. The test requires the observed outcome and verifies that each
virtual device disappears and no gesture remains active.

The pan test checks left/right/up/down and diagonal translation, simultaneous
pan and zoom, cancellation, and ignored two-finger input on both device types.
It also checks that a pan restores maximized windows, cancellation re-maximizes
them, and fullscreen content stays fixed. Assertions pair viewport and window
geometry with the position and size of a rendered terminal marker. Touchpad
expectations use independently recorded libinput center deltas; touchscreen
expectations use the injected contact centroid. Both scripts accept `--device`
and `--timeout` and clean up their owned session on failure. The pan script also
accepts `--total-timeout` to bound its complete workflow.

### Optional Desktop Dependencies

Native daily-use sessions use the distribution's MATE polkit authentication agent
and GTK desktop portal backend. Install their runtime dependencies with:

```sh
sudo apt-get install -y mate-polkit xdg-desktop-portal xdg-desktop-portal-gtk fuse3
```

GTK window controls also use `gsettings-desktop-schemas` and
`glib-compile-schemas` from `libglib2.0-bin` (both already present on this VM).
The supervisor copies only the installed window-manager schema, its enums, and
applicable defaults into its private runtime and compiles a Hearthspace-specific
default that exposes maximize/restore and close buttons. GTK 3/4 `settings.ini`
fallbacks also live there. Existing user settings and explicit
`GSETTINGS_SCHEMA_DIR` overrides remain authoritative; no host configuration is
written. Missing optional schemas or the compiler produce a warning and leave
the session usable. See [window controls](docs/WINDOW_MANAGEMENT.md#coordinates-and-window-states).

On the physical development host, `mate-polkit` and `mate-polkit-common` version
`1.26.1-6` were installed; the portal packages and FUSE helper were already present.
The earlier `policykit-1-gnome` (`0.105-8build1`) remains installed but is not used:
its authentication dialog calls the X11-only `gdk_x11_get_server_time` on a Wayland
window and crashes. MATE's agent checks the window backend before that call.
No system authentication policy was changed.

Ubuntu 26.04's `notify-send` AppArmor profile permits `Notify` and
`GetServerInformation` but omits capability queries and response signals. This
makes `notify-send --action` report unsupported actions and `--wait` miss closure.
The repository supplies a narrow local protocol extension, installed on this host:

```sh
sudo install -m 0644 scripts/setup/notify-send.apparmor /etc/apparmor.d/local/notify-send
sudo apparmor_parser -r /etc/apparmor.d/notify-send
```

Merge these rules with an existing local override instead of replacing it. The
profile stays enforced; the extension allows only notification capability/close
methods and action/closure signals on their standard session-bus object.

Session locking uses the distribution's `swaylock` client and its packaged PAM
configuration. Install it with `sudo apt-get install -y swaylock`. Version
`1.8.4-1` was installed on the physical development host for this integration;
`/etc/pam.d/swaylock` includes the existing login authentication policy. No PAM
or password configuration was modified. The compositor handles screen isolation
through `ext-session-lock-v1`; the locker handles authentication.

For native audio validation, `pulseaudio-utils` and its `libpulsedsp` dependency
(`1:17.0+dfsg1-2ubuntu4`) were also installed. `pactl info` verifies a launched
application's PulseAudio-compatible connection; `pw-cli info 0` verifies its
PipeWire connection. Installing these client utilities does not start a second
audio server.

The native supervisor starts the notification daemon, permission store, document
portal, GTK portal backend, desktop portal, and polkit agent after Wayland is ready.
The portal processes must acquire their private-bus names before the shell starts. Helpers
are restarted at most twice after an unexpected exit and are stopped with their
session. `--no-desktop-services` skips these helpers for isolated diagnostics;
normal nested sessions also leave them disabled.

Native sessions also coordinate logind lid/suspend and wake recovery. They use
session-owned inhibitors and the packaged swaylock; no `/etc/systemd/logind.conf`,
PAM, or persistent RTC changes are needed. `--no-desktop-services` disables this
coordinator as well. See [power behavior](docs/POWER.md) and the opt-in
[physical suspend test](docs/POWER_TESTING.md), which temporarily owns an otherwise
unused RTC alarm and removes it after testing.

The notification daemon is the workspace binary `hearthspace-notifications`;
include it alongside the session, compositor, shell, and settings daemon when
building or installing a native session. Its application protocol, shell control
API, limits, and tests are documented in [Desktop notifications](docs/NOTIFICATIONS.md).

Hearthspace supplies `hearthspace-portals.conf` in the session's data directory,
selecting the GTK backend according to the standard
[portal configuration lookup](https://flatpak.github.io/xdg-desktop-portal/docs/portals.conf.html).
User portal configuration retains its normal precedence. Private runtime D-Bus
service overrides prevent portals from being activated outside supervision.
Existing portal owners may be adopted only when their executable, private bus,
runtime directory, and process start identity match the owned session. The GTK
chooser uses the local VFS; network filesystem browsing is not configured here.
No shared user service environment or system-wide portal configuration is changed.

Document grants and portal permissions are stored under
`$XDG_DATA_HOME/hearthspace/portals` (defaulting to
`~/.local/share/hearthspace/portals`). Harness runs that isolate app profiles store
this state in their private runtime instead. Shutdown stops the helpers and
detaches the `fuse.portal` mount at the owned runtime's `doc` directory and any
`fuse.gvfsd-fuse` mount at its `gvfs` directory before removing that runtime.
Nautilus can activate the latter through the private session bus. Independent
native recovery performs the same checks after stopping the session scope,
including after supervisor crashes. Other mounted filesystems beneath the runtime
block recursive removal. The host desktop's document and GVfs mounts are preserved.

`HEARTHSPACE_ISOLATE_APP_PROFILES=1`, set by the native harness, also gives portal
helpers and D-Bus-activated applications private XDG configuration, cache, data,
and state directories inside the owned runtime. Both these applications and
isolated compositor launches use the keyfile GSettings backend, so a separately
activated dconf service cannot redirect their writes to ordinary preferences.
The settings daemon and Hearthspace Settings retain their authoritative paths.
Ordinary native sessions leave application configuration and GSettings defaults
unchanged. This isolates XDG-aware app preferences; it is not a filesystem sandbox.

Hearthspace discards inherited ambient, inheritable, effective, and permitted
Linux capabilities before starting the private bus or desktop processes. On this
Ubuntu host, PAM grants `CAP_WAKE_ALARM`; passing it to GTK's Glycin image loader
causes Bubblewrap to fail with `Unexpected capabilities but not setuid`. The
native harness uses `/usr/bin/setpriv` (from `util-linux`) before `dbus-run-session`
for the same reason. Keep the capability bounding set and normal sudo/polkit
authentication intact; no system PAM policy or image-loader sandbox is disabled.

The settings app uses `rfd` for file selection. On Linux, `rfd` is built with
its XDG Desktop Portal backend and falls back to `zenity` if the portal request
fails. Hearthspace gives session children a private `XDG_RUNTIME_DIR` and strips
the host `DBUS_SESSION_BUS_ADDRESS` from shell-launched apps so file pickers do
not accidentally discover GNOME's session bus and open through GNOME's portal on
another TTY. Native daily-use sessions provide the GTK portal backend; nested and
diagnostic sessions may still need the fallback. Install `zenity` for that path:

```sh
sudo apt-get install -y zenity
```

The file chooser portal is an external desktop service. The compositor displays
its dialog as an ordinary Wayland client, and the portal launches URI handlers on
the same private desktop bus.

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

### Native Firefox Package

The physical development host also has Mozilla's official Firefox DEB for
daily browser and portal testing. Snap expects its document portal at the
canonical `/run/user/UID/doc`; Hearthspace's private portal uses a session-specific
runtime. Catalog Snap launches have [owned tracking scopes](docs/SNAP_APPLICATIONS.md),
but that does not resolve the document-portal path difference. The native DEB
provides a browser without that Snap-specific path requirement and retains
Firefox's normal content-process sandbox.

The installation follows [Mozilla's Ubuntu instructions](https://support.mozilla.org/en-US/kb/install-firefox-linux).
The key downloaded from `https://packages.mozilla.org/apt/repo-signing-key.gpg`
was checked in a temporary GPG home before installation. Its primary fingerprint
was `35BAA0B33E9EB396F59CA838C0BA5CE6DC6315A3`. Three new files were installed with
mode `0644`:

- `/etc/apt/keyrings/packages.mozilla.org.asc`: the verified signing key.
- `/etc/apt/sources.list.d/mozilla.sources`:

  ```deb822
  Types: deb
  URIs: https://packages.mozilla.org/apt
  Suites: mozilla
  Components: main
  Signed-By: /etc/apt/keyrings/packages.mozilla.org.asc
  ```

- `/etc/apt/preferences.d/mozilla-firefox`:

  ```text
  Package: firefox firefox-l10n-*
  Pin: origin packages.mozilla.org
  Pin-Priority: 1000

  Package: firefox
  Pin: release o=Ubuntu
  Pin-Priority: -1
  ```

The preference is limited to Firefox and its language packs. APT's normal
repository-signature validation succeeded with the dedicated `Signed-By` key.
After reviewing `apt-get -s install firefox`, the installation used:

```sh
sudo apt-get update
sudo apt-get install -y --allow-downgrades firefox=158.0~build2
```

This replaced only Ubuntu's transitional `firefox` DEB (`1:1snap1-0ubuntu9.1`);
its version epoch accounts for APT's downgrade label. The Firefox Snap
(`149.0.2-1`, revision `8107`) remains installed. `/usr/bin/firefox` now resolves
to `/usr/lib/firefox/firefox`, and `/snap/bin/firefox` still launches Snap.
The corresponding desktop IDs are `firefox.desktop` and
`firefox_firefox.desktop`. Future DEB updates come from the configured Mozilla
repository; the version above records this host's installation.

HTTP, HTTPS, and HTML defaults remained `google-chrome.desktop`, and the automatic
`x-www-browser` selection remained `/usr/bin/google-chrome-stable`. Existing
browser profiles were not opened or migrated. Native acceptance uses a new
explicit `--profile` directory with `--no-remote`, plus temporary desktop entries
removed after the owned session stops; see [application acceptance](docs/DAILY_APPS_TESTING.md).
Preparing these temporary entries also regenerates
`~/.local/share/applications/mimeinfo.cache` with `update-desktop-database`, which
is regenerated again after their removal. Existing `mimeapps.list` files and
browser defaults are preserved.

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

The same feature builds `hearthspace-clipboard-test`, a GTK editor used to verify
copy/paste between Wayland and XWayland. It requires the GTK headers and Xwayland
runtime listed above. See [clipboard regression testing](./docs/CLIPBOARD.md)
for the headless test and native VT workflow.

### Workspace Packages

The root `Cargo.toml` is a virtual workspace. Its default member is `hearthspace-session`, so `cargo run` starts the session supervisor rather than the compositor directly.

Common packages:

```text
crates/hearthspace-session      session supervisor; root cargo run target
crates/hearthspace-compositor   Smithay compositor; binary: hearthspace-compositor
crates/hearthspace-shell        Xilem shell client; binary: hearthspace-shell
crates/hearthspace-settingsd    settings daemon; binary: hearthspace-settingsd
crates/hearthspace-notifications notification daemon; binary: hearthspace-notifications
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

The supervisor starts `hearthspace-settingsd`, `hearthspace-compositor --no-shell`, `hearthspace-shell`, and optionally a settings-app build watcher. Native sessions also supervise notifications, portals, and authentication. It creates per-session sockets under `$XDG_RUNTIME_DIR/hearthspace/session-<pid>/` and passes them to child processes through environment variables.

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

### Compositor and UI Git Dependencies

Smithay is pinned directly to upstream commit
`5c2ba4132556a08abc70ae1771d275ec2cfc031f`, which includes the X11 INCR header
fix needed for large clipboard transfers. No vendored Smithay copy is required.

The shell UI, settings app, UI storybook, and shared UI components are built
with [Xilem](https://github.com/linebender/xilem). We depend on upstream Xilem
from git rather than the latest published crates because crates.io is still
behind APIs Hearthspace uses, including
`xilem::WindowOptionsExtLinux::with_name(general, instance)` for setting
Wayland `app_id` / X11 `WM_CLASS`.
Keep these dependencies pinned to an explicit commit until intentionally
updating Xilem and rerunning the full workspace checks.

The direct Naga 28 and Fontique 0.8 dependencies intentionally match the versions
used by this Xilem revision. They enable required features in the existing
dependency graph; independently moving them to newer major versions would
introduce separate copies and leave the required features disabled.

```toml
masonry = { git = "https://github.com/linebender/xilem", rev = "271a27a6d4a930f7878d404f9014e3c50a3a9b88", features = ["testing"] }
xilem = { git = "https://github.com/linebender/xilem", rev = "271a27a6d4a930f7878d404f9014e3c50a3a9b88" }
```

No local Xilem clone is required for normal development; Cargo fetches the git
dependency automatically. A local clone at `~/git/linebender/xilem` is useful
for source search when working through Xilem/Masonry behavior or API questions.

### Linting

Clippy and rustfmt come from the pinned rustup toolchain. Run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --features udev,e2e,test-apps -- -D warnings
```

The development host also has [cargo-deny 0.20.2](https://github.com/EmbarkStudios/cargo-deny/releases/tag/0.20.2)
in `~/.cargo/bin` and [Ruff 0.17.0](https://github.com/astral-sh/ruff/releases/tag/0.17.0)
in `~/.local/bin`. Their official Linux release archives were verified against
the published SHA256 checksum files before installing the binaries. Cargo-deny
replaced 0.19.9; Ruff's previously temporary binary was verified and installed
on PATH. These are user installations, with no apt package changes.

```sh
cargo deny check advisories bans sources --warn unmaintained
ruff check scripts
ruff format --check scripts
```

### Verified Versions

The development VM currently has:

```text
rustc: 1.99.0
cargo: 1.99.0
rustfmt: 1.10.0-stable
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
