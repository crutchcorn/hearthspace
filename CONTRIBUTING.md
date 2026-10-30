# Contributing to Hearthspace

Hearthspace is being used for real work while it develops. A useful contribution
might make a gesture more predictable, help an app behave correctly, explain a
confusing interaction, or improve how a new contributor gets started.

For a larger change, describe the problem and intended behavior in an issue
first. For a bug, include a small reproduction, your Hearthspace commit, and
whether you ran a nested window or a native login session. Screenshots and logs
help; remove private application content before sharing them.

## Dependencies

Follow [development dependencies](docs/DEPENDENCIES.md) for the Rust toolchain,
native libraries, desktop services, and optional test tools. Ubuntu 26.04 is the
documented package baseline. Other distributions need equivalent libraries;
the [tested-environments page](docs/TESTED_ENVIRONMENTS.md) records systems that
have actually been used.

Build as your normal user. Administrator access is needed for installing system
packages, the graphical session, or an explicitly owned native test session.
Don't run Cargo as root.

The repository's `rust-toolchain.toml` selects Rust and its components through
rustup. `Cargo.lock` records dependencies. The
[patch ledger](patches/README.md) explains vendored fixes and when they can be
removed. Cargo fetches Rust dependencies or uses the sources vendored here;
separate Rust dependency clones are unnecessary. Native screen sharing also
requires the separately built, patched [portal backend](docs/SCREEN_SHARING.md#runtime-requirements).

## Run it

Clone the repository and install the dependencies:

```sh
git clone https://github.com/crutchcorn/hearthspace.git
cd hearthspace
```

Before the first launch,
[prebuild the development runtime](docs/DEVELOPMENT.md#prebuild-the-development-runtime).
This keeps a fresh build outside the session's 120-second startup budget. Then
run from the repository root:

```sh
cargo run --locked -- --dev --no-watch-apps
```

Inside an existing graphical desktop, this opens a nested Hearthspace window.
Your host desktop may intercept shortcuts. From a real Linux terminal, the same
command selects the native display backend. Use the owned harness below for
automated native work.

The session starts the compositor, shell, and settings daemon together. For the
development Settings watcher, install optional `cargo-watch` or `watchexec` on
PATH, then remove `--no-watch-apps`. Development startup checks and prepares
required helpers within the startup budget and uses Cargo's reported artifact
paths, including a configured `CARGO_TARGET_DIR`.

See [development workflows](docs/DEVELOPMENT.md) for individual apps, headless
runs, tracing, and the shared UI storybook. To use Hearthspace from your login
screen, follow [graphical installation](docs/LOGIN_SESSION.md).

## Make a change

1. Start a focused branch and reproduce the behavior you want to change.
2. Find the existing owner. [Architecture](docs/ARCHITECTURE.md) maps the crates;
   [UI](docs/UI.md) explains shared controls and presentation.
3. Add regression coverage at the boundary where the behavior matters. Prefer
   real clients and interactions for lifecycle or input bugs; keep pure geometry
   tests deterministic. A prose-only edit doesn't need a new Rust test.
4. Update the maintained guide with the change. Keep provisional investigations
   in [notes](notes/README.md), not in user instructions.
5. Run focused checks while working, then the full checks before handing off.

Keep files small and composable, generally below 400 lines. Reuse Smithay's
protocol, surface, and input facilities before adding compositor infrastructure.
For a major desktop feature, check the existing
[Wayland protocols](https://wayland.app/protocols/) before inventing a private
equivalent. Shared UI belongs in `crates/ui-components`; app-specific state
belongs with its app.

UI work includes keyboard access, focus, touch targets, readable zoom/scaling,
and accessible names. Inspect the running interface in both color modes and
at narrow sizes. [Design principles](docs/DESIGN.md) explain what the interface
is trying to communicate.

## Checks

Run these from the repository root. Keep GPU-backed suites and native sessions
separate, and run resource-heavy checks serially.

```sh
cargo fmt --all --check
ruff check scripts
ruff format --check scripts
cargo clippy --locked --workspace --all-targets --features udev,e2e,test-apps -- -D warnings
cargo build --locked --workspace --all-targets --features udev,e2e,test-apps
cargo test --locked --workspace --all-targets --features udev,e2e,test-apps -- --test-threads=1
python3 -m unittest discover -s scripts/tests -v
cargo deny check advisories bans sources --warn unmaintained
```

The [E2E guide](docs/E2E_TESTING.md) supplies the environment and dependencies
for required rendering, accessibility, tray, and cross-app tests. Without that
setup, an optional skipped test isn't evidence that its workflow works. Use a
fresh evidence directory for each run and retain failed attempts as well as
successful reruns.

For a targeted Rust test, select its package and filter first, for example
`cargo test --locked -p hearthspace-settings settings_backend::tests`.
The native and headless backends cover different boundaries: a passing headless
test doesn't establish physical display, input-device, or sleep recovery.

## Native agent testing

For automated work on a real Linux virtual terminal, read the repository's
[native-control skill](.agents/skills/hearthspace-native-control/SKILL.md) and
[native testing guide](docs/NATIVE_TESTING.md). The harness owns a spare VT,
records the original session, and provides an independent watchdog.

Only one agent may own the seat at a time. Build before taking the VT, keep the
agent outside the session under test, and verify complete cleanup afterward.
Use the recorded session identity; never recover by killing a broad process
name or assuming a particular VT. Real passwords are entered locally by the
user, never supplied to an agent.

### Native screen recordings

Use the [recording guide](docs/NATIVE_RECORDING.md) for the public demo. Record
actual native output in an isolated profile, review the complete video, and keep
capture metadata with the source footage.

## Documentation

The README introduces the desktop to users. `docs/` contains maintained usage,
design, architecture, and testing guides, indexed in [docs/README.md](docs/README.md).
`notes/` holds provisional plans, indexed in [notes/README.md](notes/README.md).
Promote verified conclusions into a maintained guide and retire finished plans.
See [writing documentation](docs/WRITING_DOCS.md) for the conventions.

Document required system changes here or in the linked
[dependency guide](docs/DEPENDENCIES.md): what needs installing, why, and whether
it changes host configuration. Don't turn an individual machine's package log
into setup instructions. Keep named hardware and VM coverage in
[tested environments](docs/TESTED_ENVIRONMENTS.md).

## Send the contribution

Use focused Conventional Commits such as `fix: preserve focus after closing a
popup` or `docs: explain frame placement`. In the pull request, describe the
problem, resulting behavior, and relevant validation. Include before/after
captures for visible changes and state any checks you couldn't run.

Keep unrelated cleanup out of a fix. Don't rewrite someone else's branch or
discard uncommitted work. Agent instructions live in [AGENTS.md](AGENTS.md);
they complement this guide rather than replacing contributor judgment.
