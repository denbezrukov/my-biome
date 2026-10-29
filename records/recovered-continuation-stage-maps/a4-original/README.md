# Coherent A4 final source overlay

Copy only source/ into the continuation after A2 passes. The source is original A4 final3c28dae on base6359, with all 12 original existing-file diffs and all 4 fix transitions authenticated by their original pre/post Git blob prefixes. The two previously fragmented files were already independently reconstructed and authenticated as the fix preimages. Three other full sources are literal retained snapshots without an independently recorded original whole-file hash; their checksums establish copy integrity.

The overlay includes exactly17Rust source/test paths, no laterB-stage data or A3 service. It requires A2 package diagnostic metadata fields and existing resolver/graph dependencies, with no biome_project_analysis crate dependency. The original pre-trim changeset is intentionally excluded; assess the actual user-facing scope before choosing its release entry.

Fresh tests should execute28project graph tests and6resolver resolution tests, then the complete affected crates. Format/lint and fresh review remain necessary. No builds/tests or old command execution occurred while materializing this source. Files have fresh mtimes, and no product checkout, index, or branch was modified.
