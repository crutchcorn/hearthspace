<div align="center">
<h1>Hearthspace</h1>

<picture>
    <img width="256" alt="A cube with a chunk taken out and a fire inside" src="https://raw.githubusercontent.com/crutchcorn/hearthspace/refs/heads/main/assets/logo.svg">
</picture>

<p>A spatial Linux desktop for persistent, local-first computing.</p>

</div>

<hr />

Hearthspace is an experimental Wayland compositor and shell built around an infinite canvas for applications and agent control.

<div align="center">

https://github.com/user-attachments/assets/b3e8d3fc-f47c-4ad8-baff-7f79327cb5ad

</div>

Instead of treating windows as temporary rectangles that pile up until you clean them away, Hearthspace explores a different model: applications live in a large spatial canvas, can be organized by context, and will eventually become easier to search, suspend, restore, and reason about.

**Hearthspace is experimental.** Browser, editor, terminal, and file-manager workflows have been exercised on native AMD hardware. The [daily usability record](./todos/DAILY_USABILITY.md) tracks acceptance evidence and remaining limits; this does not establish compatibility with every application or GPU.

## Try the desktop

After [setting up the development dependencies](./CONTRIBUTING.md), start a full
session with `cargo run -- --dev --no-watch-apps`. From a real Linux TTY this uses
the native DRM/KMS backend; inside another desktop it opens a nested window.

To choose Hearthspace at the graphical login screen, run
`scripts/install-session install` as your normal user, then log out and select
**Hearthspace** in the session chooser. The installer builds an optimized native
release before requesting sudo. See [graphical login](./docs/LOGIN_SESSION.md)
for installation, updates, removal, and the remaining local acceptance checks.

Super+Return opens a terminal. Tap Meta (Super), press Super+Space, or click the
search icon to search installed applications and running windows. Alt+Tab switches
applications with live window previews until Alt is released. Alt+Shift+Tab moves
backward. Super+arrow keys pan the canvas,
and Super+M / Super+F toggle maximize / fullscreen.
Slide three fingers to pan the canvas; pinch them to zoom. Swipe four fingers
left/right on a touchpad, or four/five on a touchscreen, to switch windows with
the same previews until you lift. Selection smoothly pans to the focused window.
Pinch inward to open
the live window overview, or outward to dismiss it: use four fingers on a native
touchpad, or four/five on a touchscreen. The native libinput backend does not
emit five-finger touchpad gestures. Lift your fingers to finish the gesture. The
Super+O shortcut also opens it; click or tap a preview to
focus that window, or press Escape to return. Native sessions provide
notifications, locking, and [lid/suspend recovery](./docs/POWER.md);
the bell icon opens notifications, the map icon opens the canvas minimap, and
the power menu contains Lock and confirmed Log out. Settings is available in
search. See [the compact shell](./docs/SHELL.md) and
[window management](./docs/WINDOW_MANAGEMENT.md) for keyboard controls and
[app compatibility](./docs/APPS.md) for profile and
package behavior.

Assign app shortcuts in **Settings → Search → Gestures**. Open search with Meta,
the search icon, or a downward touchscreen swipe from the top bar. The drawing
icon at the right of the search field enables **Draw to launch**; the adjacent
Settings icon opens its Search tab. Click and drag with a mouse or trackpad, or draw
with one touchscreen finger; a clear match launches immediately on release.
Use one finger to scroll Settings and search results outside the drawing area.
See [search gestures](./docs/FAVORITE_APPS.md) for assignment and recognition details.

Agents can start an owned native session on a spare VT, inspect apps and stable
window IDs, send input or Unicode paste, and verify screenshots or accessibility
state through `scripts/hearthspace-agent`. Follow [native testing](./docs/NATIVE_TESTING.md)
for watchdogs, session ownership, and recovery, or run the repeatable
[daily smoke workflow](./docs/DAILY_SMOKE.md).

## Goals

Hearthspace is an experiment in rethinking the desktop around context.

The long-term goal is a Linux-based environment where your workspace is spatial, persistent, local-first, and intelligent by design. This means:

* Applications live in a persistent spatial workspace
* Windows and groups can be searched, restored, and organized by context
* Local AI understands real desktop objects instead of acting like a separate chatbot
* Inactive applications can eventually be quieted down without forcing users to manually manage everything
* The system stays grounded in Linux, Wayland, and native desktop technologies

## Technology

Hearthspace is written in Rust.

The compositor is built with [`smithay`](https://github.com/Smithay/smithay), a Rust library for building Wayland compositors.

The shell UI is built with [`xilem`](https://github.com/linebender/xilem), an experimental Rust UI framework from the Linebender project.

## Roadmap

The rough direction is:

1. Build a stable compositor foundation
2. Add enough shell UI to use Hearthspace directly
3. Make the canvas useful for real workflows
4. Persist windows, groups, applications, and workspace state
5. Explore smarter application lifecycle management
6. Add local AI as a layer over the workspace model

The first milestone is not to build the full vision. It is to make Hearthspace usable enough to dogfood, learn from, and iterate on.

Planned work is tracked under [todos/](./todos/).

## License

Hearthspace code is licensed under the Apache License 2.0.

The Hearthspace name and logo are not covered by the Apache-2.0 license. You may not use them to imply endorsement of a modified version, commercial product, or unrelated project without permission.
