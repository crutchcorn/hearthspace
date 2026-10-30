# Development dependencies

These instructions use Ubuntu 26.04 package names. Install only the optional
groups needed for your work. Build commands run as your normal user; `sudo` is
for package installation and the specific native-session operations that need it.

## Rust and native libraries

Install Rust through [rustup](https://rustup.rs/). The repository's
[`rust-toolchain.toml`](../rust-toolchain.toml) selects the toolchain, Clippy,
rustfmt, rust-analyzer, and Rust sources automatically. Check it with
`rustup show` from the repository root; changing your global default isn't needed.

```sh
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config clang libclang-dev \
  libwayland-dev wayland-protocols wayland-utils libinput-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libudev-dev libseat-dev \
  libgbm-dev libegl1-mesa-dev libgles2-mesa-dev libdrm-dev \
  libsystemd-dev libglib2.0-dev libglib2.0-bin libgtk-4-dev \
  libgtk4-session-lock-dev libgtk4-layer-shell-dev libpam0g-dev \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
  python3 python3-venv dbus xdg-utils fonts-liberation
```

The locker requires GTK 4.12 or newer and the native `gtk4-session-lock` library;
use version 1.3 or newer for its lifecycle fixes. These are separate from the
Rust bindings fetched by Cargo. The GStreamer development libraries support
workspace test targets. `fonts-liberation` supplies the shell's intended font.
The pkg-config name for xkbcommon is `xkbcommon`, not `libxkbcommon`.

The compositor and shell need working graphics drivers. For software-rendered
headless tests, install `libgl1-mesa-dri` and `mesa-vulkan-drivers`, then follow
[the E2E environment setup](E2E_TESTING.md).

## Desktop runtime

A full native session uses logind/libseat and these runtime packages:

```sh
sudo apt-get install -y \
  mate-polkit xdg-desktop-portal xdg-desktop-portal-gtk fuse3 \
  gsettings-desktop-schemas pulseaudio-utils upower xwayland foot zenity
```

- MATE polkit supplies a Wayland-capable authentication agent. Don't substitute
  an agent that assumes X11 window handles.
- The GTK portal backend provides file dialogs and related desktop integration.
  `zenity` provides the explicit screen/window sharing chooser and the optional
  file-dialog fallback. Screen sharing also requires portal-wlr 0.8.4 with the
  Hearthspace backend patches and PipeWire; follow
  [screen-sharing setup](SCREEN_SHARING.md#runtime-requirements) to build and
  install the pinned backend.
- `pactl` from `pulseaudio-utils` must support JSON output (version 16 or newer).
  It works with PulseAudio or PipeWire's PulseAudio service; no second audio
  server is needed. See [sound and brightness](AUDIO_AND_BRIGHTNESS.md).
- UPower supplies battery/UPS status. Systems without a battery need no fake
  indicator. Xwayland enables X11 applications.
- Super+Return launches `foot` directly, so the terminal shortcut requires it.
  The same terminal is also used by integration tests.
- Installed GSettings schemas and `glib-compile-schemas` let the supervisor
  prepare private GTK window-control defaults. Existing user overrides remain
  authoritative; no host dconf settings are rewritten.

The [installer](LOGIN_SESSION.md) provisions the dedicated `hearthspace-lock`
PAM service through a checked transaction and preserves custom policies. A
source-tree native run also needs that service. Use the installer rather than
copying an unchecked PAM file. Missing or unreadable policy makes locking
unavailable; never add an authentication bypass for development.

Shared permission/document and GTK/frontend portals remain user-manager services.
The screen-sharing backend is session-owned in both modes. Private test sessions
own their separate helpers and profiles. Read [application integration](APPS.md)
before diagnosing bus, audio, or portal routing by changing global environment
variables. Hearthspace doesn't require persistent logind configuration, new
backlight permissions, or disabling application sandboxes.

## Test and recording tools

```sh
sudo apt-get install -y \
  at-spi2-core libayatana-appindicator3-1 libgtk-3-0t64 \
  libinput-tools python3-evdev ffmpeg
```

AT-SPI and AppIndicator support are needed for the corresponding required
integration tests. `python3-evdev` and `libinput-tools` are for native input
fixtures; these use temporary owned
uinput devices, without persistent udev rules. FFmpeg supplies `ffmpeg` and
`ffprobe` for [recordings](NATIVE_RECORDING.md).

For optional [GTK touch and titlebar acceptance](GTK_TOUCH_TESTING.md), also
install:

```sh
sudo apt-get install -y python3-gi gir1.2-gtk-3.0 gnome-calculator
```

The fixtures use Python's PyGObject bindings and GTK 3 introspection data; the
touch workflow also exercises GNOME Calculator. The titlebar matrix needs GTK 4
introspection data (`gir1.2-gtk-4.0`), provided by the `libgtk-4-dev` dependency
above.

Native automation additionally requires systemd/logind, login PAM, `util-linux`,
a spare VT on `seat0`, and authorized noninteractive sudo. The agent must stay
outside the desktop under test. Don't make a blanket sudoers or lingering change
as part of setup; have the machine's administrator choose an appropriate access
policy. See [native testing](NATIVE_TESTING.md).

Use Ruff 0.17.0, matching CI, in a virtual environment outside the repository or
a tool runner:

```sh
ruff_env="${XDG_DATA_HOME:-$HOME/.local/share}/hearthspace-dev/ruff-venv"
python3 -m venv "$ruff_env"
"$ruff_env/bin/pip" install ruff==0.17.0
"$ruff_env/bin/ruff" check scripts
"$ruff_env/bin/ruff" format --check scripts
```

Use that executable in place of `ruff` in the contributor checks unless it's on
PATH. Install `cargo-deny` through its
[upstream instructions](https://github.com/EmbarkStudios/cargo-deny#installation).
Neither tool needs a system Python override or root-owned Cargo files.

## Optional integrations

- **Settings development watcher:** put either `cargo-watch` or `watchexec` on
  PATH to rebuild Settings as you edit. The supervisor prefers `cargo-watch` when
  both are available. Use `--no-watch-apps` to disable it; neither tool is needed
  for an ordinary desktop session.
- **Remote desktop:** WayVNC 0.9 or newer, with the capture protocols used by
  Hearthspace. Follow [remote desktop](REMOTE_DESKTOP.md) for localhost binding,
  authentication, and SSH transport.
- **Local AI:** an already-installed Ollama service supporting the D1
  `POST /v1/systemone` endpoint. Ordinary `/api/chat` support isn't sufficient.
  Hearthspace's opt-in manages its model, not the host service. See
  [window groups](WINDOW_GROUPS.md).
- **Application acceptance:** install the actual apps you intend to test. Use
  private profiles and the [daily-app workflow](DAILY_APPS_TESTING.md). Browser
  package choices and Snap constraints belong to [app compatibility](APPS.md)
  and [Snap applications](SNAP_APPLICATIONS.md), not a required package downgrade.
- **Cursor editing:** install `x11-apps` for `xcursorgen` and ImageMagick; follow
  [cursor assets](CURSOR.md).

### Confined notification tools

A distribution's `notify-send` AppArmor policy may omit notification capability
queries or action/closure signals. If denials establish that problem, inspect
[`scripts/setup/notify-send.apparmor`](../scripts/setup/notify-send.apparmor).
Merge its narrow rules into `/etc/apparmor.d/local/notify-send`, preserving any
existing local rules, then reload the owning profile:

```sh
sudo apparmor_parser -r /etc/apparmor.d/notify-send
```

The profile remains enforced. To undo the addition, remove only the added rules
and reload it again. Don't replace an administrator's local override or disable
AppArmor to make a notification test pass. See [notifications](NOTIFICATIONS.md).
