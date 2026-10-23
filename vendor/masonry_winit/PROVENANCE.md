# Vendored masonry_winit

Source: [linebender/xilem at `271a27a6d4a930f7878d404f9014e3c50a3a9b88`](https://github.com/linebender/xilem/tree/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry_winit), Apache-2.0.

This directory contains the runtime `src/`, upstream README/license, and original
manifest with two focused local patches:

- [Initial window properties](../../patches/README.md#xilem-initial-window-properties).
- [Retired-window presentation](../../patches/README.md#masonry-retired-window-presentation).

The build manifest resolves upstream workspace values against the same revision;
examples, stress tests, and their development-only dependencies are omitted.
Other Linebender crates remain pinned upstream Git dependencies.

To refresh, copy these files from a compatible upstream revision, regenerate
the standalone manifest from that revision's workspace values, and reapply only
the still-needed [initial-properties patch](../../patches/xilem-initial-window-properties.patch)
and [lifecycle patch](../../patches/masonry-retired-window-presentation.patch).
Remove this vendored package and its Git-source Cargo patch when upstream passes
both the first-paint and retired-window regressions plus live shell checks.
The separate vendored Xilem package can be removed when its initial-properties
repair is no longer needed.
