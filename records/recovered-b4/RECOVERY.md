# B4 literal history recovery

Recovery performed on 2026-10-29 by original implementer `/root/project_b4_implement`. This archive contains text copied from retained conversation tool outputs, not a rerun or reimplementation. No source file in `/workspace/my-biome` was read or modified during recovery. No project/build/install/format/lint/oracle command was run. Recovery-only Python handled copying, hashing and patch-text validation.

Original identity/order:

1. B4 base: `43c453530573278cfcdae0b78c1195ba3a99262b`.
2. Main B4: `3d400313de4fd936b2a290e7e9f088a3f62df118`, subject `feat(project): extract static script and binary usage`.
3. Final fix round 1: `185e908965a0c375a404b99ff3d42d5feaa6aec0`, subject `fix(project): preserve script context and parser bounds`.

## Complete files

`main/` paths are repository-relative complete files from the main B4 candidate. All seven files under main are checked against the independently retained original 31-file SHA-256 manifest:

- crates/biome_module_graph/src/project_commands.rs
- crates/biome_project_analysis/tests/scripts.rs
- crates/biome_service/src/project_analysis/scripts.rs
- .changeset/free-bushes-refuse.md
- xtask/project_analysis/fixtures/scripts/README.md
- xtask/project_analysis/fixtures/scripts/comparison.json
- xtask/project_analysis/fixtures/scripts/command-comparisons.json

The service production script file, changeset and corpus comparison files are unchanged by fix round 1.

`final/crates/biome_module_graph/src/project_commands.rs` is the complete literal post-format source read retained before final commit. Its original final SHA-256 was not printed individually in the retained history. The original main version is independently hash-verified. The derived final collector patch is computed only from these two literal recovered files and is clearly labeled derived.

`final/crates/biome_project_analysis/tests/scripts.rs` is assembled mechanically from the complete hash-verified main file plus the exact final added lines in `02-final-fix1-tests.patch`. Its Git blob ID matches the original final diff prefix `f35bd3eaad0`; full recovered blob SHA-1 is `f35bd3eaad0b6759095f2105ac1b3ab62f7c4c81`.

## Exact patches and partial source

- `patches/00-main-integration.patch`: literal main-stage git diff for module graph project_facts, engine exports/reachability, and service snapshot integration. Apply only against the original B4 base, after restoring its dependencies. This is a subset of the main commit, not its entire patch.
- `patches/01-fix1-engine.patch`: literal fix-round engine diff, main to final. Originally observed immediately before formatting; the later formatter did not change this engine delta. All hunk counts match the retained headers. This patch does not supply missing main engine source.
- `patches/02-final-fix1-tests.patch`: literal final formatted diff for graph, owned and service regression tests, main to final.
- `partial/engine-scripts.main.lines1-240.rs.txt` and `partial/engine-scripts.main.lines334-680.rs.txt`: exact disjoint main engine source reads. Main engine file has 1048 lines; lines 241–333 and 681–1048 are missing here. The second excerpt intentionally ends at the `fn on_demand(` signature.
- `partial/service-scripts.tests.main.lines1-125.rs.txt`: exact first 125 lines of the 846-line main source integration test file, ending at a test signature. Lines 126–846 are missing here; final additional tests are separately preserved by the exact patch.
- `partial/graph-project.test.main.b4-final49lines.rs.txt`: exact B4-specific tail tests from a retained source read. The name describes the original 49-line addition including its preceding separator; this excerpt begins at its first attribute and omits the preceding blank line. It does not replace the full graph test file. Final extra test is preserved by the exact patch.

No missing code has been filled in. In particular, no full `crates/biome_service/src/project_analysis/inventory.rs` source is visible in this agent's retained context. No exact engine inventory.rs/model.rs/lib.rs/normalize.rs or service owned_facts.rs/registration diff is visible here beyond summaries; these were not synthesized. The retained source summaries are not substituted for code.

## Original evidence and hashes

`original-main-frozen-files.sha256` is the exact 31-file original manifest, SHA-256 `a4fc017eacf6dc2506bb18af411f02bb0d8c10f35724a735707869b807d9b8a2`. It includes missing-file hashes for later recovery validation; presence there does not mean the file is recovered.

The original final five-file fix manifest's content is not printed in retained history; its known SHA-256 is `4b145db598a6ff751ba18ac9328777f28ec26506f8aecbfd32f85a605af0e650`. Do not mistake the new recovery manifest for that original.

`evidence/35-review-all-red.log` is the complete retained literal old RED log. `38-review-affected-suites.results.excerpt.txt` contains only the literal old result lines, not the full log. The final lint/commit and report-hash tool-output excerpts are likewise old retained outputs, not recovery-time gate results. Original final suite: 895 passed, 0 failed, 10 ignored; this is historical evidence only. Original final report SHA-256: `3963493bed9201a944a4841c2bd713d9fad542836ff0db997435e2d6fe8e3f6d`.

The original reference observations/input bytes are not present in this worker's visible text and have not been recreated from native expectations. Both native comparison JSON files are full and original-hash-verified. No parity, new oracle result, build pass or restored complete project is claimed.

`recovery-manifest.json` records recovery-time hashes/sizes and selected original-hash comparisons for every archived file except itself. It is a new inventory, not original project evidence.
