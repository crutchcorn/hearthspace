# Screen and window sharing

Native Hearthspace sessions provide the desktop ScreenCast portal used by
Discord, Chromium, and other PipeWire-aware applications. Start sharing in the
application, then select a screen or window in **Share a screen or window** and
press **Share**. Cancel closes the request without selecting another source.
Stop sharing from the requesting application.

A screen includes the visible desktop. A window source contains the selected
application's surfaces, independent of canvas zoom and overlapping applications.
Closing that window ends its stream; it never changes into a screen stream.
Capture is unavailable while the session is locked or its VT is inactive.

This integration supplies video. Application audio sharing, remote control,
and browser tab capture are separate capabilities. Enabling screen sharing does
not enable Hearthspace's virtual keyboard or pointer protocols. The
`--remote-desktop` development option is not required.

## Runtime requirements

Install `xdg-desktop-portal`, `xdg-desktop-portal-gtk`, `zenity`, and a working
PipeWire service/session manager. Hearthspace uses **xdg-desktop-portal-wlr
0.8.4 with the session-policy, Start-session, and capture-lifecycle patches** for
ScreenCast. The lifecycle repair keeps capture buffers and pending frames paired
across resize, pause, and shutdown.
Version 0.8.2 fixed PipeWire 1.6 compatibility;
0.8.3 has a known recording stall fixed in 0.8.4. See the
[upstream releases](https://github.com/emersion/xdg-desktop-portal-wlr/releases).
An unpatched distribution package does not provide the required service-name
and consent controls. Ubuntu 26.04's 0.8.1 package also predates the stream fixes.

Build the pinned release as your normal user, starting from the Hearthspace
repository root. The other compiler and graphics development libraries are
listed in [dependencies](DEPENDENCIES.md). The
[patch record](../patches/xdg-desktop-portal-wlr-session-policy.md) describes its
upstream basis and removal conditions.

```sh
sudo apt-get install -y meson ninja-build libpipewire-0.3-dev libinih-dev zenity
hearthspace_checkout="$PWD"
git clone --branch v0.8.4 --depth 1 \
  https://github.com/emersion/xdg-desktop-portal-wlr.git
cd xdg-desktop-portal-wlr
test "$(git rev-parse HEAD)" = 34153094662acd713241ca6cbbb20003ef67da5f
git apply "$hearthspace_checkout/patches/xdg-desktop-portal-wlr-session-policy.patch"
git apply "$hearthspace_checkout/patches/xdg-desktop-portal-wlr-start-session.patch"
git apply "$hearthspace_checkout/patches/xdg-desktop-portal-wlr-capture-lifecycle.patch"
meson setup build --prefix=/usr/local -Dsystemd=disabled -Dman-pages=disabled
meson compile -C build
python3 - "$hearthspace_checkout/patches" <<'PY'
import hashlib, json, sys
from pathlib import Path
binary = Path("build/xdg-desktop-portal-wlr")
project = json.loads(Path("build/meson-info/intro-projectinfo.json").read_text())
receipt = {
    "version": project["version"],
    "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "source_revision": "34153094662acd713241ca6cbbb20003ef67da5f",
    "session_policy_sha256": hashlib.sha256(
        (Path(sys.argv[1]) / "xdg-desktop-portal-wlr-session-policy.patch").read_bytes()
    ).hexdigest(),
    "start_session_sha256": hashlib.sha256(
        (Path(sys.argv[1]) / "xdg-desktop-portal-wlr-start-session.patch").read_bytes()
    ).hexdigest(),
    "capture_lifecycle_sha256": hashlib.sha256(
        (Path(sys.argv[1]) / "xdg-desktop-portal-wlr-capture-lifecycle.patch").read_bytes()
    ).hexdigest(),
}
Path("build/xdg-desktop-portal-wlr.json").write_text(json.dumps(receipt) + "\n")
PY
sudo install -D -m 0755 build/xdg-desktop-portal-wlr \
  /usr/local/libexec/xdg-desktop-portal-wlr
sudo install -m 0644 build/xdg-desktop-portal-wlr.json \
  /usr/local/libexec/xdg-desktop-portal-wlr.json
sudo install -D -m 0644 LICENSE /usr/local/share/doc/hearthspace-portal-wlr/LICENSE
```

This installs the backend executable, source/patch/hash receipt and its notice.
The installer checks the receipt against this checkout's required patches and the
installed binary. The backend also rejects unknown policy flags before the
session starts its portal frontend. This does not enable a global service or
replace another desktop's portal configuration. Keep the build
directory and revision for upgrades. To undo this local installation, remove
only those three installed files after stopping Hearthspace; a distribution-managed
backend, if present, remains untouched. Hearthspace prefers the local executable
over `/usr/libexec/xdg-desktop-portal-wlr`.

Rebuild and run `./scripts/install-session install` after updating Hearthspace,
then log out and log back in. Installing a release does not replace an already
running compositor or its portal processes.

## Integration and ownership

The session writes temporary portal configuration selecting GTK for ordinary
dialogs, Hearthspace for appearance, and wlr for ScreenCast. It starts an owned
wlr backend with an explicit configuration and the bundled GTK chooser, before
refreshing the portal frontend. Existing shared permission/document services
retain their normal user-manager ownership. Hearthspace's backend owns the
dedicated `org.freedesktop.impl.portal.desktop.hearthspace_wlr` name. Another
desktop's ordinary wlr service may coexist; it cannot answer requests addressed
to this name. An existing foreign owner of the dedicated name is a startup
conflict; Hearthspace does not replace or kill it. During a backend restart gap,
requests fail until the owned backend is ready again.

The backend's `--no-restore` option ignores saved selections and disables new
persistence, including tokens originating in another desktop. Each new request
must use the chooser. The shared permission store remains unchanged. Picker
paths containing semicolons or line breaks are rejected because the backend's
INI parser cannot preserve those shell-quoted paths.

The compositor implements Smithay's standard output and foreign-toplevel image
capture protocols. The backend handles portal requests, consent, and PipeWire
negotiation. The chooser requires an explicit selection; a missing helper, error,
or cancellation never selects a default screen. Capture protocols, like the
existing local control socket, are available to trusted local Wayland clients;
the portal is the consent boundary for applications using desktop portals.

Private native test sessions keep a private bus and their own backend. PipeWire
access is also a boundary: reaching a host PipeWire socket does not prove its
portal access policy recognizes a frontend on a different bus. Integration tests
must use an owned matching PipeWire environment or verify that access explicitly.

Run the backend policy regression after building the compositor and patched
backend. This check also requires Python's GObject bindings (`python3-gi` on
Ubuntu):

