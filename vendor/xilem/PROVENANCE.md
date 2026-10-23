# Vendored xilem

Source: [linebender/xilem at `271a27a6d4a930f7878d404f9014e3c50a3a9b88`](https://github.com/linebender/xilem/tree/271a27a6d4a930f7878d404f9014e3c50a3a9b88/xilem), Apache-2.0.

This directory contains the unchanged runtime `src/`, upstream README/license,
and original manifest plus the focused initial-window-properties patch described
in [the patch ledger](../../patches/README.md#xilem-initial-window-properties).
The build manifest resolves upstream workspace values against the same revision;
examples, stress tests, and their development-only dependencies are omitted.
Other Linebender crates remain pinned upstream Git dependencies.

To refresh, copy these files from a compatible upstream revision, regenerate
the standalone manifest from that revision's workspace values, and reapply only
[the source patch](../../patches/xilem-initial-window-properties.patch) if still
needed. Remove both vendored packages and the Git-source Cargo patches when the
upstream creation path passes the first-paint regression and live shell checks.
