# A4 independent restoration assessment

The coherent A4 resolver/module-graph subunit can be assembled after A2 without importing the incomplete A3 service. Fresh builds/tests remain necessary; this is source-dependency assessment only.

All 12 existing-file preimages in the two original A4 diff groups match base6359 byte-for-byte by recorded Git blob prefixes. `A4-PREIMAGE-CHECKS.json` records each check. A1/A2 do not modify these 12 paths. The A3 filesystem identity increment is useful but is not referenced by the recovered A4 source; A4 can coexist with it without a direct prerequisite.

## Prerequisites

A4 resolver additions consume the A2 package metadata fields:

- `PackageJson.project_metadata.diagnostics`, via `ResolverManifest for PackageJson`.
- `TsConfigJson.metadata_diagnostics`, via `ResolverManifest for TsConfigJson`.

Those are the only newly introduced upstream project-analysis interfaces found in A4 source. The collector/export/resolution facts belong to biome_module_graph; there is no biome_project_analysis, configuration, service, workspace inventory or Node dependency in the recovered A4 scope. No additional engine fields or A3 engine contracts are prerequisites. Existing module-graph/resolver Cargo dependencies provide all imports; original A4 has no Cargo changes.

## Exact assembly sequence

1. Apply recovered-a4/a4-01-resolver.complete.diff and a4-02-graph-and-resolver-tests.complete.diff to the matching base versions, checking their recorded pre/post blob identities.
2. Copy `a4-project_exports.complete.rs` to `crates/biome_module_graph/src/db/queries/project_exports.rs`, `a4-dependencies.complete.rs` to `src/db/queries/dependencies.rs`, and `a4-resolution.complete.rs` to `src/resolution.rs` in the same crate.
3. Use the main-stage collector and test at `assembly/derived/a4/crates/biome_module_graph/src/project_facts.rs` and `tests/spec_tests/project.test.rs`. Their complete original pre-fix blobs are authenticated: `aaeec90614dca91b9171126a729ef61afd77623a` and `fe70beeca51222f98204343f01e003e5a6d0f9f5`. Do not copy the latest mixed-stage versions.
4. Apply `fix1-01-production.complete.diff` and `fix1-02-tests.complete.diff`, again checking every pre/post original blob. This advances the two reconstructed files plus resolver lib/tests to final A4 `3c28dae197840dd6adee956e7d884531a073f7cc`.

This accounts for 17 Rust paths. The eighteenth original path was a changeset: its preserved body is explicitly pre-trim, with the exact trim operation retained separately. Reassess user-facing scope for the actual restored batch; do not silently copy the pre-trim body as final.

## Fresh verification targets

Run focused tests before the full suites:

- `cargo test -p biome_module_graph --test spec_tests project_` — 18 main project tests plus 10 final fix regressions should execute (28, not zero).
- `cargo test -p biome_resolver --test spec_tests resolution_` — four main regression tests plus two final fix regressions should execute (6, not zero).
- `cargo test -p biome_module_graph -p biome_resolver` — includes existing resolver/graph behavior.
- Repository formatting/lint/review gates before commit.

The main tests exercise repeated runtime/type imports, declaration identity, exports/star resolution, CommonJS mutation uncertainty, custom elements, condition matrices and metadata repair. Final fixes cover import-type occurrences, opposite-facet shadowing, escaped import/export/member names, unsupported namespace escape, and relative/absolute/declaration source mappings. Restoring and passing these does not establish end-to-end project analysis or A5 snapshot consistency.

No broader source was materialized or executed during this assessment.
