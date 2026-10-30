# Development workflows

Start with [CONTRIBUTING](../CONTRIBUTING.md) for dependencies and the full check
sequence. These commands run from the repository root as the normal user.

## Prebuild the development runtime

Before the first session launch, build the runtime packages outside the
supervisor's 120-second startup budget. A fresh build can exceed that budget
while preparing helpers or waiting for the desktop to become ready.

Use the same inventory-driven commands as the native development harness:

```sh
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
PYTHONPATH=scripts python3 - <<'PY'
import os
import subprocess

from runtime_artifacts import development_commands

for command in development_commands(os.environ["CARGO_TARGET_DIR"]):
    subprocess.run(command, check=True)
PY
```

This builds each runtime package serially with its development feature selection,
including the native backend. Keep `CARGO_TARGET_DIR` consistent when launching
the session. Repeat the prebuild after dependency changes or clearing build
artifacts; incremental startup builds still count toward the deadline.

The runtime binary/package inventory is
[`scripts/runtime-artifacts.json`](../scripts/runtime-artifacts.json).
Installer, harness, and development bootstrap consume it. If you add a required
helper, update that contract and its tests. Development launchers use Cargo's
reported executable rather than assuming `target/debug`; installed releases use
adjacent binaries and don't depend on a checkout.

## Run the complete desktop

```sh
cargo run --locked -- --dev
cargo run --locked -- --dev --no-watch-apps
cargo run --locked -- --dev --dry-run --print-env
```

The root package is the session supervisor. It starts the compositor, shell,
settings daemon, and optional Settings build watcher with session-specific
runtime paths. The watcher requires `cargo-watch` or `watchexec` on PATH; without
either, Settings builds when first launched. `--no-watch-apps` disables the
watcher explicitly. Development startup prepares required helpers before using
them, within the startup budget described above. On a real TTY it chooses the
native backend; inside another graphical desktop it chooses nested mode.
Backend selection is explicit in the child command, so stale inherited display
variables don't choose it accidentally.

Native development starts its own session bus, so it can run from a TTY while
another desktop is still logged in. Notifications, portals, and application
activation use that private bus without taking over the other desktop's
services. The supervisor stops its bus when the session ends. An explicitly
supplied `--private-session-bus` remains externally owned.

Installed login sessions continue using the standard user bus, where Hearthspace
provides `org.freedesktop.Notifications`. Development isolation does not disable
notifications or change that installed-session binding.

Nested mode is useful for fast UI work, but the host can intercept keys and
mouse gestures. It doesn't establish native display, power, or lock behavior.
Prefer an explicit host-shortcut change you can undo over copying a global
collection of desktop settings. `--scroll-zooms` on a direct compositor run can
help when the host intercepts modifiers.

## Isolate a compositor behavior

```sh
cargo run --locked -p hearthspace-compositor --bin hearthspace -- --winit
cargo run --locked -p hearthspace-compositor -- --headless --no-shell
cargo run --locked -p hearthspace-compositor -- --headless --no-shell --headless-size 1280x720 --headless-scale 2
```

Use `--bin hearthspace` when the direct run should start a shell. Shell startup
re-executes the current binary with `--shell`; the default `hearthspace-compositor`
binary doesn't handle that mode, so direct runs of it need `--no-shell`. The
complete session starts its shell separately.

`--winit`, `--headless`, and `--tty` are mutually exclusive. Headless uses an
offscreen output, defaulting to 1280×720 at scale 1. `--no-shell` lets a fixture
supply only the clients under test. The native `--tty` path requires the `udev`
feature; use the [owned native harness](NATIVE_TESTING.md) for automated runs.

`--exit-after-ms` asks the compositor to exit from its event loop, but can't
recover a stuck loop. The native harness's independent watchdog owns that case.
Native emergency exit chords are Ctrl+Alt+Backspace and Ctrl+Alt+Esc.

For a focused regression:

```sh
cargo test --locked -p hearthspace-compositor --features e2e,test-apps --test headless_control canvas_search
```

