# Remote Development Desktop

Hearthspace can share its physical native desktop through WayVNC. Remote keyboard,
pointer buttons, movement, and scrolling use the compositor's regular input
handling, including window focus, dragging, and desktop shortcuts.

## Start and connect

Install [the optional dependencies](../CONTRIBUTING.md#remote-development-with-wayvnc)
on the Hearthspace machine. WayVNC 0.9 or newer is required for the standard
image-copy-capture protocol (Ubuntu 26.04 supplies 0.9.1). From a Linux TTY:

```sh
cargo run -- --dev --no-watch-apps --vnc
```

`--vnc` requires `--dev` and the native backend. `--vnc-port 5901` selects a
different port. The supervisor starts WayVNC after the shell is ready, records
its PID in the session's `processes.json`, and stops it with the desktop.
WayVNC uses shared-memory capture with the cursor included and preserves the
physical output resolution.

On the other machine, keep this SSH tunnel running:

```sh
ssh -N -o ExitOnForwardFailure=yes -L 5900:127.0.0.1:5900 USER@HEARTHSPACE_HOST
```

Connect your VNC viewer to `127.0.0.1:5900`. TigerVNC uses
`vncviewer 127.0.0.1::5900` for an explicit port.
Authentication and encryption come from SSH; WayVNC
listens only on the Hearthspace machine's IPv4 loopback interface. Its launch
uses an explicit empty configuration, so personal WayVNC settings cannot change
the listening address or authentication policy.

The development host's current addresses are `10.0.0.179` on the local network
and `100.90.91.81` on Tailscale, with user `crutchcorn`. Check `ip -br addr`
if an address changes. A tunnel command for this host is:

```sh
ssh -N -o ExitOnForwardFailure=yes -L 5900:127.0.0.1:5900 crutchcorn@100.90.91.81
```

For launching from SSH without a foreground Linux TTY, use the native launcher
example in CONTRIBUTING. Its watchdog restores the original VT if the session
is abandoned; normal cleanup is the launcher's `stop` command.

### macOS client

Use TigerVNC on macOS. Apple's built-in Screen Sharing has a
[documented RFB version compatibility problem](https://github.com/any1/neatvnc/issues/48)
with the WayVNC/NeatVNC 0.9.1 setup used here. Its generic communication error
can occur even when the SSH tunnel works; `open vnc://...` launches that client.

Install [TigerVNC with Homebrew](https://formulae.brew.sh/cask/tigervnc):

```sh
brew install --cask tigervnc
open -a TigerVNC
```

Keep this tunnel open in a separate Terminal tab. Local port 15900 avoids
conflicting with a VNC server already running on the Mac:

```sh
ssh -N -o ExitOnForwardFailure=yes -L 15900:127.0.0.1:5900 USER@HEARTHSPACE_HOST
```

Enter `127.0.0.1::15900` in TigerVNC's server field; the double colon specifies
the TCP port. No separate VNC password is configured. To check the tunnel,
`nc -w 3 127.0.0.1 15900` should print `RFB 003.008`.

## Protocols and limits

The compositor's explicit `--remote-desktop` flag enables Smithay's standard
protocol implementations:

- `ext-output-image-capture-source-v1` and `ext-image-copy-capture-v1` for capture.
- `zwp-virtual-keyboard-v1` and `zwlr-virtual-pointer-v1` for remote input.

The session supervisor adds that flag only for `--vnc`. Ordinary compositor
sessions do not expose these capture/input globals. The session runtime and
Wayland socket remain private to the desktop user; locally connected remote
desktop clients are trusted to view and control this session.

Native captured frames become available after their KMS page flip. Capture and
remote input are unavailable while the session is locked or its VT is inactive.
WayVNC can continue displaying its cached last frame during that pause. The
remote viewer cannot unlock the physical session. Use local
authentication or stop an owned development session through the native launcher.
Clipboard synchronization and resizing the physical monitor from a viewer are
not part of this integration.

The [Wayland protocol catalog](https://wayland.app/protocols/) documents the
interfaces; [WayVNC's documentation](https://github.com/any1/wayvnc/tree/v0.9.1)
describes its viewer and transport behavior.

## Validation

The real WayVNC headless regression negotiates RFB, reads desktop pixels, types
an exact marker into Foot, drags its window at scale 2, and verifies that a
disconnected client's held button does not continue dragging:

```sh
LIBGL_ALWAYS_SOFTWARE=1 GSK_RENDERER=cairo HEARTHSPACE_E2E=1 \
  python3 -m unittest discover -s scripts/tests -p 'test_wayvnc_headless.py' -v
```

It requires the built compositor, Foot, and WayVNC. Set
`HEARTHSPACE_VNC_EVIDENCE` to an absolute directory to retain logs and the text
received by the terminal. The E2E workflow installs WayVNC and runs this test
with the other headless regressions. Native presentation validation follows
[the native testing workflow](./NATIVE_TESTING.md).

The software-rendering variables match the full
[headless suite](./E2E_TESTING.md#ci-coverage-and-evidence); `GSK_RENDERER=cairo`
keeps GTK fixtures on shared-memory buffers when running that broader suite.

Native acceptance on the physical GPD host (AMD Strix Radeon 880M/890M,
`amdgpu`, Linux `7.0.0-38-generic`) used `eDP-1` at 2560×1600, scale 2.
RFB keyboard input opened Foot with Super+Return and wrote the exact text
`WayVNC native input 123!`; an RFB pointer drag moved it by 40×30 logical pixels.
The VNC image included the hardware cursor and agreed with a page-flip-confirmed
native capture. Locking denied further capture and input while WayVNC retained
its last frame. Stopping the owned session removed its runtime and listener,
restored `tty2`, and left the background T3 service active. Artifacts, screenshots,
binary provenance, and cleanup records are in
`/tmp/hearthspace-wayvnc-native.BK4Kpv/` on that host.
