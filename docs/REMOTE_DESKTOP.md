# Remote desktop for development

Hearthspace can share its physical native desktop through an opt-in WayVNC
development session. Remote keyboard, pointer buttons, movement and scrolling
use the compositor's regular input
handling, including window focus, dragging, and desktop shortcuts.

## Start and connect

Install [the optional dependencies](DEPENDENCIES.md) on the Hearthspace machine.
WayVNC 0.9 or newer is required for the standard image-copy-capture protocol.
From the repository root on a Linux TTY:

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

For launching from SSH without a foreground Linux TTY, use the owned
[native launcher](NATIVE_TESTING.md) with development sharing enabled:

```sh
remote_run="$(mktemp -d -t hearthspace-remote.XXXXXX)"
scripts/hearthspace-native start --session "$remote_run" --dev --vnc --watchdog 600
scripts/hearthspace-native status --session "$remote_run"
# Always stop the same owned session when finished:
scripts/hearthspace-native stop --session "$remote_run"
```

The watchdog bounds the session if the caller disappears and restores the
original VT through the harness recovery path. Keep the agent and SSH connection
outside the desktop being tested. See [development modes](DEVELOPMENT.md) for
Cargo-managed children and watch behavior.

### macOS client

The following example uses TigerVNC on macOS. Viewer protocol compatibility
matters independently of the tunnel; a generic communication error does not
by itself establish an SSH failure.

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

This fixture starts no shell. For the broader
[headless suite](E2E_TESTING.md#ci-coverage-and-evidence), also select Lavapipe for
the Vulkan shell and keep GTK fixtures on shared-memory buffers with
`GSK_RENDERER=cairo`.

For native acceptance, use an owned session and verify each boundary:

1. Open Foot through remote keyboard input and assert an exact typed marker.
2. Drag its window through remote pointer input and compare logical geometry.
3. Compare the VNC image, including its cursor, with a fresh native capture.
4. Lock locally and verify that new capture and remote input are denied. A cached
   pre-lock image in the viewer is not a new capture or an unlock path.
5. Stop the owned session and require the listener, processes and runtime to be
   gone, with the original VT restored.

Retain RFB assertions, unlocked captures, binary provenance and cleanup records
with the run artifacts. [Tested environments](TESTED_ENVIRONMENTS.md) centralizes
environment coverage. A headless RFB pass does not establish physical output
presentation, lock recovery or connectivity from another machine.
