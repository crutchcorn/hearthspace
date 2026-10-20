# Vendored Smithay

Source: [Smithay at `5c2ba4132556a08abc70ae1771d275ec2cfc031f`](https://github.com/Smithay/smithay/tree/5c2ba4132556a08abc70ae1771d275ec2cfc031f),
package version 0.7.0, MIT. The upstream notice is preserved in `LICENSE.txt`.

This directory contains upstream `src/`, `build.rs`, the two GBM C feature probes,
README, license, and original manifest. The only runtime source change is the
[XSettings flush patch](../../patches/smithay-xsettings-flush.patch): after
queuing the settings property, flush the X11 connection so idle clients receive
the change. Initialization, set, remove, and clear share that publication path.
It adds no X11 round trip or public API.

`Cargo.toml.orig` is the exact upstream manifest. The standalone `Cargo.toml`
omits upstream workspace members, examples, benchmarks, development-only
dependencies, and the custom release profile; target autodiscovery is disabled.
Runtime/build dependencies, features, and package identity remain unchanged.
Hearthspace's dependency stays pinned to the original Git revision, with the
workspace Cargo patch selecting this copy.

To refresh:

1. Copy these same files from a compatible upstream revision and retain its
   original manifest and notices.
2. Normalize the standalone manifest as described above, preserving all runtime
   dependencies and features.
3. Reapply only the source patch if still needed. Follow the verification and
   removal criteria in the [patch ledger](../../patches/README.md#smithay-xsettings-publication).
4. Refresh `Cargo.lock` and run the idle-X11 regression and live GTK3 appearance
   check before removing the patch or changing the pinned revision.
