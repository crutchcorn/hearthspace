# Dependency patches

## AccessKit AT-SPI adapter ordering

- **Dependency:** `accesskit_atspi_common` 0.18.1, vendored in
  [`vendor/accesskit_atspi_common`](../vendor/accesskit_atspi_common/).
- **Patch:** [keep adapters sorted by ID](accesskit-atspi-adapter-order.patch),
  backported from upstream commit
  [`fb7efa8cdbe6ebb48106f30c1c66163ebc781b49`](https://github.com/AccessKit/accesskit/commit/fb7efa8cdbe6ebb48106f30c1c66163ebc781b49).
- **Reason:** the four shell corners create separate Wayland windows. When AT-SPI
  activates, their first redraws can arrive out of creation order. AccessKit
  appended adapters to a vector but used binary search to find them, causing a
  panic followed by an abort during cleanup. Sorted insertion preserves that
  lookup invariant without changing shell window or accessibility behavior.
- **Upstream:** [issue 820](https://github.com/AccessKit/accesskit/issues/820) and
  [merged fix 821](https://github.com/AccessKit/accesskit/pull/821). As of
  2026-10-17, the latest compatible release is 0.18.1, and the fix has not shipped
  in a release. Only the `atspi-common` change is needed here: Winit creates
  adapters on the same event-loop thread, while their deferred activation order
  varies. The upstream Unix message-queue change addresses concurrent creation.
- **Verification:**
  `cargo test -p hearthspace-shell --test accesskit_adapter_order` registers
  windows in order 2, 0, 1 and checks accessibility enumeration, updates, removal,
  and reactivation. It fails against the original 0.18.1 implementation. Shell
  acceptance additionally exercises the real private AT-SPI bus.
- **Removal:** upgrade the AccessKit version used by Xilem/Masonry to a compatible
  upstream release containing this fix. Remove the Cargo patch, vendored package,
  and patch file, refresh `Cargo.lock`, and rerun the regression and shell
  acceptance with accessibility enabled before removing this ledger entry.

## AccessKit clipped-node registration

- **Dependency:** the same vendored `accesskit_atspi_common` 0.18.1.
- **Patch:** [refresh clipping-dependent registrations](accesskit-atspi-clipped-nodes.patch),
  a local fix in `src/adapter.rs` and `src/adapter/filter_updates.rs`.
- **Reason:** at 4× scale, Background settings initially places Photo fit and its
  explanatory text below the viewport. Scrolling changes a container transform
  while the descendants' node data stays unchanged. AccessKit's visibility filter
  then exposes those descendants through `GetChildren`, but their D-Bus objects
  remain unregistered because they received no node-update callback. Clients see
  `UnknownObject` for visible controls.
- **Behavior:** when geometry or filtering topology changes, revisit the old and
  new clipping scopes, including siblings that may gain the filter's offscreen
  neighbor exception. Process descendants before parent child-list events and
  deduplicate node updates. Match actual exposed ancestry through transparent
  containers. Ordinary value/text updates do not trigger a scope traversal; the
  extra work is limited to affected clipping scopes, with no persistent cache
  or public API change.
- **Upstream:** checked on 2026-10-20. The
  [package releases](https://crates.io/crates/accesskit_atspi_common/versions)
  still have 0.18.1 as the latest 0.18.x release. Current
  [adapter source at `a39329d`](https://github.com/AccessKit/accesskit/blob/a39329d281a9381510496c17e81e87763edbb8bb/adapters/atspi-common/src/adapter.rs)
  retains the same callback-only registration logic. The clipping filter was
  introduced in [PR 574](https://github.com/AccessKit/accesskit/pull/574).
  No existing issue or fix for this specific registration gap was found; no
  upstream report has been published.
- **Verification:**
  `cargo test -p hearthspace-shell --test accesskit_clipped_nodes --test accesskit_adapter_order`
  checks entering/leaving view after ancestor scrolling, viewport resize,
  sibling movement/reordering, nested clips, and simultaneous updates. It also
  checks balanced interfaces and registration before `ChildAdded` events.
  The initial three clipping regressions fail against unpatched 0.18.1.
  Validate the real Settings Photo fit control through the private AT-SPI bus
  after scrolling at 4× scale.
- **Removal:** remove this patch after a compatible upstream release passes
  these clipping regressions and the real 4× Settings workflow without it.
  Preserve the separate adapter-order patch until its own removal condition is
  met.

See [vendored provenance and refresh steps](../vendor/accesskit_atspi_common/PROVENANCE.md).
