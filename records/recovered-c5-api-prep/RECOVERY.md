# Original C5 API preparation source recovery

Exports transcribe literal source retained in this helper's original conversation. No rebuilt product source was read. No Git historical command, checkout/index change, package execution, formatter, build, test or subagent was used during recovery.

The original preparation finally bound `e645266862c389ffaae3c7e72aeac7b6e02b162d`, which was a C4 fix candidate under independent scoped review. There was no C5 product implementation in this helper: its work produced an API map and hash manifest only.

Complete priority files recovered:

- `crates/biome_project_analysis/src/lib.rs`: original 237 lines; read at d574 and hash confirmed unchanged at e645. Export matches retained original SHA-256.
- `crates/biome_project_analysis/src/result.rs`: complete original 167 lines from precommit C4 reading, confirmed unchanged in both subsequent original 37-input comparisons. Parent independently authenticated the original 5430-byte SHA-256 using the recovered original C4 manifest; export matches `0f199a13761e2994ccc177e7e004b3ec98c9c4b97eda5bcf5d509f3acf37c5ef`.
- `crates/biome_project_analysis/src/options.rs`: complete original 200 lines, likewise unchanged in original d574/e645 comparisons; original SHA-256 was not printed.
- `crates/biome_project_analysis/src/report_metadata.rs`: original 405 lines; d574 complete reading, unchanged at e645. Export matches retained original SHA-256.
- `crates/biome_service/src/project_analysis/policy.rs`: original 424 lines; d574 complete reading, unchanged at e645. Export matches retained original SHA-256.
- `crates/biome_cli/src/reporter/project_analysis/mod.rs`: original 234 lines; d574 complete reading, unchanged at e645. Export matches retained original SHA-256.
- `crates/biome_cli/src/reporter/project_analysis/native.rs`: original 13 lines; d574 complete reading, unchanged at e645. Export matches retained original SHA-256.

Explicit gaps:

- Engine `policy.rs` was not read in full in this helper's original scope; no literal implementation is available here. Recover it from its original owner rather than derive it from references in exported options/service code.
- Service `provenance.rs` was not read in full; no implementation is available here.
- There was no separately inspected `request.rs`: `ProjectAnalysisRequest` was declared in service `project_analysis.rs`, with option translation in the recovered service `project_analysis/policy.rs`. Another helper is recovering `project_analysis.rs`; it is intentionally not duplicated here.
- CLI `compatibility.rs` was not read in full; no implementation is available here.
- CLI `explain.rs` was only seen via an exact line-25 search result during provisional C4 work on HEAD `12bf926317b7032e2ddc46124bf277b8409bf5fd`. `explain.rs.precommit-line25.fragment` preserves that one original line verbatim. Lines 1–24 and 26 onward are missing; this fragment is not a full e645 file, and formatting/final content was never independently bound by this helper.

Original preparation artifacts are absent at their former paths in this replacement workspace. `original-artifact-receipts.json` preserves the exact original final report/manifest SHA-256 values previously printed in the original conversation. These are receipts, not reconstructed full artifacts. `recovery-manifest.json` records export SHA-256 values, stages and original-hash matches.

Additional complete original engine files exported: `normalize.rs` (178 lines), `model.rs` (168), `inventory.rs` (199), and `facts.rs` (325). Their original hashes were not printed in retained history; every byte was transcribed from complete original tool output. The recovered C4 committed-files manifest contains only C4 changes, so it does not independently authenticate these unchanged older files or `options.rs`. The parent-authenticated original C4 manifest is preserved under `original-artifacts/`; six exports match its exact whole-file hashes.
