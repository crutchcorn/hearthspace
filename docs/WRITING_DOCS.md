# Writing Docs

This document explains how Hearthspace documentation should be written. Good
docs should help contributors understand a project area well enough to make
future changes, even if the source files move or the implementation evolves.

The policy is:

- Write conceptual, evergreen documentation for contributors.
- Explain why the project uses a pattern, not only what the code does today.
- Start with the mental model before operational details.
- Keep durable commands, protocols, dependencies, and workflows when they help
  contributors do real work.
- Do not turn `docs/` pages into source maps or implementation inventories.

## The Mental Model

Project docs should teach how to think about an area of the system.

Most docs need three layers:

- Concepts: the domain ideas or external systems someone must understand.
- Project policy: Hearthspace's stable decisions, defaults, and boundaries.
- Operations: commands, dependencies, protocols, and workflows contributors use.

Prefer that order. A reader should understand the model before reading commands
or edge cases.

## Recommended Shape

Most docs should use some version of this structure:

1. Title.
2. Short opening paragraph explaining what the document teaches.
3. Policy summary with stable project decisions.
4. `The Mental Model` section.
5. Concept sections that explain the topic and Hearthspace-specific choices.
6. Operational sections only when useful.
7. `Contributor Rules` section with concise guardrails for future changes.

Not every doc needs every section. Use the smallest structure that teaches the
topic clearly.

## Tone

Write as a maintainer explaining the system to another contributor.

Good docs should:

- Be direct and factual.
- Name ownership boundaries explicitly.
- Distinguish concepts that are easy to conflate.
- Prefer stable principles over temporary implementation status.
- Use short sections with descriptive headings.
- Include examples only when they support the conceptual model.

Avoid writing docs as if they are generated notes, source-code summaries, or a
chronological debugging transcript.

## What Not To Put In docs/

Do not add source-map or implementation-waypoint sections to `docs/` pages.

Avoid sections named or shaped like:

- `Implementation Waypoints`
- `Source Map`
- `Code Paths`
- `Relevant Files`
- Long lists of source files and what each file currently does.
- Line-number references.
- Function-by-function control-flow mirrors.

Docs should not depend on exact source paths. If a reader needs the source, they
can search the codebase. The doc should teach the concepts and project rules
that make the source understandable.

References to other documentation pages are fine when they help navigation.
Stable external concepts, commands, protocols, environment variables, CLI flags,
asset locations, and package names are also fine when the document is about how
contributors use them.

## Handling Current State

Avoid turning docs into a list of current gaps. Mention limitations only when
they are stable operational constraints contributors need to know.

Prefer phrasing like:

```text
The policy is...
The model should preserve...
This path is intentionally staged...
Future work should keep this boundary...
```

Avoid phrasing like:

```text
As of this document...
Currently this function...
This exact file does...
Line X calls line Y...
```

## Operational Details

Commands, dependencies, protocols, and environment variables belong in docs when
they are durable and useful.

Good operational sections explain:

- When to run a command.
- What the command proves or changes.
- Which feature flag or dependency is required.
- Which failure mode the reader is likely to see.

Avoid dumping every possible command or option. Prefer the stable path a
contributor should use first.

## Contributor Rules

Use these rules when writing or rewriting docs:

- Lead with concepts and project policy.
- Preserve durable operational facts.
- Remove source-path inventories from `docs/` pages.
- Replace current-state narration with stable boundaries and rules.
- Link to related docs when that helps navigation.
- Keep docs useful after source files are reorganized.
- Do a final pass asking whether the document teaches a contributor how to make a
  future change correctly.

## Examples Of Good Emphasis

Good emphasis:

- Hardware cursor vs software cursor as display concepts.
- Compositor core vs display backend ownership.
- Headless control-socket tests vs WayDriver session tests.
- Renderer damage vs KMS damage clips.
- Pointer position vs cursor shape vs cursor presentation.

Poor emphasis:

- A list of every file touched by the feature.
- A chronological trace of function calls.
- A snapshot of temporary TODOs.
- A section that exists only to help someone grep for filenames.
