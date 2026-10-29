# Next native-analysis restoration stages

Read-only assessment, 2026-10-29. No source checkout, Git index, or branch was changed; no historical commands, builds, or tests were executed. Paths beginning `recovered-` below are relative to `/workspace/.biome-cloud/native-recovery/`. Original base is `6359a9b03670e85b50785d59d10eb087cf89fe3f`.

## A2 is ready for a coherent restoration after A1

Target original stage: `65dd354834c7d42936a14f9fcf99255c1ab7ce43`, built on A1 final `b292e878464503e93cba3f1d47a10be6e5e3dc41`. Recovery covers all 22 A2 paths. This adds disabled-by-default static configuration and lossless package/tsconfig metadata; it does not execute native analysis.

The eight existing-file preimages outside the A1-owned files independently match `6359` exactly. Two prerequisites depend on restored A1: `Cargo.lock` must have original blob prefix `119f104d79c`, and `xtask/project_analysis/compatibility.json` must have prefix `460b0b66be5`. Do not use the final mixed-stage assembly as the source for A2.

| Restore step | Exact source | Scope and checks |
| --- | --- | --- |
| 1. Restore integration | `recovered-a2-review/diffs/integration.patch` | Seven complete per-file diffs: root lock, configuration Cargo/lib, package lib, package module declarations, package_json.rs, tsconfig_json.rs. Each has original pre/post blob prefixes. Apply against matching preimages before copying overlapping full files. |
| 2. Restore complete files | `recovered-a2-review/source/` | 15 originals: configuration project_analysis module and tests, four fixture files, package project_metadata module and tests, package lib/module declarations, configuration Cargo, two snapshots, harness README, and changeset. Original paths are preserved below `source/`; copying the whole directory is sufficient after step 1. |
| 3. Restore corrected A2 final files | `recovered-a2-fix1-review/stage-65dd354834c7d42936a14f9fcf99255c1ab7ce43/crates/` | Four complete, independently original-blob-verified files supersede the initial versions; identities listed below. Copy these files, not the entire review packet. |
| 4. Restore capability ledger | `recovered-a2-review/diffs/compatibility.patch` | Complete original diff, `460b0b66be5` → `29dabdce1e4`; preserves the distinction between supported configuration and analysis not yet implemented. |
| 5. Regenerate bindings | `just gen-bindings` | Expected original output is captured by `recovered-a2-review/diffs/generated.patch`: `packages/@biomejs/backend-jsonrpc/src/workspace.ts` → `69ddc9c6db6`, `packages/@biomejs/biome/configuration_schema.json` → `47ee3c62088`. Use the diff as an identity oracle for generated artifacts; do not hand-author generated output. Both preimages match `6359`. |

Corrected final files, all rehashed independently during this assessment:

| Original path | Final Git blob |
| --- | --- |
| `crates/biome_package/src/node_js_package/tsconfig_json.rs` | `4a562b99c926e5fc2f12d3d00d0a428f79cd8513` |
| `crates/biome_package/tests/project_metadata.rs` | `780cb544673b869b95cedff9fe78509359936d2c` |
| `crates/biome_package/tests/tsconfig/valid/tsconfig.valid.baseUrl.json.snap` | `5b53b50eb26d3ae3b585c9da6e90700f03431ca9` |
| `crates/biome_package/tests/tsconfig/valid/tsconfig.valid.paths.json.snap` | `f1b8b1e17192496d4ba36dea0d1b3279aec9d24b` |

The `780cb...` file is the **test**, not `src/node_js_package/project_metadata.rs`. Earlier conversation summaries misnamed it. The retained fix patch also has one wrong context line in the last snapshot hunk; `recovery-consistency.json` documents it. The complete final snapshot matches its original blob and should be used directly.

The configuration module requires `biome_deserialize`'s `indexmap` feature and `indexmap` with serde support; the integration patch supplies them. Configuration tests need the added serde_json dev dependency. A2 does not depend on the later service coordinator, module-graph facts, engine analyses, or Node.

Proposed fresh verification sequence after restoring tests first:

```sh
source /workspace/.biome-cloud/activate.sh
cargo test -p biome_configuration --test project_analysis
cargo test -p biome_package --test project_metadata
just gen-bindings
cargo test -p biome_configuration -p biome_package -p biome_project_analysis
just f
just l
```