Follow [E2E testing](E2E_TESTING.md) for graphics/accessibility setup and required
checks. Private socket fixtures should not attach to the host desktop's bus or
Wayland display.

## Settings and shared UI

For standalone Settings work, run the daemon and app in separate terminals:

```sh
cargo run --locked -p hearthspace-settingsd
cargo run --locked -p hearthspace-settings
```

These defaults use the normal settings profile, including
`~/.config/hearthspace/config.toml`. Use an isolated session when a test should
not modify your own preferences. [Settings](SETTINGS.md) documents overrides,
transactions, and live refresh.

Inspect reusable controls in the storybook:

```sh
cargo run --locked -p hearthspace-ui-storybook
cargo check --locked -p hearthspace-ui-components -p hearthspace-ui-storybook
```

Put reusable primitives and theme properties in `crates/ui-components`, keep
feature state in its consumer, and add a story for new shared controls.
`xilem-storybook` remains independent of Hearthspace's theme. See [UI](UI.md)
for sizing, focus, interaction, and accessibility conventions.

The `test-apps` feature includes GTK and clipboard fixtures. It is unnecessary
for ordinary desktop use:

```sh
cargo run --locked -p hearthspace-compositor --bin hearthspace --features test-apps -- --gtk-test-app
```

## Assets and native input

[Cursor assets](CURSOR.md#cursor-assets) documents editable artwork, hotspots,
and regeneration. [UI](UI.md#icons-and-artwork) covers shared native glyphs and
application artwork. Keep generated assets beside their editable sources and
check the result at the actual output density.

Virtual-input fixtures exercise the native gesture path without relying on a
particular laptop. Follow [pinch and touch](PINCH_ZOOM.md#verification-boundaries),
[GTK touch testing](GTK_TOUCH_TESTING.md), and [native testing](NATIVE_TESTING.md)
for owned device creation, sequence cancellation, and cleanup. Build before
claiming a VT; these workflows are separate from headless regressions.

```sh
scripts/pinch-smoke --session "$(mktemp -d -t hearthspace-pinch.XXXXXX)"
scripts/pan-smoke --session "$(mktemp -d -t hearthspace-pan.XXXXXX)"
```

Run them serially. Both default to touchpad and touchscreen coverage; use
`--device touchpad` or `--device touchscreen` for a focused run. Each run owns
its session and temporary input devices and must restore the original VT.

## Read the right logs

Tracing is enabled without an extra Cargo feature. Start broad, then narrow the
module when investigating a noisy path:

```sh
RUST_LOG=info cargo run --locked -- --dev --no-watch-apps
RUST_LOG=hearthspace_compositor=debug,smithay=warn cargo run --locked -p hearthspace-compositor -- --headless --no-shell
RUST_LOG=hearthspace_compositor::compositor::input=trace cargo run --locked -p hearthspace-compositor -- --headless --no-shell
```

Native runs retain `session.json`, `session.log`, binary hashes, and command
results in their explicit artifact directory. Keep the original failed run when
retrying; a later pass doesn't diagnose the earlier failure.

The private control socket is passed through `HEARTHSPACE_COMMAND_SOCKET`.
Use the typed clients in `hearthspace-ipc` or the documented agent commands
instead of adding another framing implementation. Ordinary replies use `ok`
or `err`; payload replies carry a bounded byte count. Launch acceptance has
separate deadline/cancellation semantics. See [architecture](ARCHITECTURE.md)
and [native testing](NATIVE_TESTING.md).

## Dependency work

Cargo fetches pinned upstream revisions, with local repairs selected by the
workspace's patch table. Inspect [`patches/README.md`](../patches/README.md)
and the affected vendor provenance before updating them. Check upstream for an
existing repair before building another workaround.

Xilem, Masonry, rendering dependencies, and feature-enabling dependencies must
remain compatible. Changing one major version independently can create a second
copy that doesn't enable the intended features. Use `cargo tree` to inspect the
actual graph, then run the full checks after a dependency change.
