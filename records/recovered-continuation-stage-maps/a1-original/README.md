# Coherent A1 source overlay

This directory materializes all 51 product paths of original A1 final b292e878464503e93cba3f1d47a10be6e5e3dc41 on base6359a9b03670e85b50785d59d10eb087cf89fe3f. Copy only `source/` into an isolated checkout based on that original base. Do not copy the later mixed-stage recovery assembly.

The result.rs evidence identities are reconstructed from exact retained replacements. normalize.rs and the initial contracts snapshot already contain those earlier changes and must not receive them twice. contracts.rs then receives the final ownership fix. Full reviewed artifact/conformance source includes the documented clippy/test-order changes. Later B/C files are excluded.

The root Cargo files and final contracts/test inventory match retained original Git blob prefixes. The npm lock and license match retained original SHA-256. Reviewer duplicates match all corresponding materialized paths. Other source snapshots have provenance but no independent historical whole-file hash; `manifest.json` makes this distinction explicit. Three fixture-group digests are separately authenticated by the original review recovery.

No archived command executed, no product checkout/index/branch modification, no build/install/network/test run. The parent must format, test, inspect and review the coherent overlay before treating it as a working continuation. Historical observation and validation files are historical records only.
