# Vendored AccessKit AT-SPI core

This directory contains the source of `accesskit_atspi_common` **0.18.1** from
[crates.io](https://crates.io/crates/accesskit_atspi_common/0.18.1). Its package
metadata identifies upstream revision
[`f40dfc01a0c0e76de535969f82fb35e19513737d`](https://github.com/AccessKit/accesskit/tree/f40dfc01a0c0e76de535969f82fb35e19513737d/platforms/atspi-common).

Imported files are the published `src/` directory, normalized `Cargo.toml`,
`Cargo.toml.orig`, `.cargo_vcs_info.json`, README, and changelog. The upstream root
`LICENSE-APACHE` and `LICENSE-MIT` files were copied from that exact revision;
source copyright notices are preserved. Cargo registry bookkeeping and the
package's standalone lockfile are omitted. The Hearthspace workspace lockfile
controls dependency resolution, and this directory is excluded from workspace
membership.

Local source changes are:

- Sorted adapter insertion in `src/context.rs`, backported from upstream
  [`fb7efa8cdbe6ebb48106f30c1c66163ebc781b49`](https://github.com/AccessKit/accesskit/commit/fb7efa8cdbe6ebb48106f30c1c66163ebc781b49).
- Clipped-node registration updates in `src/adapter.rs` and the local
  `src/adapter/filter_updates.rs` module. When an ancestor's geometry changes,
  unchanged descendants can enter or leave the exposed accessibility tree.
  Revisit the affected clipping scopes, including neighboring nodes, so their
  platform interfaces and child-list events stay consistent. Transparent
  containers must not bypass an excluded ancestor.

Exact deltas, upstream investigation, regressions, and removal conditions are
tracked in the [patch ledger](../../patches/README.md).

To refresh, first check for a compatible upstream release containing the fix.
Prefer removing the override when that release can pass the regression. If the
override is still required, replace the imported files with the chosen published
package, preserve the license files from its recorded upstream revision, reapply
only the documented patches still needed, update this provenance and the ledger,
and refresh `Cargo.lock`. Run the adapter-order and clipped-node regressions and the real shell
acceptance with AT-SPI enabled. Do not reformat or refactor upstream files as part
of the refresh.