```sh
sudo apt-get install -y python3-gi
scripts/check-portal-backend /tmp/hearthspace-portal-policy-check
```

The output directory must be new. This bounded check starts a disposable D-Bus
broker, software headless compositor, and private PipeWire server. It checks valid
restore data, chooser cancellation, accepted selection, and persistence in the
real backend; its logs and `result.json` retain binary hashes and cleanup status.
It does not test frontend token storage or native frame presentation.

Check the patched backend's capture lifetime against its built source tree:

```sh
scripts/check-portal-lifecycle /path/to/xdg-desktop-portal-wlr \
  /tmp/hearthspace-portal-lifecycle-check
```

Use a fresh output directory. This compiles the real backend callbacks with
transport fixtures and AddressSanitizer/UndefinedBehaviorSanitizer. It covers
pause/resume, buffer removal and replacement, queued timers, constraints
failures, teardown, and late Wayland events. Each invocation has a deadline.
It starts no desktop or PipeWire daemon; real changing-frame acceptance remains
necessary for the complete sharing path.

## Diagnosing a stalled request

Check the running session's logs for `portal-wlr` and `desktop-portal`. Verify
the ScreenCast backend exists before changing permissions. GTK alone does not
implement ScreenCast. A chooser opening establishes source discovery, not a
working video stream: successful acceptance must also receive changing frames
from the selected source.

Use [native testing](NATIVE_TESTING.md) for owned validation. Preserve backend
and frontend logs, request responses, stream metadata and frame evidence.
Validate both screen and window sharing, including an overlapping unrelated
window, resize, close, cancellation, and lock/inactive-session behavior. Avoid
sharing personal content or joining a real call merely to test negotiation.
