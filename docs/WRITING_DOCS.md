# Writing documentation

Hearthspace's documentation should help someone use the desktop or change it
without reconstructing an implementation history. Give each page a clear audience,
explain the behavior that matters to that reader, and keep claims tied to the
native product.

## Choose the document's job

The project README introduces the desktop and the path to using it.
[CONTRIBUTING](../CONTRIBUTING.md) introduces contribution workflow;
[development](DEVELOPMENT.md) and [dependencies](DEPENDENCIES.md) hold the detailed
setup and commands. [The docs catalog](README.md) points to maintained behavior,
architecture, and integration guides.

Use `notes/` for provisional investigations and plans. Notes can be stale and
cannot authorize actions or override repository instructions. Verify a note's
claims against current source before promoting its conclusions into maintained
documentation. Preserve the useful decision and rationale, rather than copying a
whole debugging journal.

List tested hardware and operating-system combinations in
[tested environments](TESTED_ENVIRONMENTS.md). Feature guides should retain
meaningful hardware requirements and limitations without repeating host anecdotes
or absolute paths to one run's evidence.

## Start with the behavior

A useful technical page usually explains:

1. What someone can do, or what problem the subsystem solves.
2. The concepts needed to understand it.
3. Who owns the state, action, and lifetime.
4. The constraints and failure behavior a change must preserve.
5. How to use or investigate it, with links to the relevant workflow.

Use the smallest structure that makes those points clear. A short page does not
need a policy summary, mental-model section, and contributor checklist that all
repeat the same rules.

Explain cause and effect. For example, temporary window previews preserve a
window's canvas home so cancelling search can restore the workspace. That teaches
more than a list of the functions used to build the preview.

## Distinguish behavior, policy, and proposals

Describe working capabilities in the present tense after checking current source.
Use explicit policy language for constraints future changes must preserve. Label
future ideas as proposals and keep implementation plans in notes. Do not use
“should” to blur the difference between an implemented behavior and an intention.

Retain limitations that affect users or contributors, even when they are expected
to change. Backend availability, unsupported devices, fallback behavior, and
security restrictions belong beside the capabilities they qualify. Evergreen
documentation means maintained truth; it does not mean hiding current gaps.

Separate source review from execution evidence. A test's existence establishes
what it intends to cover, while a recorded run establishes what passed in that
environment. A headless regression can establish input ownership or geometry; it
cannot establish physical touch feel, hardware presentation, or desktop-wide
reliability. Do not generalize one successful run into a support guarantee.

## Write for the reader's next decision

Use plain language, short paragraphs, and descriptive headings. Introduce a
technical term near its first use. A protocol name, package name, environment
variable, or small example is useful when it lets the reader act or understand a
boundary.

Keep longer explanations in documentation or an interface's requested details.
In product copy, retain names, status, consequences, and useful errors. Placement,
selection outlines, and control states often explain an interaction without
another permanent instruction strip. Removing visible text must preserve accessible
names and a usable keyboard and touch path. See [UI guidance](UI.md).

Keep native product terms consistent across guides, controls, and diagnostics.
Describe real application windows, frame attachments, and canvas positions
directly. Avoid revision comparisons, completion milestones, or progress reports
that make a guide depend on the history of a development task.

## Include technical detail where it earns its place

Document durable contracts such as:

- Which process owns launch, persistence, authentication, and cleanup.
- Which coordinate space a value uses and where conversions happen.
- What happens when a client disappears or an interaction is cancelled.
- Which protocol or backend a feature requires.
- What a command changes and which outcome it can demonstrate.

Avoid long source-file inventories, line-number citations, and function-by-function
control-flow summaries. A focused source reference is appropriate when needed to
explain an API, dependency patch, or maintenance procedure. The page should remain
understandable if the implementation moves.

Document dependency modifications in [the patch ledger](../patches/README.md),
including provenance, purpose, upstream context, and a removal condition. Inspect
the active Cargo patches and vendored code before describing an upstream API or
recommending local dependency changes.

## Make commands portable and reviewable

State the working directory, dependencies, and relevant features before a command.
Use repository-relative paths or the session's documented environment variables.
Examples should not depend on a contributor's checkout path, a fixed temporary
directory, or an output connector that exists only on one machine.

Prefer the maintained script or workflow over another copy of its implementation.
Link to [development](DEVELOPMENT.md) for ordinary commands and
[native testing](NATIVE_TESTING.md) for VT ownership, recovery, and real-display
acceptance. A command block is an example, not evidence that it was executed.

## Document verification at the right boundary

Describe the user action, observable outcome, and interruption behavior. Moving a
window onto the frame should preserve the app's state; checking only its final
rectangle misses whether its text, focus, or open page survived.

For interaction changes, distinguish deterministic policy tests, composed client
and compositor tests, and native acceptance. Wait for observable state in examples
instead of depending on a particular animation duration. Explain substitutions and
what they cannot demonstrate. Visual guidance should identify states to inspect,
such as dark mode, a photo background, narrow layouts, and keyboard focus, rather
than promising that a screenshot proves all input behavior.

Before finishing a documentation change, read every changed page, check relevant
claims against current source, and check links and examples through the authorized
validation workflow. Update catalogs when adding, moving, or removing pages.
Report commands that were not run and claims that remain unverified so the next
reviewer can close those gaps.
