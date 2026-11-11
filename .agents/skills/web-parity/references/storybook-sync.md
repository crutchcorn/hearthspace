# Sync the native and web component catalogs

Use this alongside Web Parity when discovering shared components or updating
Hearthspace's UI Storybook. This is a comparison workflow, not automatic React
to Rust code generation.

## Start from the current reference

The reference project is
[hearthspace-ui-experiments](https://github.com/crutchcorn/hearthspace-ui-experiments).
Read its current [catalog guide](https://github.com/crutchcorn/hearthspace-ui-experiments/blob/main/docs/storybook.md),
`AGENTS.md`, `CONTRIBUTING.md`, and `package.json`. Record the reference revision
and whether it has local changes in task evidence. Do not assume the prototype's
GitHub Pages URL also hosts Storybook: verify deployment separately.

When local exploration is authorized, run these in the web checkout:

```sh
pnpm install --frozen-lockfile
pnpm storybook --host 127.0.0.1 --ci
```

Use the Node and pnpm versions declared in `package.json`; select an existing
installation rather than changing the system defaults. Open the printed address
(usually port 6006) through the available browser tools. `--ci` avoids opening
an extra browser. Reuse a running server only after identifying its owner; stop
only a server you started when finished. `pnpm build:storybook` produces the
static `storybook-static/` catalog, not a deployment.

## Load the Storybook agent guidance

The catalog inspected in November 2026 uses Storybook 11 Beta with React/Vite.
Its agent skills are served by the installed CLI, not repository `SKILL.md`
files. Discover them and read the relevant output:

```sh
pnpm exec storybook skills --help
pnpm exec storybook skills ui-workflow
pnpm exec storybook skills component-docs
pnpm exec storybook skills write-story
```

Use `component-docs` for discovery and API questions; use `write-story` before
an authorized web-story edit. `setup` is for changing Storybook setup, not a
prerequisite for browsing an existing catalog. Read fresh guidance rather than
copying an entire generated skill into Hearthspace. Some generic output still
contains older CSF examples: this project's current `.storybook/preview.tsx`
and adjacent stories use `definePreview`, `preview.meta`, and `meta.story`
(CSF 4 factories), with test helpers from `storybook/test`. Preserve that format.
A native-only sync does not authorize web edits, new addons, or external review
publication. Use review/testing tools only within the requested scope and when
the installed configuration supports them; do not invent an MCP connection.

Discover IDs and intended usage with the actual tools:

```sh
pnpm exec storybook tools docs list --withStoryIds true
pnpm exec storybook tools docs show --id <component-id>
pnpm exec storybook tools docs show-story --storyId <story-id>
pnpm exec storybook tools stories preview --stories '[{"storyId":"<story-id>"}]'
```

Replace placeholders with IDs returned by discovery, never guessed slugs.
`stories find-by-component` maps an authorized source path to stories;
`stories changed` finds stories affected by web edits. Inspect command help if
an installed version rejects a command. Follow documented props and examples;
use source/CSS to measure rendering or investigate gaps in generated docs.

## Navigate and vary the preview

- Search or expand **Shared → Controls** for Button, Text Input, Toggle Switch,
  Checkbox, Select, Level Bar, and Level Slider. **Shared → Icons** separates
  Actions, Apps, Filled Apps, Canvas, and System glyphs. **Desktop** groups
  Shell, Settings, Apps, Canvas, Frame, and Windows examples.
- Select a named story for its Canvas and editable **Controls** panel. Button
  includes Neutral, Primary, Destructive, Quiet, Disabled, and Icon Only states.
  Editing `children` changes preview copy without changing source. Reset args
  between comparisons; do not click **Update story** or **Create new story**
  during read-only exploration.
- **Docs** shows component props and multiple examples. The live component is
  rendered inside the preview iframe; interact with that frame, not a similarly
  named manager control. Open its observed `iframe.html?id=…&viewMode=story`
  URL for an isolated preview when the manager makes inspection difficult.
- **Hearthspace color mode** selects Light/Dark. **Theme** independently chooses
  Orange, Rose, Violet, Blue, Teal, or Olive. Verify `data-color-mode`,
  `data-accent`, and `--accent-base` on `.hearthspace-preview` inside the iframe;
  manager chrome or a plain background override does not establish app theme.
- Inspect **Actions** for callbacks and **Interactions** for available story
  plays. A displayed Interactions tab is not proof a story has assertions.
  Select → Automatic updates its args through `useArgs`: open it, choose Manual,
  verify the label/value, then reset. App Picker also needs open-menu coverage.

The preview adds a 24px surrounding pad. `CanvasSample` defaults to a 640×360
positioning surface, `SettingsSample` is a scrollable settings container, and
`WidgetSample` provides shell popup portals through `PlacedWidget`. Read
`.storybook/surfaces.tsx`, `fixtures.ts`, `preview.tsx`, and `preview.css` when
those affect the comparison. Measure the component itself, not the manager
sidebar or wrapper padding. Match logical preview dimensions, browser zoom,
output scale, canvas zoom, text, props, and theme before comparing screenshots.

## Map to native ownership and stories

Run the native gallery from Hearthspace's repository root:

```sh
cargo run --locked -p hearthspace-ui-storybook
cargo check --locked -p hearthspace-ui-components -p hearthspace-ui-storybook
```

`crates/ui-components` owns reusable native widgets, tokens, and glyphs.
`crates/hearthspace-ui-storybook` owns the gallery and component previews;
`crates/xilem-storybook` owns the theme-independent story/control state model.
React code and Storybook's JS manager do not belong in that Rust state model.

Record a focused mapping in task evidence: web component/story ID, source
revision, args/globals, native story ID and component owner, matching states,
and missing states. Names and API shapes differ: web Button variants may map
to Action Button, Compact Action Button, or Settings Materials examples rather
than one identically named native story. The native gallery currently starts
with `hearthspace_light_properties()`; do not claim dark/accent parity merely
because the web toolbar supports it. Record missing native controls/themes as
gaps and add support only within the requested UI change.

Compare the relevant enabled/disabled, hover/pressed/focused, selected, empty,
error, open-menu, and narrow states. Use the same text and fixtures on both
sides. Then inspect the affected shell/Settings surface in context; catalog
coverage cannot prove compositor clipping, backdrop blur, popup placement,
keyboard restoration, touch cancellation, authentication, or real services.
Fixtures and `fn()` callbacks demonstrate intent, not native capabilities.

## Validate and report

For a web edit, run its maintained checks and `pnpm build:storybook`; exercise
the affected stories in the browser. For native edits, run the relevant Rust
checks and use the native-control skill for real desktop interaction. Keep
story IDs, args/globals, logical bounds, observations, and remaining mismatches
with the evidence. Do not add screenshot regression tests just for a sync.
A documentation-only workflow update needs verified commands, link checks,
and skill validation, not a new Rust test or full compositor rebuild.
