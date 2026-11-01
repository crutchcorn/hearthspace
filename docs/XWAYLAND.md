# Xwayland input and preparation

Hearthspace uses a pinned, patched Xwayland to keep X11 input attached to the
window selected on the canvas. Managed X11 backing windows use a stable root
origin while their canvas positions remain independent. The dependency carries
the selected surface through queued input and grab replay, so overlapping X
backings do not redirect a click to another application.

**Development checkpoint:** the isolated dependency regressions pass. The
supported launch, installer and maintained test integration is still undergoing
validation. The branch is not yet an installed or release-ready fix.

## Prepare the dependency

Install the [Xwayland build prerequisites](DEPENDENCIES.md#xwayland-build-tools)
and [prebuild the runtime](DEVELOPMENT.md#prebuild-the-development-runtime) first.
Run preparation as your normal user, before launching or installing the desktop.
It does not replace `/usr/bin/Xwayland` and does not run during session startup.

The following example uses the default debug compositor. For another profile or
`CARGO_TARGET_DIR`, pass the actual prebuilt compositor executable reported by
Cargo. Preparation stages companions beside that executable.

```sh
mkdir -p "$HOME/.cache/hearthspace" "$HOME/.local/state/hearthspace-checks"
xwayland_archive="$HOME/.cache/hearthspace/xwayland-24.1.10.tar.xz"
xwayland_build_dir="$HOME/.cache/hearthspace/xwayland-$(date +%Y%m%d-%H%M%S)"
curl --fail --location \
  https://xorg.freedesktop.org/archive/individual/xserver/xwayland-24.1.10.tar.xz \
  --output "$xwayland_archive"
scripts/prepare-xwayland \
  --archive "$xwayland_archive" \
  --build-dir "$xwayland_build_dir" \
  --compositor "$PWD/target/debug/hearthspace-compositor"
```

The build directory must be fresh. The helper checks the archive and patch
against [trusted requirements](../data/xwayland-requirements.json), extracts the
source into its owned directory, and runs a bounded serial debug build. It
retains the build log and preparation metadata. It never downloads a dependency
itself. Preserve failed build directories and choose a new directory to retry.

The two runtime companions are:

| File | Purpose |
| --- | --- |
| `hearthspace-Xwayland` | The patched executable selected by absolute path |
| `hearthspace-Xwayland.json` | Version, source/patch identity and executable digest |

The compositor verifies the files against its embedded requirements before
launching Xwayland with `-surface-input-targets`. Missing, modified or incompatible
companions fail startup with preparation instructions. A version string or an
unpatched executable on `PATH` cannot substitute for them. Receipts describe a
trusted local build; they are not signed attestations.

Normal startup requires both Xwayland and its window manager to become ready
within 15 seconds. Public compositor sockets and ordinary app startup wait for
that result; a stalled handshake fails with the selected dependency paths in
the diagnostic, and startup cleanup terminates and reaps the owned server.
Keyboard shortcuts and deferred application launches remain gated until public
listeners are ready. This gate applies only to startup: a later Xwayland failure
retires its X11 windows while preserving Wayland applications and new Wayland
launches, without substituting another server.

For an intentionally Wayland-only development or test session, pass
`--no-xwayland` to the session or compositor command. Missing companions never
select this mode automatically.

The installer copies both companions into the same immutable release as the
compositor, together with upstream notices. Even `--destdir` requires compatible
artifacts. The native harness verifies companions before acquiring a VT,
including with `--no-build`. After changing the dependency pin or patch, repeat
preparation for every compositor directory you intend to launch.

Use `scripts/install-session install --no-build --binary-dir /absolute/runtime/bin`
after preparing a complete runtime there. The default installer build uses
`target/release`; prepare that profile's compositor companions before installing.
Follow the [guarded build guidance](DEVELOPMENT.md#guarded-builds) for local
release Cargo builds on a shared desktop.

## Check input routing

Run the maintained dependency check against the same prepared source, compiler
database and staged executable:

```sh
scripts/check-xwayland-input \
  --xwayland "$PWD/target/debug/hearthspace-Xwayland" \
  --source "$xwayland_build_dir/source/xwayland-24.1.10" \
  --compile-commands "$xwayland_build_dir/build/compile_commands.json" \
  --output "$HOME/.local/state/hearthspace-checks/xwayland-$(date +%Y%m%d-%H%M%S)"
```

The output directory must be fresh. The check verifies preparation provenance,
builds the standalone fixture with its lockfile, and runs both required parts:

- A private Wayland server and real X clients exercise relative motion,
  confinement, exact event counts, recipient isolation and cleanup.
- Ninety-eight actual-callback cases check scroll distance and ordering, with
  49 default-off sequences compared against the retained upstream-baseline
  traces. The callback fixture substitutes MI draining, so it does not by itself
  establish DIX delivery.

The standalone Rust crate also needs formatting, lint and supervisor checks:

```sh
cargo fmt --manifest-path tests/xwayland-relative/Cargo.toml --check
cargo clippy --locked --manifest-path tests/xwayland-relative/Cargo.toml \
  --all-targets -- -D warnings
cargo test --locked --manifest-path tests/xwayland-relative/Cargo.toml \
  -- --test-threads=1
```

The regular compositor suite covers batched recipients, CSD move/resize/close,
touch fallback, popup anchoring, synchronous grabs, replay and target lifetime.
Run the full [contributor checks](../CONTRIBUTING.md#checks) before completion.
Software headless and private-protocol results do not replace native validation.

## Compatibility boundaries

Normal X grabs retain their arbitration and release behavior. Ordinary client
warps and XTEST retain global X semantics; they do not inherit a native surface
target. Surface incarnation checks protect queued native input from destruction
followed by XID reuse.

The X root remains finite. A backing or menu extending beyond it can still have
unreachable input regions; zoomed-out canvas maximization can request such a
backing. Use focused view or a smaller application size for that case. See
[window management](WINDOW_MANAGEMENT.md#xwayland-coordinate-boundary-experimental)
and the [patch record](../patches/xwayland-surface-input-targets.md) for the
routing contract, scroll precision boundary and upstream removal criteria.
