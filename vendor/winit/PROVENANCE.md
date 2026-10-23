# Vendored Winit 0.30

This directory contains the published [winit 0.30.13 crate](https://crates.io/crates/winit/0.30.13),
downloadable from [the crates.io archive](https://static.crates.io/crates/winit/winit-0.30.13.crate).
The archive SHA-256 is
`a6755fa58a9f8350bd1e472d4c3fcc25f824ec358933bba33306d0b63df5978d`,
matching the original Hearthspace lockfile. Its `.cargo_vcs_info.json` identifies
upstream revision
[`e9809ef54b18499bb4f2cac945719ecc2a61061b`](https://github.com/rust-windowing/winit/tree/e9809ef54b18499bb4f2cac945719ecc2a61061b).

All published files are preserved, including the normalized `Cargo.toml`,
`Cargo.toml.orig`, build script, runtime source, examples, tests, documentation,
Apache-2.0 `LICENSE`, and attribution notices. Only the package's standalone
`Cargo.lock` is omitted; the Hearthspace workspace lockfile controls resolution.
The package is excluded from workspace membership. Winit 0.31 used by Smithay
remains an unmodified registry dependency.

The sole source change backports
[`250cd01ae8d83a8969265a14496fb6957b44a34e`](https://github.com/rust-windowing/winit/commit/250cd01ae8d83a8969265a14496fb6957b44a34e),
merged in [upstream PR 4760](https://github.com/rust-windowing/winit/pull/4760)
on 2026-10-15. Window cleanup removes its queued compositor updates before
reporting `Destroyed`. The backport adapts the upstream owned ID `w` to the
0.30 event loop's borrowed `window_id`; it does not change event timing or APIs.
The upstream changelog addition is represented by this provenance and
[the patch ledger](../../patches/README.md#winit-wayland-destroyed-window-updates).
The exact source delta is in
[`winit-wayland-destroyed-updates.patch`](../../patches/winit-wayland-destroyed-updates.patch).

To refresh, prefer a compatible published release containing the upstream fix.
Verify its archive checksum, replace the imported files without reformatting or
refactoring them, and retain only patches still required. Refresh the workspace
lockfile and rerun the real Wayland create/drop regression and native layout
editor drag workflow. Once a compatible release passes both without this patch,
remove the Cargo override, vendor directory, patch file, and ledger entry.
