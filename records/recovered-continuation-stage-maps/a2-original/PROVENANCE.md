# Original A2 source overlay

Coherent non-generated A2 final source at `65dd354834c7d42936a14f9fcf99255c1ab7ce43`, to be applied after restored A1 final `b292e878`. Contains 20 repository-relative files under `source/` and excludes both generated outputs.

The immutable base `6359a9b` supplied unchanged preimages. Restored A1 supplied `Cargo.lock` and the compatibility ledger. Eight exact original per-file diffs were composed only after matching their historical Git blob preimages; every resulting blob matched its historical postimage prefix. Fifteen complete initial-stage exports were verified against their recovery checksums; duplicate representations had to agree exactly. Four complete original-blob-verified final-fix files then superseded their initial versions.

The two generated diffs were composed only in memory to record expected output hashes in `manifest.json`; no generated file was materialized. Run `just gen-bindings` in the restoration worktree to produce them. The original bad patch context in the paths snapshot was bypassed in favor of its verified complete final file.

All writes used fresh filesystem timestamps. No working tree, Git index or branch was changed. No historical commands, builds, dependency installs, code generation or tests were executed. Newly computed checksums establish copy integrity; historical evidence is recorded separately per file.
