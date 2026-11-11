# Satellite X11 compatibility

Hearthspace runs X11 applications through
[xwayland-satellite](https://github.com/Supreeeme/xwayland-satellite), which
exposes them as XDG toplevels. Canvas positions stay in Hearthspace instead of
becoming X11 root coordinates. Satellite is the only X11 backend; the direct
Smithay XWM and backend selector have been removed.

Satellite is pinned to revision
`7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0`, package 0.8.3. Build the
`hearthspace-satellite` package alongside the compositor using the
[runtime build workflow](DEVELOPMENT.md#prepare-an-installable-runtime). The
helper and its verified private Xwayland must be beside the compositor executable.
There is no PATH fallback or helper-path override. Old `HEARTHSPACE_X11_BACKEND` and
`HEARTHSPACE_SATELLITE_BINARY` settings have no effect.

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
  --archive /absolute/path/to/xwayland-24.1.14.tar.xz \
  --build-dir /absolute/path/to/a-fresh-build-directory \
  --compositor /absolute/path/to/hearthspace-compositor
```

Choose the actual compositor's directory, including `target/release` or a custom
Cargo target. The helper must be beside that compositor. Preparation validates the archive
and patch hashes before compiling serially, retains its build evidence, and
stages `hearthspace-Xwayland` and its JSON receipt beside both executables.
Missing, modified or mismatched companions fail before Xwayland is launched.
Install the [development headers](DEPENDENCIES.md#satellites-private-xwayland-build)
first. This does not install or replace the distribution's Xwayland.

## Runtime packaging

The inventory builds `hearthspace-satellite` with the desktop. Preparation stages
its private server and receipt into the same binary directory. Installation
verifies/copies both companions, their exact file modes and X.Org notices;
Satellite's MPL notice, provenance and complete modified source are included under
`share/doc/hearthspace/`. Source ownership/time metadata is normalized so identical
input trees produce identical source archives. No system Xwayland is replaced.
The native harness validates companions beside the selected compositor, including
an explicit compositor override, before acquiring a VT. It records their hashes.
CI prepares the same pinned source before integration tests and retains build
provenance. Normal startup never downloads or builds the C server.

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

## Appearance and cursor preferences

Satellite remains the XSettings manager. Hearthspace sends its saved and live
light/dark theme through a private, client-restricted settings interface; scale
changes retain that theme and theme changes retain Satellite's DPI entries.
The shared cursor default is seeded before DISPLAY publication. Explicit
XCURSOR_SIZE, existing resource properties, and legacy Xdefaults/Xresources
remain authoritative; no host resources or environment are rewritten.

## Client resize hints

Satellite applies X11 base sizes and cell increments to ordinary buffer resizes.
Hearthspace retains canvas anchoring and XDG min/max constraints. Maximization
and fullscreen use their assigned bounds without cell rounding; malformed hints
cannot request negative or overflowing buffer sizes.

GTK frame extents become XDG window geometry. Buffer sizing includes those
insets, while placement, constraints and capture use the visible rectangle.
Deleted or malformed insets restore the full geometry. When a host restore
configure leaves a dimension unspecified, Satellite uses the remembered floating
buffer size, including for applications launched maximized. Explicit host sizes
remain authoritative.

## Clipboard and primary selection

X11 clipboard owners can publish before any X11 window has keyboard focus.
The standard data-control manager is visible only to the private Satellite
connection. Satellite uses it for X11 export and ignores its host offers;
reads from the host continue through ordinary clipboard/primary offers.
Selection identity survives replacement and destruction, and clipboard and
primary ownership remain independent. The real GTK workflow covers both
transfer directions, large text, primary middle-click and owner-exit recovery.
These checks do not establish native presentation behavior.

## Desktop-file drag-and-drop

Satellite translates host URI-list Copy offers into XDND versions 2–5. Target
status, drop and finish remain associated with one source-window incarnation.
Only the accepted target receives a drop; a stale conversion cannot retrieve a
later drag's files. Motion is coalesced while a position acknowledgment is
outstanding, and release waits for that final status before sending XdndDrop.
Transfers reuse Satellite's selection and INCR code with bounded property
chunks. Undropped offers remain owned by SCTK until host leave; cancellation
revokes acceptance. A dropped offer finishes only after the matching X target
reports success. Clipboard and primary ownership are independent.

Validated XDND proxy windows retain the original destination identity. A proxy
must point to itself; stale or invalid properties fall back to the real target.
Destroying an active proxy revokes acceptance before the held release.

The real receiver regressions check exact URI bytes, INCR completion, repeated
drops, rejection, Escape, lock, source removal and replacement-window isolation.
Other formats/actions and X11-to-Wayland application drag-and-drop are not added
by this desktop-file bridge. See [Clipboard](CLIPBOARD.md).

## Window identity

Satellite reports a nonblank X11 class, then instance, then the executable
basename from the XRes client credential. Changes and property deletion update
the same managed window. The host reports these windows as X11 using private
connection provenance; a shell-like class does not change their application role.

## Focus and interaction compatibility

Globally-active X11 clients receive WM_TAKE_FOCUS when host keyboard focus
returns. Host and EWMH maximization update the same XDG window and preserve
fullscreen state. Touch retains Hearthspace's established X11 pointer fallback;
the bridge's XDG wrapper does not imply native-touch support in every X11 app.
Canvas moves retain a bounded X backing origin. Popups prefer an explicit mapped
transient parent with a live XDG role, falling back to valid hover/focus history.
Popup offsets and EWMH input
coordinates use that backing root, separately from displayed canvas positions.

## Input routing and verification

The private server carries the
[surface-target repair](../patches/xwayland-surface-input-targets.md).
It retains the Wayland surface recipient through queued X input, rather than
choosing a recipient from overlapping backing roots at delivery time. It also
repairs button state on re-entry without delivering an unmatched release to a
neighbor after a cancelled move. The optional cancellation extension handles
compositor-owned interactions; it does not replace the server routing repair.

Real-client regressions cover batched recipients, canvas positions beyond the
finite X root, cancelled interactions, popup anchors, replayed/frozen grabs,
window destruction, and scrolling. Other adoption checks cover clipboard,
appearance, cursor defaults, identity, touch fallback, and packaging. Headless
results do not establish native display or physical input behavior; native
acceptance runs separately through the [owned harness](NATIVE_TESTING.md).

The private server does not make the X root infinite. X11 backing sizes and
protocol coordinates retain their finite limits. Keyboard-initiated EWMH
move/resize and general application-to-application cross-protocol drag-and-drop
remain unsupported. See [Clipboard](CLIPBOARD.md) for the separately supported
desktop-file workflow and selection ownership.
