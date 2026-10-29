# B2 exact-text recovery inventory

This archive contains only text available verbatim in the surviving agent conversation. No recovered command was executed. No repository file was inspected or changed during recovery. Missing source has not been inferred from implementation summaries.

Original repository: `/workspace/my-biome`, branch `feat/native-project-analysis`.

- B2 base: `df22208cc013659d537dc12e6d95cbf3c25ba11c`.
- Main B2 commit: `d1722ea71d1ade0ce92d61d3eed533aa0d839808`, `feat(project): analyze exported values and types`.
- Final fix-round commit: `990a42e751d276e8b403a0de8d099864dea54bf0`, `fix(project): retain uncertain import and registration uses`.

## Exact retained source artifacts

1. `stages/d1722ea7/crates/biome_service/src/project_analysis/fixtures/README.md`: complete original file, from a full `cat` output before the final precision-limit append. This is the only complete Rust-adjacent source/documentation file retained here; no complete production Rust file survives in current context.
2. `stages/fix1-pre-import-ruling/production.diff`: exact historical unified diff against d1722ea7 for the changeset and four production files, after the redundant-reference lint fix and before exclusion of imported constructor terminals. This is an intermediate stage, not the final commit diff. It requires the missing d1722ea7 source baseline.
3. `stages/fix1-pre-import-ruling/exports-tests.diff`: exact historical unified diff against d1722ea7 adding four tests (210 added lines). The imported-constructor test was added later. It requires the missing d1722ea7 complete tests baseline.
4. `stages/fix1-final/import-terminal-edit.py.txt`: exact historical edit command for the final constructor-kind exclusion. Inert transcript, not executed during recovery. Apply only after the preceding production diff if reconstructing elsewhere.
5. `stages/fix1-final/crates/biome_service/src/project_analysis/exports.tests.rs.fragment`: exact final imported-constructor test text from the final source read. This is an append fragment, not a complete file or unified diff.
6. `stages/fix1-final/fixture-readme.diff`: exact final README append diff against d1722ea7. This applies to artifact 1, independently of the Rust patches.

Patch order: start from d1722ea7, apply artifacts 2 and 3 (disjoint files), apply artifact 4's recorded replacement to its named source file, append artifact 5 to exports.tests.rs with the original blank line separating test functions, and apply artifact 6 to artifact 1. Artifact 5 selects the complete function and excludes its surrounding blank-line separator. This order is provenance documentation only; recovery has not performed these operations or verified an assembled tree.

## Missing material and limits

- The complete df22208..d1722ea7 implementation diff is absent from current retained context. Summaries describe it but do not supply recoverable source.
- No complete latest production Rust file survives; `project_facts.rs`, engine `exports.rs`/`facts.rs`, and service `owned_facts.rs` are represented by exact fix diffs only.
- The complete original 36-test exports.tests.rs baseline, complete engine owned tests/fixtures, original 40-case oracle JSON, graph query/snapshot changes, and complete task report are absent. Selected baseline test excerpts are listed below. No substitutes are generated.
- Historical final-source manifests were generated and verified, but their complete per-file SHA-256 values were not printed in surviving context. The archive does not invent those hashes or claim a reconstructed tree matches them.
- Git `index` values inside recovered diffs are retained exactly as printed; they identify that historical diff stage, not necessarily final blobs.
- Reported historical verification (not rerun): final graph/service 798 passed, unchanged engine 32 retained, combined 830 passed, 0 failed, 10 existing ignored; 41 B2 source tests; 40 oracle projections unchanged. These are historical claims, not validation of this recovery archive.

The parent owns any application/checkpointing to `recovery/native-project-analysis-history`. This archive does not change `/workspace/my-biome`, install tools, run tests, or execute source/edit transcripts.

## Additional exact source excerpts

`excerpts/d1722ea7/` preserves the original file paths with `.fragment` suffixes. These are contiguous excerpts selected from actual historical tool output, not reconstructed complete files:

- `crates/biome_module_graph/src/project_facts.rs.lines-1-220.fragment`: syntax fact definitions and imports before fix round 1.
- `crates/biome_module_graph/src/project_facts.rs.registration_identity.fragment`: complete original registration-identity function before fix round 1.
- `crates/biome_project_analysis/src/exports.rs.lines-135-310.fragment`: import/registration consumption, beginning and ending inside surrounding code.
- `crates/biome_project_analysis/src/exports.rs.output-through-evidence-prefix.fragment`: output normalization, surface/workspace helpers, Consumption fields and namespace uses, ending inside evidence generation.
- `crates/biome_project_analysis/src/exports.rs.select.fragment`: complete original select method.
- `crates/biome_project_analysis/src/exports.rs.lines-808-875.fragment`: unknown-namespace method and following reporting_uncertain signature; the latter's body is missing.
- `crates/biome_project_analysis/src/facts.rs.ImportOccurrence.fragment`: complete original owned import-occurrence struct only.
- `crates/biome_service/src/project_analysis/owned_facts.rs.imports-projection.fragment`: original import projection within a larger expression.
- `crates/biome_service/src/project_analysis/exports.tests.rs.prefix-through-expected.fragment`: test imports and helper functions, excluding the original test bodies.
- `crates/biome_service/src/project_analysis/exports.tests.rs.namespace-and-registry.fragment`: six complete original tests from the contiguous namespace/registry source excerpt.
- `crates/biome_service/src/project_analysis/exports.tests.rs.native-registration.fragment`: complete original native registration positive test.

`historical/d1722ea7/report-hashes.fragment.md` contains exactly the 12 historical initial-commit hash-table rows printed in the surviving report-tail output. These are original historical hashes, not hashes computed during recovery, and are not the missing complete 24-file manifest or the final fix-round manifest.

Recovery used file-write tool patches only. One orchestration attempt had a JavaScript string-literal syntax error before executing any writes; the same three artifacts were subsequently saved successfully. No recovered command was executed.
