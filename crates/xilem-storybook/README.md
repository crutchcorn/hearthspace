# xilem-storybook

`xilem-storybook` is a small, Hearthspace-agnostic state crate for building
Storybook-like component galleries in Xilem applications.

It intentionally does not depend on Hearthspace UI crates or impose a visual
theme. Consumers provide Xilem views for the sidebar, preview, and controls,
while this crate owns reusable story metadata, selected-story state, and typed
live control values.

The Hearthspace repository uses it from `hearthspace-ui-storybook` to showcase
`hearthspace-ui-components`, but other Xilem projects can reuse the same model
with their own component library and styling.
