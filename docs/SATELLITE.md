# Experimental Satellite integration

Hearthspace can evaluate X11 applications through
[xwayland-satellite](https://github.com/Supreeeme/xwayland-satellite), which
exposes them as XDG toplevels. Canvas positions then stay in the compositor
instead of becoming X11 root coordinates. The normal backend remains the direct
Smithay Xwayland integration. Satellite is not ready for regular sessions or
included in the installed runtime inventory.

The evaluation is pinned to revision
`7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0`, package 0.8.3. Build the
`hearthspace-satellite` package alongside the compositor using the
[guarded build workflow](DEVELOPMENT.md#guarded-builds). In an owned, private
headless test, `HEARTHSPACE_X11_BACKEND=satellite` selects it. The helper is
resolved beside the compositor executable; an absolute
`HEARTHSPACE_SATELLITE_BINARY` path can select a separately built copy.

The compositor supplies a private Wayland socket, owns the helper process
group, waits for its display readiness record, and publishes that display to
session clients. The helper owns Xwayland and the X11 compatibility state.
Applications do not receive the private socket. The helper group is stopped
and reaped during compositor teardown.
The embedded helper gives nested Xwayland null stdin/stdout so it cannot retain
the private host socket or readiness pipe after the helper exits. The library's
default stdio behavior remains inherited for other consumers.

## Prepare the private server

After building the helper and compositor, download the pinned source archive
from the URL in [requirements](../data/xwayland-requirements.json) to persistent
storage. Then run, as your normal user:

```sh
scripts/prepare-xwayland \
  --archive /absolute/path/to/xwayland-24.1.10.tar.xz \
  --build-dir /absolute/path/to/a-fresh-build-directory \
  --compositor /absolute/path/to/hearthspace-compositor
```

The helper must be beside that compositor. Preparation validates the archive
and patch hashes before compiling serially, retains its build evidence, and
stages `hearthspace-Xwayland` and its JSON receipt beside both executables.
Missing, modified or mismatched companions fail before Xwayland is launched.
Install the [development headers](DEPENDENCIES.md#satellites-private-xwayland-build)
first. This does not install or replace the distribution's Xwayland.

## Optional cancellation bridge

The local [Satellite patch](../patches/satellite-interaction-cancel.patch)
defines `xwayland_satellite_interaction_v1`. It adds cancellation to an accepted
XDG move or resize; it grants no authority to start one. Satellite records the
specific toplevel, seat and input serial it used for the request. EWMH Cancel,
including a message with button zero, consumes that record and forwards it.
Unmapping or destroying the X window removes the record. Without the optional
global, ordinary move/resize remains available and cancellation is unsupported.
EWMH itself carries no initiating serial: Satellite associates Cancel with that
X window's latest forwarded request. The protocol checks protect the forwarded
identity; they cannot recover intent missing from an old X11 message.

Hearthspace advertises this interface only to its private Satellite client.
It checks the currently accepted interaction and the still-owned input grab.
Stale serials, other toplevels/seats, duplicates and completed sequences cannot
cancel another operation. Accepted cancellation retains the reached geometry,
finishes resize through the existing late-buffer anchoring path, and keeps the
compositor's held pointer buttons captured until they drain.

Satellite applies a new Wayland viewport size before its actual X buffer has
necessarily resized. Regression tests distinguish those two states when
checking a delayed buffer. These are headless protocol and client tests, not
evidence of native input or presentation behavior.

## Migration in progress

Stock Xwayland has two demonstrated input problems:

- Batched clicks to overlapping X roots both reach the last window. The same
  failure reproduces in Satellite's own upstream fixture without Hearthspace.
- On re-entry after a cancelled move, Xwayland's button-state repair can deliver
  an unmatched release to a neighbor, despite the compositor retaining and
  draining the original Wayland sequence.

The assertions remain enabled in `headless_control` under `satellite::`.
The private surface-target server now passes these focused cases, including
the additional non-delivering button-state repair. Hearthspace's helper
requires its exact prepared sibling binary and receipt; it never silently
uses stock Xwayland. Broader adoption gates remain pending. The [evaluation note](../notes/satellite-evaluation.md)
links the standalone reproduction and retained results.

[Issue 68](https://github.com/crutchcorn/hearthspace/issues/68) also requires
popup, frozen/replayed grab, destruction and scroll regressions; clipboard;
light/dark XSettings; cursor-size preferences; ordinary move/resize and touch;
zoom; multiple outputs; metadata; packaging; and native acceptance. These gates
must pass before switching defaults or retiring the custom Xwayland checkpoint.
The optional cancellation extension does not solve Xwayland recipient routing.