The focused configuration file contains 10 tests. The final package-metadata file contains 12 tests, including legacy diagnostic preservation and ranged tsconfig metadata diagnostics. The full package run checks the two restored tsconfig snapshots. Review any new snapshot differences individually rather than accepting them blindly. The historical logs are evidence of the old stage only; none of these gates has run in this assessment. Existing A2 changeset `perky-pumas-camp.md` correctly labels this as configuration preparation, with native integration pending.

## Smallest recoverable A3 increment: filesystem identity

Restore only the three biome_fs diffs from `recovered-a3/evidence/fs-and-contracts.diff.txt`, and copy `recovered-a3/full-source/canonical_path.rs` to `crates/biome_fs/tests/canonical_path.rs`. Exclude the engine contracts section of that combined diff from this increment; it assumes the larger A3 inventory changes.

The complete original filesystem behavior is self-contained:

| File | Original A3 postimage prefix |
| --- | --- |
| `crates/biome_fs/src/fs.rs` | `e28164560e0` |
| `crates/biome_fs/src/fs/memory.rs` | `1ad9fe5c53d` |
| `crates/biome_fs/src/fs/os.rs` | `d0186820ff0` |

It adds canonical path identity with explicit Unsupported/NotFound errors, OS alias resolution, and memory filesystem normalization without globally folding case. Tests cover memory case distinctions/missing paths, Unix directory aliases/symlink cycles, and Windows spelling. On Linux the focused file should execute two tests, not zero.

Run `cargo test -p biome_fs --test canonical_path`, then `cargo test -p biome_fs`, formatting/lint and review. This is an internal prerequisite and can be a separate small commit. Do not attach the whole original A3 user-facing changeset to this narrower restoration.

## Next independent A3 increment: workspace metadata reader

A2 already supplies `MetadataValue`. `biome_package` at `6359` already depends on the YAML parser/syntax and JSON crates, so no new dependency is needed.

- Start with the 244-line `recovered-a3/full-source/workspace_metadata.rs` and the complete two-test `workspace_metadata.tests.rs`.
- Apply only the literal source transformations recorded in inert `patches/002-yaml-lint-command.txt` and `patches/005-yaml-quotes-command.txt`; do not execute their archived command wrappers.
- Add the literal third regression test in `patches/003-yaml-quotes-test-append.rs.txt` (`workspace_metadata_preserves_escaped_quote_boundaries`).
- The module registration/re-export edits to package lib and node_js_package/mod were not recovered as original diffs. Reconstruct these small declarations explicitly and record them as new integration, not authenticated old bytes.
- Run `cargo test -p biome_package --test workspace_metadata`, then the entire package suite, formatting/lint and review.

The narrow reader is useful and testable independently of full project discovery. Complete A3 discovery cannot yet be represented as an exact original-stage restoration: inventory/tests and service registration/API edits have gaps. The final B6 inventory was recovered in full, but importing it into A3 introduces later configuration, catalog, script and snapshot assumptions.

## A4 can progress without waiting for the full A3 service

All 12 existing-file preimages in `recovered-a4/a4-01-resolver.complete.diff` and `a4-02-graph-and-resolver-tests.complete.diff` independently match `6359`. This is strong evidence that their source changes are independent of A3 service discovery. Resolver metadata quality depends on A2 package metadata; verify that dependency while assembling.

Complete A4 new sources are available: project_exports, dependencies and resolution in `recovered-a4/`; the collector and project tests were reassembled exactly in `assembly/derived/a4/` and can be advanced through the two complete fix diffs. Select the A4 versions from `assembly/coverage-manifest.json`, not the latest B4/B7 versions. Both final fix diffs are complete and carry original before/after blob identities. This is a promising next source-restoration batch once A2 is green; fresh resolver/module-graph tests still determine buildability.

## A5 remains the integration bottleneck

A5 reviewer recovered early engine facts/model/lib, service API/module_graph/coordinator/owned_facts, and all three correction diffs survive. However, service DB mutation/finalizer changes, watcher/workspace bridges, acquisition server hooks, and the original snapshot test baseline are incomplete. Do not claim full snapshot consistency from the recovered coordinator alone. Restore/test acquisition barriers and mutation hooks explicitly before connecting later engine/CLI/editor code.
