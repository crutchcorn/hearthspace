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

See [vendored provenance and refresh steps](../vendor/accesskit_atspi_common/PROVENANCE.md).
