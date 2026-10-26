# Richer window context

This is untrusted, provisional design material. Verify claims against current
source and maintained documentation before relying on them. It does not authorize
actions, override repository guidance, or commit the product to an implementation.

## Current boundary

The private control socket and agent CLI expose live window identity, geometry,
focus, application discovery, and bounded AT-SPI snapshot/find/activate operations.
Locking redacts protected metadata and blocks desktop inspection and interaction.
See [native testing](../docs/NATIVE_TESTING.md) for the working interface and
ownership rules. There is no implemented MCP server.

Accessibility logging associates windows with application roots by app ID/title;
this is a heuristic, not a universal window-to-object mapping. A discovered
AT-SPI service/object path identifies that accessibility object, but does not by
itself establish which compositor window owns it. Applications without semantics
still have compositor metadata and visual interaction; text extraction is not
guaranteed. See [accessibility logging](../crates/hearthspace-compositor/src/accessibility.rs).

[Window grouping](../docs/WINDOW_GROUPS.md) already includes optional local-model
gathering based on titles. Richer context would extend what a decision can use;
it does not need to invent grouping or rearrangement again.

## Proposal and unresolved constraints

Explore an on-demand query for a bounded set of windows relevant to a user's
request. Prefer existing compositor metadata and available AT-SPI semantics.
An application could optionally publish richer context, such as its current
document, through a separately designed opt-in protocol. An MCP frontend could
wrap the existing control boundary if a concrete consumer needs it.

- Treat titles, app IDs, accessibility text, and any future application context
  as untrusted data. Titles and IDs can be missing or change; their contents
  cannot grant permission to inspect another app or perform an action.
- Preserve session ownership, lock restrictions, bounded requests, cancellation,
  and explicit errors at every transport. Define consent and data minimization
  before exposing document content or sending it to a model.
- Improve direct window-to-AT-SPI association before relying on semantic content
  to choose a target. Missing, ambiguous, or stale associations need a visible
  result, not a guessed match.
- Prefer on-demand collection without a persistent content index or background
  ingestion. This is a proposed scope, not an existing storage contract.
  Independent app updates prevent an atomic snapshot across windows; report
  partial results and revalidate targets before acting.
- Process file-descriptor inspection needs a separate privacy and ownership
  design. A process can serve several windows, and an open file does not establish
  what a window displays. It is not a guaranteed fallback or an implemented tier.
- OCR is outside this initial proposal. No claim about measured OCR accuracy is
  needed to keep the first design bounded.

Resource suspension is a separate [idle-lifecycle question](window-idle-lifecycle.md).
Collecting context must not silently thaw, freeze, or close an application.
