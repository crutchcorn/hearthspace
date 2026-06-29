# Native Backend Testing

Evergreen runbook for testing Hearthspace's native `udev`/DRM/KMS backend on a
VT and interpreting the common log output.

## Build And Run

Build the native backend explicitly:

```sh
cargo build --no-default-features --features udev
```

Basic VT smoke without the shell:

```sh
target/debug/hearthspace --tty --no-shell --exit-after-ms 10000 2>&1 | tee /tmp/hearthspace-udev-smoke.log
```

Native shell smoke:

```sh
target/debug/hearthspace --tty --exit-after-ms 15000 2>&1 | tee /tmp/hearthspace-udev-shell.log
```

Client stress repro with retained logs:

```sh
target/debug/hearthspace --tty 2>&1 | tee /tmp/hearthspace-udev-client.log
```

Use `Ctrl+Alt+Backspace` or `Ctrl+Alt+Esc` to exit a live native run.

## Expected Native Startup Markers

Useful successful-start lines:

```text
Hearthspace native backend acquired seat seat0
Selected primary DRM device /dev/dri/card1
Created DRM surface for connector ... CRTC ... mode ...
Created GBM buffered surface for connector ... CRTC ...
Native dmabuf feedback main device is ... from /dev/dri/card1
Native backend initialized; entering compositor event loop
Queued native frame on CRTC ... with buffer age 0
DRM vblank for ...
```

`Unable to become drm master, assuming unprivileged mode` can appear in the VM
while KMS rendering still works. Treat it as informational unless commits fail
immediately afterward.

## Expected Shutdown Noise

When the compositor exits intentionally, clients lose the Wayland socket. These
messages are expected after an emergency-exit chord, `SIGINT`, `SIGTERM`, or
`--exit-after-ms`:

```text
Native emergency exit chord pressed; stopping compositor event loop
Io error: Broken pipe (os error 32)
Gdk-Message: Lost connection to Wayland compositor.
failed to read events from the Wayland socket: Broken pipe
```

Firefox may also generate a minidump after the compositor intentionally exits:

```text
ExceptionHandler::GenerateDump ... minidump generation succeeded
Exiting due to channel error.
```

Do not treat those client-side shutdown messages as the root cause unless they
appear before Hearthspace logs an intentional stop.

## Problems To Investigate

### KMS Commit EINVAL

This means the kernel/driver rejected a page-flip/atomic commit:

```text
Page flip commit failed on device Some("/dev/dri/card1") (Invalid argument (os error 22))
```

The Parallels/virgl VM produced this when native KMS damage clips were forwarded
under heavy client redraw. Hearthspace currently avoids passing KMS damage clips;
if this returns, capture the surrounding 200 lines before the first failure and
check whether connector state changed, buffers were recreated, or session state
paused/activated.

### Client Dmabuf Import Failures

Client dmabuf import failures should include size, format, plane count, modifier
presence, and node. These are usually client/renderer compatibility issues rather
than KMS mode-setting problems.

### Shell Client EGL/WGPU Warnings

The shell can log transient EGL/wgpu context errors during startup in the VM,
especially before it falls back or reconnects. Treat them as non-fatal if native
frames continue to queue and vblank events continue arriving.

## Validation Checklist

Run automated checks before or after native smoke work:

```sh
cargo fmt --check
cargo check
cargo check --features udev
cargo check --no-default-features --features udev
cargo test
cargo test --features e2e --test headless_control
```

Native coverage still needs manual VT testing:

- Start with `--tty --no-shell` and verify a rendered background.
- Start with `--tty` and verify the shell bar appears.
- Launch at least one Wayland client such as Firefox, GNOME Calculator, or the
  GTK test app.
- Switch away from the VT and back while Hearthspace is running.
- Exit with an emergency chord and confirm the tail of the log is expected
  shutdown noise rather than a compositor panic.

## Existing Smoke Helpers

The repository includes native smoke helpers for repeatable scenarios:

```sh
scripts/smoke-udev-gtk.sh
scripts/smoke-udev-screenshot-command.sh
```

The screenshot command is expected to fail clearly on native until DRM readback
is implemented:

```text
screenshots are unsupported on the udev backend until native readback is implemented
```
