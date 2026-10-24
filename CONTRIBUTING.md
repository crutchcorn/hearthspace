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
sudo apt-get install -y build-essential pkg-config clang libclang-dev libwayland-dev wayland-protocols wayland-utils libinput-dev libxkbcommon-dev libxkbcommon-x11-dev libudev-dev libseat-dev libgbm-dev libegl1-mesa-dev libgles2-mesa-dev libdrm-dev libsystemd-dev libglib2.0-dev libglib2.0-bin libgtk-4-dev libgtk4-session-lock-dev libgtk4-layer-shell-dev libpam0g-dev xdg-utils
```

The lock client requires GTK4 **4.12 or newer**, the native gtk4-session-lock **1.3** library, and
PAM development headers. The lock-screen integration installed
`libgtk4-session-lock-dev`, `libgtk4-layer-shell-dev`, and `libpam0g-dev` on the
development host; GTK4 was already available. `gtk4-session-lock` **0.4.1** uses
these native libraries alongside the Rust GTK4 **0.11.5** bindings. These are
required desktop dependencies, including for builds without `test-apps`.

Desktop file menus use GIO for application associations and Trash.
`libglib2.0-dev` supplies its build interfaces; `libglib2.0-bin` supplies
`gio open` for default applications and `gio launch` for Open With. Properties
uses the session file manager’s `org.freedesktop.FileManager1.ShowItemProperties`
interface (provided by Nautilus, for example).

Settings and server-decorated titlebars use Liberation Sans to match the browser
reference's text metrics.
Install it with `sudo apt-get install -y fonts-liberation` if it is not already
present. Fontique discovers it through the system font collection.

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

### Graphical Login Installation

After installing the build dependencies and the native desktop runtime packages
below, run the installer as your normal user:

```sh
scripts/install-session install
```

It checks runtime dependencies, builds the native release without sudo, then
elevates only the installation step. It installs into `/usr/local`:

- `bin/hearthspace-login`: the graphical-session entry point.
- `share/wayland-sessions/hearthspace.desktop`: the GDM session chooser entry.
- `lib/hearthspace/releases/CONTENT_HASH/`: a complete runtime containing the
  session supervisor, compositor, shell, lock client, Settings, settings daemon, notifications,
  accessibility CLI, app-scope guard, Python login guard, Settings desktop entry,
  and icon. Wallpaper and cursors are embedded in the compositor.
- `lib/hearthspace/current`: an atomically updated relative symlink to the
  selected release; `install.json` records the exact owned files and hashes.

The installer also manages one fixed system file outside the prefix:
`/etc/pam.d/hearthspace-lock`. If absent, it installs the policy from
`data/pam.d/hearthspace-lock` as root:root, mode `0644`. Authentication and account
checks include Ubuntu's existing `login` stacks; no password or session rules
are added. Creation is rolled back if release activation fails. `install.json`
records the policy hash separately, so uninstall removes only the verified
policy created by that installation. A policy already owned by another prefix
is rejected instead of shared across independent uninstall lifetimes.

An existing regular root:root `0644` administrator policy is preserved, even
when identical to the supplied policy, and is never removed by the installer.
Symlinks, unsafe ownership/modes, or changes to a previously managed policy
stop installation or removal for administrator review. The installer does not
overwrite custom PAM rules. Staged installations put this file under
`DESTDIR/etc/pam.d/hearthspace-lock`, with the staging user's ownership; a system
package must install it as root:root. `--prefix` never changes the runtime PAM path.

During initial lock-screen development, the previously absent policy was
provisioned from `data/pam.d/hearthspace-lock` using `sudo install -o root -g root
-m 0644 data/pam.d/hearthspace-lock /etc/pam.d/hearthspace-lock`. Existing PAM
files were unchanged. That manually provisioned file is treated as an
administrator policy by the installer; new deployments should use the
transactional installer above.

GDM on the development host discovers `/usr/local/share/wayland-sessions`.
The existing login/GDM PAM stacks, system-wide default session, and automatic-login
settings are unchanged. Existing Ubuntu/GNOME remains available.
The launcher resolves one release before startup, so updates do not mix binaries
in an existing session. Older releases remain installed until uninstall.

The login guard runs as a transient `hearthspace-login-PID-START.service` in the
existing systemd user manager. It records the supervisor's UID, PID/start time,
exact logind session scope, leader, seat, and Hearthspace desktop identity.
When the supervisor exits or crashes, it asks logind to terminate that recorded
session and removes only the verified private runtime after the scope stops.
logind authorizes the owning user; this requires no sudo or authentication-policy
override. Background user services, including T3, are outside that login scope.
Python 3, `systemd-run`, `systemctl`, `loginctl`, and `fusermount3` are runtime
dependencies of this cleanup path. Startup fails if the guard cannot become ready.

Re-run the same installer to update. For an already verified build, use
`scripts/install-session install --no-build --binary-dir /absolute/build/bin`.
`--destdir /absolute/staging` stages an installation without sudo for packaging
tests; `--prefix` changes the final prefix. Do not run Cargo through sudo.

Log out of Hearthspace before uninstalling from another desktop:

```sh
scripts/install-session uninstall
```

Removal verifies the ownership manifest and refuses modified tracked files.
It preserves untracked files, user settings, application data, other desktops,
and administrator-managed policy.
See [graphical login](docs/LOGIN_SESSION.md) for operation and acceptance details.

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
hook. The native harness installs no persistent service units or PAM configuration;
the lock client's policy is provisioned separately by the session installer. Stop a run
through the harness to restore its original VT and remove the private runtime;
recovery records under `/run` disappear on reboot. User-selected artifact
directories retain logs and screenshots until explicitly removed.

#### Remote development with WayVNC

WayVNC 0.9 or newer is required for image-copy-capture support. On Ubuntu 26.04,
install the optional VNC server on the Hearthspace machine:

```sh
sudo apt-get install -y --no-install-recommends wayvnc
```

The physical development host uses Ubuntu's WayVNC **0.9.1-1build1**, with its
`libaml0t64`, `libneatvnc0`, and `libturbojpeg0` dependencies. SSH was already
enabled on this host; on another host, install `openssh-server` and enable
`ssh` if needed. The setup requires no firewall rule for VNC, persistent VNC
service, or system authentication changes.

On 2026-10-21, the same `apt-get` command installed WayVNC `0.9.1-1build1`
and its runtime dependencies on the ThinkPad T480s so the full headless E2E
suite could exercise VNC capture and input. No persistent VNC service was enabled.

Start a native session from a Linux TTY with:

```sh
cargo run -- --dev --no-watch-apps --vnc
```

From a background service or SSH connection, use the owned native launcher:

```sh
remote_run=$(mktemp -d /tmp/hearthspace-remote.XXXXXX)
scripts/hearthspace-agent --session "$remote_run" --timeout 300 start --dev --vnc --watchdog 28800
```

This builds before taking a spare VT and runs the development session for at
most eight hours. Retain the printed session path. Stop it with
`scripts/hearthspace-agent --session "$remote_run" --timeout 30 stop` to restore
the original desktop. See [remote desktop](./docs/REMOTE_DESKTOP.md) for
connection instructions, protocol support, and validation.

#### Native screen recordings

The native recorder uses FFmpeg to encode captured compositor frames into an
H.264 MP4. Install its optional host dependency (including `ffprobe`) with:

```sh
sudo apt-get install -y --no-install-recommends ffmpeg
```

The physical development host uses Ubuntu's FFmpeg **8.0.1**. See
[recording a native demo](./docs/NATIVE_RECORDING.md) for capture, pacing, and
cleanup. This dependency is only needed for recording, not desktop operation.

For GitHub preview checks in T3 Code on Ubuntu hosts whose AppArmor policy
blocks Chrome's sandbox, the one-time host setup is:

```sh
sudo env "PATH=$PATH" t3 browser setup
```

This setup was applied on the development host to allow T3's sandboxed browser;
it does not change Hearthspace's runtime or browser configuration.

Ordinary native/TTY and display-manager sessions use the standard user D-Bus at
`$HOST_RUNTIME/bus`. This reuses the login keyring that PAM unlocked; Hearthspace
does not start another keyring, handle passwords, or rewrite encrypted stores.
After installing an update, log out and select Hearthspace again. Passwordless
login or a keyring password different from the login password may still require
an unlock; this change does not bypass those protections.

The native harness explicitly supplies `--private-session-bus` and wraps the
supervisor in `dbus-run-session`. That isolated mode retains the sibling
`hearthspace-session-bus` helper's abstract Unix socket for confined apps. Use
this explicit mode when running alongside another desktop. Ordinary shared-bus
mode assumes one graphical desktop for the user; it refreshes that user's GTK
and frontend portal services for Hearthspace's display.

The runtime package, binary, and native-feature inventory lives in
`scripts/runtime-artifacts.json`. The installer and native harness derive their
build commands from it; development native runs prepare each package separately,
including session helper binaries, before acquiring a VT. The `--dev` supervisor
also asks Cargo to prepare its scope guard before startup, then uses the executable
path in Cargo's JSON artifact response. It never accepts an old sibling as a
fallback after a failed build. `scripts/dev-run-app` uses the same artifact
resolution, including `CARGO_TARGET_DIR` and Cargo configuration. Installed
sessions still use their immutable release's sibling binaries and need no checkout
or Cargo. Both bus modes supply
`DBUS_SESSION_BUS_ADDRESS` and `HEARTHSPACE_SESSION_BUS_ADDRESS` consistently.
Owned components keep their private runtime and sockets. Once the compositor is
ready, shared activation imports a bounded allowlist into D-Bus and the user
manager, using the canonical runtime and an absolute Wayland socket path.
Private activation never imports into the user manager.

Shared activation records its prior allowlisted values and a generation marker.
Normal shutdown and the independent cleanup guards retire owned values before
removing the runtime; newer session values are preserved. Inherited display and
accessibility overrides are removed from owned children. `WAYLAND_SOCKET` and
`AT_SPI_BUS_ADDRESS` are removed from the manager rather than published empty.
D-Bus has no unset operation, so earlier external imports of these variables
cannot be cleared through its activation API. Hearthspace does not restart the
user's bus to remove them. Nested sessions do not modify the user's
activation environment unless explicitly given their own private bus.

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

Ubuntu also installs `python3-libevdev` and `python3-pyudev` as dependencies.
The 2026-10-22 icon/switcher validation installed `python3-evdev` 1.9.3-1build1 and
`libinput-tools` 1.31.1-1ubuntu1.2 with those dependencies on the development host.
The test uses noninteractive sudo to access `/dev/uinput`; it does not change device
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
It also checks that maximized windows keep their world bounds during pan and
zoom, cancellation restores the view, and fullscreen content stays fixed.
Assertions pair viewport and window geometry with the position and size of a
rendered terminal marker. Touchpad
expectations use independently recorded libinput center deltas; touchscreen
expectations use the injected contact centroid. Both scripts accept `--device`
and `--timeout` and clean up their owned session on failure. The pan script also
accepts `--total-timeout` to bound its complete workflow.

Window-switcher acceptance uses those same installed dependencies and
temporary uinput permissions:

```sh
scripts/window-switcher-smoke --session "$(mktemp -d /tmp/hearthspace-window-switcher.XXXXXX)"
```

It checks Alt+Tab/Alt+Shift+Tab, left/right four-contact touchpad and four/five-contact
touchscreen swipes, live previews over the animated canvas, cancellation, input
ownership, and keyboard unplug recovery. Pinch cases use four touchpad contacts
and four/five touchscreen contacts: successful inward pinches keep the switcher
open after release, outward pinches dismiss it, and subthreshold inward pinches
leave it closed. Four-contact cases also test cancellation; subthreshold outward
pinches preserve the open switcher. They check that a keyboard-owned switcher is
unaffected and a repeated inward pinch does not cycle the selection.
In focused view, inward pinch opens the switcher, outward dismissal retains the
focused app, and reopening followed by Enter leaves that app bounded on the canvas.
The horizontal swipe cases also verify that
canvas panning stays blocked, cancelled swipes retain focus, and a completed
switch leaves the previous app maximized on the canvas. Its bounded keyboard
worker records libinput key events only from its own temporary device. No extra
packages, udev rules, or persistent input devices are required.
Libinput suppresses five-contact touchpad gestures; touchscreen coverage includes
both four and five contacts. Keep physical input devices idle during acceptance.

Preview clicks/taps and Escape/background dismissal in a persistent switcher need
additional acceptance. Follow the [persistent switcher acceptance steps](docs/NATIVE_TESTING.md#persistent-switcher-acceptance)
for these interactions and a separate Enter activation check.

### Recovering a black host login screen

If SSH still works, inspect the active session before assuming the laptop is
suspended. `cat /sys/class/tty/tty0/active` and `loginctl list-sessions` identify
the active VT and existing desktop sessions. Check the native harness manifest
first; recover an active harness through its own `stop` command.

On the physical development host, GDM once remained black on tty1 with Mutter's
`PowerSaveMode` set to `3` and resume/device-reclaim warnings in its journal.
The existing GNOME session on tty2 remained alive and locked. Turning the active
greeter's display power back on and running `sudo loginctl activate 2` restored
display and input without restarting GDM or closing applications. Session IDs
are dynamic: select the existing desktop's ID from `loginctl`, not the literal
`2` on another boot. Activation does not unlock the desktop.

For GNOME, the owning session bus exposes `PowerSaveMode` on
`org.gnome.Mutter.DisplayConfig`, object `/org/gnome/Mutter/DisplayConfig`,
interface `org.gnome.Mutter.DisplayConfig`; setting its `i` property to `0`
requests display-on. Use the actual session owner's bus and runtime directory.
This is a transient recovery, not a power-policy or authentication change.
The observed incident did not establish why GDM lost input. Native cleanup had
finished seven minutes earlier, with no later harness cleanup activity.

### Optional Desktop Dependencies

Battery status and low/critical warnings use the system UPower service. Ubuntu
Desktop normally includes it; on a minimal installation, install it with
`sudo apt-get install -y upower`. It remains optional: systems without UPower or
a system battery or UPS show no battery indicator. Suspend uses the native session's
existing logind integration and does not require UPower. Hearthspace does not
modify UPower thresholds, logind/PAM policy, or the host's critical-power action.
See [power controls](docs/POWER.md).

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

Session locking uses the workspace's `hearthspace-lock` client. The installer
builds and installs it beside the compositor and provisions the fixed PAM policy
as described under [graphical installation](#graphical-login-installation).
Source-tree native runs also need that system policy; after building the complete
native set, `scripts/install-session install --no-build --binary-dir "$PWD/target/debug"`
can provision it through the same checked installation transaction. Do not
replace a customized policy with an unchecked copy. Swaylock is no longer a
runtime dependency; an existing swaylock package and its PAM policy are left
untouched. The compositor owns screen isolation through `ext-session-lock-v1`;
the lock client owns authentication. Real-account unlock acceptance uses only a
local user's physical input, never an agent-supplied password.

For native audio validation, `pulseaudio-utils` and its `libpulsedsp` dependency
(`1:17.0+dfsg1-2ubuntu4`) were also installed. `pactl info` verifies a launched
application's PulseAudio-compatible connection; `pw-cli info 0` verifies its
PipeWire connection. Installing these client utilities does not start a second
audio server.

The shell's volume control also uses `pactl` from `pulseaudio-utils` (version 16
or newer for JSON output). Install it with `sudo apt-get install -y pulseaudio-utils`
if it is missing. PipeWire's PulseAudio service and PulseAudio are both supported;
missing utilities, servers, or outputs leave the control unavailable. Brightness
uses kernel backlight readings and logind's existing active-session permissions,
without new groups, udev rules, setuid helpers, or password prompts. See
[audio and brightness](docs/AUDIO_AND_BRIGHTNESS.md).

After Wayland is ready, the native supervisor starts its notification daemon and
polkit agent and waits for the selected bus's portals before starting the shell.
Normal sessions use the user-managed portal lifecycle described below. Explicit
private sessions supervise their portal helpers, restarting them at most twice
after an unexpected exit and stopping them with the session.
`--no-desktop-services` skips this integration for isolated diagnostics; normal
nested sessions also leave it disabled.

Native sessions also coordinate logind lid/suspend and wake recovery. They use
session-owned inhibitors and `hearthspace-lock` with the policy above; no
`/etc/systemd/logind.conf` or persistent RTC changes are needed.
`--no-desktop-services` disables this coordinator as well. See [power behavior](docs/POWER.md) and the opt-in
[physical suspend test](docs/POWER_TESTING.md), which temporarily owns an otherwise
unused RTC alarm and removes it after testing.

The notification daemon is the workspace binary `hearthspace-notifications`;
include it alongside the session, compositor, shell, and settings daemon when
building or installing a native session. Its application protocol, shell control
API, limits, and tests are documented in [Desktop notifications](docs/NOTIFICATIONS.md).

Hearthspace supplies `hearthspace-portals.conf` and its Settings backend metadata
in the session's data directory. The supervised appearance worker supplies live
color-mode/accent preferences; unrelated Settings keys and other portal
interfaces use the GTK backend according to the standard
[portal configuration lookup](https://flatpak.github.io/xdg-desktop-portal/docs/portals.conf.html).
User portal configuration retains its normal precedence. Shared sessions borrow
the standard permission store and document portal, starting their user units only
when missing. After activation import and appearance publication, they restart
only `xdg-desktop-portal-gtk.service` and `xdg-desktop-portal.service`, in that
order. These services belong to the user manager and are never adopted or killed
by Hearthspace cleanup. The notification daemon, appearance backend, and polkit
agent remain Hearthspace-owned; name collisions fail rather than replacing
another desktop's notification service.

Explicit private sessions retain supervised portal processes and runtime D-Bus
activation overrides. Existing owners may be adopted only when their executable,
private bus, runtime, and process start identity match. Their GTK chooser uses
the local VFS; network filesystem browsing is not configured for that mode.
No system-wide portal configuration is changed.
Native app environments opt GTK 3 into these portal settings with
`GTK_USE_PORTAL=1`; the GTK portal backend opts out to avoid recursive lookup.
No host dconf preferences are written or restored. See
[Linux appearance publication](docs/SETTINGS.md#linux-appearance-publication)
for the compatibility boundary and the private-bus integration test.

Shared sessions use the standard user document mount and permission database.
In explicit private sessions, document grants and portal permissions are stored
under
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
unchanged. Shared-bus applications activated by the user manager follow its normal
lifetime; they are not all captured by Hearthspace's process groups. Explicit
Snap and Ptyxis scopes remain session-owned. Profile isolation applies to the
private harness and is not a filesystem sandbox.

Hearthspace discards inherited ambient, inheritable, effective, and permitted
Linux capabilities before starting the private bus or desktop processes. On this
Ubuntu host, PAM grants `CAP_WAKE_ALARM`; passing it to GTK's Glycin image loader
causes Bubblewrap to fail with `Unexpected capabilities but not setuid`. The
native harness uses `/usr/bin/setpriv` (from `util-linux`) before `dbus-run-session`
for the same reason. Keep the capability bounding set and normal sudo/polkit
authentication intact; no system PAM policy or image-loader sandbox is disabled.

The settings app uses `rfd` for file selection. On Linux, `rfd` is built with
its XDG Desktop Portal backend and falls back to `zenity` if the portal request
fails. Hearthspace gives owned components a private `XDG_RUNTIME_DIR` and passes
the selected bus explicitly to launched apps. Native daily-use sessions refresh
the standard GTK portal backend for their display. Isolated harness sessions use
their own portal; nested and diagnostic sessions may still need the fallback.
Install `zenity` for that path:

```sh
sudo apt-get install -y zenity
```

The file chooser portal is an external desktop service. The compositor displays
its dialog as an ordinary Wayland client, and the portal launches URI handlers on
the selected desktop bus.

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

### Native Steam Touch Testing

The 2026-10-23 CSD titlebar investigation enabled Ubuntu's `i386` architecture
and installed `steam-installer` 1:1.0.0.85~ds-2build1 with its 32-bit runtime and
Mesa dependencies. Steam's client downloads on first launch. This setup is
optional; deterministic XWayland regression tests do not require Steam.

```sh
sudo dpkg --add-architecture i386
sudo apt-get update
sudo apt-get install -y steam-installer
```

Launch Steam only inside an owned native test session when testing its CSD
header. Keep its stderr and session cleanup evidence; a responsive compositor
alone does not establish that Steam recovered after a gesture.

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

For an isolated CSD acceptance run on a host with only the Snap launcher,
Mozilla's [Linux tarball](https://support.mozilla.org/en-US/kb/install-firefox-linux#w_install-firefox-from-mozilla-tarball-builds)
can be extracted into the run's artifact directory. Pass its absolute `firefox`
executable to `scripts/firefox-smoke --firefox PATH`; the harness creates a new
profile. The CSD recovery run prepared Firefox **158.0.1**, checking its
`linux-x86_64/en-US/firefox-158.0.1.tar.xz` against Mozilla's release
`SHA512SUMS` before extraction. This requires no package repository, desktop
registration, default-browser change, or system installation. Retain the archive
URL and checksum with the native evidence; preparation alone is not a passing test.

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
crates/hearthspace-lock         GTK4 session-lock client with fixed PAM authentication
crates/hearthspace-settingsd    settings daemon; binary: hearthspace-settingsd
crates/hearthspace-notifications notification daemon; binary: hearthspace-notifications
crates/hearthspace-settings     settings app; binary: hearthspace-settings
crates/hearthspace-ui-storybook UI component gallery app; binary: hearthspace-ui-storybook
crates/ui-components            shared Hearthspace Xilem UI components/theme
crates/xilem-storybook          generic Storybook-like state model for Xilem apps
crates/hearthspace-ipc          typed settings/control contracts and bounded clients
crates/hearthspace-desktop      catalogs, favorites, artwork, and app/file launch services
crates/hearthspace-wayland-client coherent live output observation without UI dependencies
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

Two small runtime packages, `xilem` and `masonry_winit`, are vendored at that
same revision to apply per-window theme properties before the first frame.
The [patch ledger](./patches/README.md#xilem-initial-window-properties) records
the upstream omission, source diff, verification, and removal conditions.

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

### Optional local AI for window groups

Window grouping itself needs no model. **Gather similar windows** uses an
already-installed local Ollama service with the D1 `POST /v1/systemone` API.
The development host has Ollama **0.40.5**; Hearthspace does not install or
reconfigure its service. Older Ollama releases without the decision endpoint
must be updated separately. Ordinary `/api/chat` support is insufficient.

Turn on **Settings → AI → Enable AI features** to opt in. If the model is already
installed, Hearthspace enables it without downloading again. Otherwise, the Rust
`hf-hub` client downloads `LiquidAI/d1-3B-GGUF/d1-3B-Q4_K_M.gguf`, then imports it
as the dedicated Ollama model `hearthspace-d1:latest`. Download files and
preferences live under
`$XDG_CACHE_HOME/hearthspace/ai` (normally `~/.cache/hearthspace/ai`); Ollama also
stores an imported copy in its configured model directory. Turning the switch
off opens a confirmation; **Turn off and delete** removes the managed copy and
download, preserving unrelated models. No systemd, login, firewall, or
privilege-policy changes are required by this feature.

The default endpoint is `http://127.0.0.1:11434`. Set
`HEARTHSPACE_OLLAMA_URL=http://127.0.0.1:PORT` before starting the desktop to use
another local runtime or a mock server. Remote addresses, HTTP proxies and
redirects are rejected for title processing. Automated tests use local mock
servers and require no model download. See [window groups](docs/WINDOW_GROUPS.md)
for interaction, cancellation, and model lifecycle details.

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
CI also runs Ruff 0.17.0 from an isolated Python virtual environment; its runner
installs `python3-venv` rather than modifying the system Python environment.

```sh
cargo deny check advisories bans sources --warn unmaintained
ruff check scripts
ruff format --check scripts
```

On the ThinkPad, Ruff checks can also use the existing `uvx` tool runner:
`uvx ruff==0.17.0 check scripts` and
`uvx ruff==0.17.0 format --check scripts`. This keeps Ruff in uv's tool cache
without installing a system Python package.

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
