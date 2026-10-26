# Clipboard

Copy and paste between Wayland and X11 applications with their usual shortcuts.
Selecting text and middle-clicking uses a separate primary selection, leaving
the ordinary clipboard unchanged. Hearthspace keeps no clipboard history, so
closing the application that owns copied data can make that data unavailable.

## Selection ownership

Hearthspace uses Smithay's `wl_data_device` selection handling for ordinary
Wayland copy/paste and its XWM selection bridge for XWayland interoperability.
Keyboard focus controls which Wayland client receives the clipboard offer;
X11 requests for the Wayland clipboard require an XWayland window to have focus.
X11 applications share the same X server, so this is an XWM-level access check.

The bridge forwards advertised MIME types and transfers data through Smithay's
file-descriptor APIs. Hearthspace does not read application clipboard contents
into its window metadata. The owning application supplies the data.
Primary selection uses Smithay's primary-selection protocol and the XWM bridge,
including transfers between Wayland and XWayland apps.
Both selection offers follow keyboard focus. General application-to-application
cross-protocol drag-and-drop is not implemented. Hearthspace separately supports
dragging its own [desktop files](DESKTOP_ITEMS.md) into Wayland or X11 applications.

Agents can explicitly replace the clipboard with `hearthspace-agent clipboard-set`
or replace it and send a paste shortcut with `hearthspace-agent paste`. These
commands support Unicode, multiline text, and UTF-8 input files up to 16 KiB.
`paste --shortcut ctrl+shift+v` supports terminals using that binding. This changes
the ordinary clipboard; it does not change primary selection. The compositor owns
that explicit payload until replaced, and serves it through bounded asynchronous
transfers so a stalled paste recipient cannot freeze the desktop. The `type`
command retains its keyboard-event behavior and US ASCII mapping.

Large X11 transfers use Smithay's INCR protocol support. The vendored Smithay base
includes the upstream INCR header fix. Its local changes and refresh conditions
are recorded in the [dependency patch ledger](../patches/README.md).

## Regression workflow

The GTK fixture is a real editor running once with `GDK_BACKEND=wayland` and once
with `GDK_BACKEND=x11`. Tests send ordinary Ctrl+A/C/V through the compositor's
seat and compare the destination editor's exact text after GTK reports a
completed paste. Fixture JSON includes the actual backend and keyboard focus.
F5 reloads an input file into the editor so Unicode and long payloads do not
depend on the automation CLI's ASCII typing support.

For X11 Ctrl+C selections larger than 64 KiB, the fixture offers UTF-8 bytes
through GDK's normal clipboard API. This avoids a GTK 4.22 string serializer
deadlock addressed by [GTK commit `420845f7`](https://github.com/GNOME/gtk/commit/420845f7bd4ede96d72d175d159110a5b48181af).
The transfer still uses X11 INCR and ordinary destination paste;
small copies, cuts, and Wayland copies retain GtkTextView's standard bindings.
Fixture JSON records the last copy/cut's `clipboard_serialization` as
`gdk-utf8-bytes` or `gtk-text-view` (`none` before the first copy/cut). This is
test compatibility coverage, not a compositor workaround or validation of the
affected GTK string serializer.

Build the native session and test fixture:

```sh
cargo build --workspace --all-targets --features udev,e2e,test-apps
```

Run the headless regression with a private runtime and D-Bus session:

```sh
HEARTHSPACE_E2E=1 python3 -m unittest discover -s scripts/tests -p test_clipboard_headless.py -v
```

Set `HEARTHSPACE_CLIPBOARD_EVIDENCE` to a fresh directory to retain headless
artifacts. Without it, the test uses a temporary directory. Both the fixture
binary and Xwayland must be installed/built; missing prerequisites fail an
enabled test. The headless bus has no service activation directories, and fixture
clients disable portals and GVfs so clipboard tests cannot create unrelated
desktop services or document-portal mounts in their temporary runtime.

Run the same clipboard workflow on a spare native VT, separately from headless
GPU tests:

```sh
scripts/clipboard-smoke --session "$PWD/artifacts/clipboard-native-1" --timeout 120
```

Choose a fresh session path. This command uses the binaries already built above,
starts an owned native session, and stops it on success or failure. The launcher retains its independent
watchdog. It preserves app logs, input text, observed editor state, command/results
JSON, and PNG/metadata captures; the final result requires VT restoration and
complete session cleanup. See [Native Testing](NATIVE_TESTING.md) for host
requirements and recovery.

The shared workflow covers primary-selection middle-click paste in both
directions and independence from the ordinary clipboard, copy and cut with Unicode/multiline text, large
transfers in both directions, clipboard replacement, source process exit, and
native Wayland copy/paste. A changed screenshot alone is not accepted as a
successful transfer. Source-exit cases occur after completed transfers; they do
not cover interruption during a transfer or an XWayland server crash. Cancellation
of in-flight transfers on XWayland disconnection remains an upstream limitation.
