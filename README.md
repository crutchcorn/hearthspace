<div align="center">
<h1>Hearthspace</h1>

<picture>
    <img width="256" alt="A cube with a chunk taken out and a fire inside" src="https://raw.githubusercontent.com/crutchcorn/hearthspace/refs/heads/main/assets/logo.svg">
</picture>

<p>A spatial Linux desktop for persistent, local-first computing.</p>

</div>

<hr />

Hearthspace is a very early experimental Wayland compositor and shell built around an infinite canvas for applications, local-first AI, and a more persistent relationship with your workspace.

<div align="center">
<video controls>
  <source src="https://raw.githubusercontent.com/crutchcorn/hearthspace/refs/heads/main/assets/demo.mp4" type="video/mp4">
</video>
</div>

Instead of treating windows as temporary rectangles that pile up until you clean them away, Hearthspace explores a different model: applications live in a large spatial canvas, can be organized by context, and will eventually become easier to search, suspend, restore, and reason about.

**Hearthspace is not ready for daily use. I’m developing it in public while working toward a dogfoodable alpha.**

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
