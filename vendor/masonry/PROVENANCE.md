# Vendored Masonry

Source: [linebender/xilem at `271a27a6d4a930f7878d404f9014e3c50a3a9b88`](https://github.com/linebender/xilem/tree/271a27a6d4a930f7878d404f9014e3c50a3a9b88/masonry), version 0.4.0, Apache-2.0.

This directory preserves upstream `src/`, README, license, authors, and the
original manifest. The sole runtime change is the
[Portal progress repair](../../patches/masonry-portal-finite-progress.patch),
described in [the patch ledger](../../patches/README.md#masonry-finite-scrollbar-geometry).
The standalone manifest resolves workspace package, dependency, and lint values
against that same revision. Example and benchmark targets and their assets are
omitted. Feature definitions and dependency kinds are preserved, including
`masonry_winit` as a development dependency referenced by renderer features.
This directory is excluded from the Hearthspace workspace.

To refresh, copy these files from a compatible upstream revision, regenerate
the standalone manifest from its workspace values, and reapply only the source
patch if still needed. Check the source diff against the original revision;
only `src/widgets/portal.rs` has a functional change. Four upstream trailing
whitespace lines in `doc/implementing_container_widget.md`,
`doc/vertical_stack.rs`, and `widgets/variable_label.rs` are normalized. The
functional source patch uses zero context; apply it from the
repository root with `git apply --unidiff-zero`. Remove this vendor directory and its
Git-source Cargo patch when unmodified upstream passes the focus-reveal/resize
regression and native repeated tray-menu acceptance. Refresh `Cargo.lock` and
retain the regression until upstream coverage replaces it. The separate Xilem
and Masonry Winit patches have independent removal conditions.
