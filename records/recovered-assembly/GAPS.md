# Current recovery gaps

This is an observed-path inventory, not a complete historical diff. No missing source has been invented. Exact-source gaps remain recoverable if another original transcript contains their bytes; they are not declared irretrievable.

## later known changes missing

- `Cargo.lock` — latest complete source: exact diff postimage from recovered-b1/patches/00-main-lock.diff
- `Cargo.toml` — latest complete source: exact diff postimage from recovered-a1/diffs/initial-Cargo.diff.txt
- `crates/biome_module_graph/src/project_facts.rs` — latest complete source: exact diff postimage from recovered-b4/patches/00-main-integration.patch
- `crates/biome_module_graph/tests/spec_tests/project.test.rs` — latest complete source: exact diff postimage from recovered-a4/fix1-02-tests.complete.diff
- `crates/biome_project_analysis/src/catalogs.rs` — latest complete source: e2dcace
- `crates/biome_project_analysis/src/dependencies.rs` — latest complete source: 74f3d041
- `crates/biome_project_analysis/src/exports.rs` — latest complete source: 20104d6
- `crates/biome_project_analysis/src/members.rs` — latest complete source: 20104d6
- `crates/biome_project_analysis/src/policy.rs` — latest complete source: 20104d6
- `crates/biome_project_analysis/src/reachability.rs` — latest complete source: a62fb487
- `crates/biome_project_analysis/src/resolution.rs` — latest complete source: 20104d6
- `crates/biome_project_analysis/src/scripts.rs` — latest complete source: 185e9089
- `crates/biome_project_analysis/tests/contracts.rs` — latest complete source: exact diff postimage from recovered-a3/evidence/fs-and-contracts.diff.txt
- `crates/biome_service/Cargo.toml` — latest complete source: exact diff postimage from recovered-a3/evidence/cargo-and-final-normalize.diff.txt
- `crates/biome_service/src/project_analysis/inventory.rs` — latest complete source: e2dcace
- `crates/biome_service/src/project_analysis/owned_facts.rs` — latest complete source: 07d3c3e7
- `packages/@biomejs/backend-jsonrpc/src/workspace.ts` — latest complete source: exact diff postimage from recovered-a2-review/diffs/generated.patch
- `xtask/project_analysis/README.md` — latest complete source: exact diff postimage from recovered-b1/patches/00-main-conformance-description.diff
- `xtask/project_analysis/compatibility.json` — latest complete source: exact diff postimage from recovered-b1/patches/00-main-conformance-description.diff
- `xtask/project_analysis/src/conformance.rs` — latest complete source: 3d051362 (A1 review; use exact final fix where changed)

## missing complete source

- `.changeset/smooth-owls-show.md`
- `crates/biome_cli/src/cli_options.rs`
- `crates/biome_cli/src/commands/mod.rs`
- `crates/biome_cli/src/lib.rs`
- `crates/biome_cli/src/reporter/mod.rs`
- `crates/biome_cli/src/reporter/project_analysis/compatibility.rs`
- `crates/biome_cli/src/reporter/project_analysis/explain.rs`
- `crates/biome_cli/src/runner/mod.rs`
- `crates/biome_cli/tests/cases/mod.rs`
- `crates/biome_cli/tests/cases/reporter_project_analysis.rs`
- `crates/biome_cli/tests/fixtures/project_reporters/cycle-presentation.json`
- `crates/biome_cli/tests/fixtures/project_reporters/fixtures.json`
- `crates/biome_cli/tests/fixtures/project_reporters/oracle.json`
- `crates/biome_cli/tests/snapshots/main_cases_help/check_help.snap`
- `crates/biome_cli/tests/snapshots/main_commands_analyze/analyze_help_local.snap`
- `crates/biome_project_analysis/src/cycles.rs`
- `crates/biome_project_analysis/tests/exports.rs`
- `crates/biome_project_analysis/tests/fixtures/duplicate_binding.json`
- `crates/biome_project_analysis/tests/fixtures/duplicate_spelling.json`
- `crates/biome_service/src/project_analysis/fixtures/members-oracle.json`
- `crates/biome_service/src/project_analysis/inventory.tests.rs`
- `crates/biome_service/src/project_analysis/provenance.rs`
- `crates/biome_service/src/project_analysis/rpc.tests.rs`
- `crates/biome_service/src/project_analysis/snapshot.tests.rs`
- `crates/biome_service/src/scanner/workspace_bridges.rs`
- `crates/biome_service/src/workspace.rs`
- `crates/biome_service/src/workspace/client.rs`
- `crates/biome_service/src/workspace/server.rs`
- `crates/biome_service/src/workspace_types.rs`
- `packages/@biomejs/backend-jsonrpc/tests/workspace.test.mjs`
- `packages/@biomejs/js-api/tests/project-analysis.test.ts`
- `xtask/project_analysis/REPORTING.md`
- `xtask/project_analysis/src/compare.rs`
- `xtask/project_analysis/src/normalize.rs`

## Mechanical regeneration limits

All composed diffs require matching original preimage and postimage blob identities. Missing generated TS/schema bindings may be regenerated only after full matching Rust input contracts are restored; that would be new generation, and original identity still needs hash comparison. Missing fixture/lock/reference inventories require their retained literal inputs and immutable source versions. Algorithms, service plumbing and reporter bodies cannot be reconstructed as original bytes from an API summary.
