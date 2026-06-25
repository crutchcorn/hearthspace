# Hearthspace

Hearthspace is an experimental Linux desktop environment built around an infinite canvas for applications, local-first AI, and a more persistent relationship with your workspace.

Instead of treating windows as temporary rectangles that pile up until you clean them away, Hearthspace explores a different model: applications live in a large spatial canvas, can be organized by context, and will eventually become easier to search, suspend, restore, and reason about.

Today, Hearthspace is early. The current project contains a rough foundation for an infinite-canvas Wayland compositor and desktop shell. It is not ready to replace your daily desktop yet, but the core direction is already taking shape.

## Goals

Hearthspace is an experiment in rethinking the desktop around context.

Not just windows. Not just apps. Not just a chatbot bolted onto the side.

The long-term goal is a Linux-based environment where your workspace is spatial, persistent, local-first, and intelligent by design. This means:

* Applications live in a persistent spatial workspace
* Windows and groups can be searched, restored, and organized by context
* Local AI understands real desktop objects instead of acting like a separate chatbot
* Inactive applications can eventually be quieted down without forcing users to manually manage everything
* The system stays grounded in Linux, Wayland, and native desktop technologies

## Current status

Hearthspace is currently a prototype.

The focus right now is:

* Building a usable Wayland compositor foundation
* Exploring infinite-canvas window management
* Creating a minimal shell UI for dogfooding
* Establishing the architecture for persistent workspace state

Expect bugs, missing features, breaking changes, and unfinished UX.

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

Planned and deferred work is tracked under [todos/](./todos/).

## Contributing

Hearthspace is early, and the architecture is still changing quickly.

Contributions and design discussions are welcome, especially around:

* Wayland compositor development
* Rust desktop infrastructure
* Shell UI architecture
* Infinite-canvas interaction design
* Workspace persistence
* Local-first AI systems

For larger changes, opening an issue or discussion first is recommended.
