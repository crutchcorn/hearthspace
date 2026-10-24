# Vendored Masonry

Source: [linebender/xilem at `271a27a6d4a930f7878d404f9014e3c50a3a9b88`](https://github.com/linebender/xilem/tree/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry), version 0.4.0, Apache-2.0.

This directory preserves upstream `src/`, README, license, authors, and the
original manifest. The runtime changes are:

- [Portal progress repair](../../patches/masonry-portal-finite-progress.patch),
  described in [the patch ledger](../../patches/README.md#masonry-finite-scrollbar-geometry).
- [Button keyboard press ownership](../../patches/masonry-button-key-ownership.patch),
  described in [the patch ledger](../../patches/README.md#masonry-button-keyboard-press-ownership).

The standalone manifest resolves workspace package, dependency, and lint values
against that same revision. Example and benchmark targets and their assets are
omitted. Feature definitions and dependency kinds are preserved, including
`masonry_winit` as a development dependency referenced by renderer features.
This directory is excluded from the Hearthspace workspace.

To refresh, copy these files from a compatible upstream revision, regenerate
the standalone manifest from its workspace values, and reapply only the source
patches still needed. Check the source diff against the original revision;
only `src/widgets/portal.rs` and `src/widgets/button.rs` have functional changes.
Four upstream trailing whitespace lines in `doc/implementing_container_widget.md`,
`doc/vertical_stack.rs`, and `widgets/variable_label.rs` are normalized. Both source patches use zero context; apply them from the repository root
with `git apply --unidiff-zero`. Remove each patch when compatible unmodified upstream
passes its ledger's regressions. Remove this vendor directory and its Git-source
Cargo patch only after both repairs are available upstream. Refresh `Cargo.lock`
and retain the regressions until upstream coverage replaces them. The separate
Xilem and Masonry Winit patches have independent removal conditions.
