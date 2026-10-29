# B7 retained-history recovery

Recovery-only export by `/root/project_b7_implement`, 2026-10-29. No reads or writes to `/workspace/my-biome`, builds, tests, installs, old task commands, or remote actions were performed during this recovery. Shell/Python operations only saved literal retained text, replayed observed replacements inside this recovery directory, and computed hashes. Historical test output is evidence from the lost environment, not a new verification run.

## Historical stages

1. B7 base: e2dcaceab06c4870fa5e9c9eeef13e31db8c8aee.
2. Main B7 commit: b0adffecc6ff7d18497f8b487f2f3ef5b0c8ea3f, `feat(project): enforce analysis policy and compatibility filters`, 36 files.
3. Final B7 fix commit: 20104d6aeff4c50f6fb5d308270f747b6bffd878, `fix(project): preserve source-specific tag policy`, five files: four engine modules plus service policy tests. This is the latest B7 stage retained here.

`b0adffe/` and `20104d6/` preserve original repository-relative paths. The final directory also contains byte copies of complete recovered artifacts that the five-file fix did not modify.

## Complete source, checked against historical Git blob prefixes

The original retained `git diff` outputs expose 11-hex blob prefixes. All eight reconstructed files below match those prefixes. Full blob IDs in the recovery manifest were computed from recovered bytes; only their listed prefixes were independently visible historically.

| File under crates/biome_project_analysis/src | Main B7 prefix | Final B7 prefix | Retained literal provenance |
| --- | --- | --- | --- |
| options.rs | f6cb936a251 | 6b6c5488dcf | Full section in tool chunk f7d6d0; final diff in 5a1899 |
| members.rs | 699fe0552c6 | 3128506c077 | Full read ac389d; final diff in 5a1899 |
| policy.rs | e1888bf6d3c | 87542e41e32 | Full read fda655; exact one-line fix and diff fb1ccf |
| exports.rs | 246b19847ce | dddddd4484f | Full read 18356e had a middle truncation; overlapping literal lines450–550 in f7d6d0 close it exactly; resulting whole-file prefix matches. Final diff fb1ccf |

`exports.rs` is complete, 41,352 bytes at main B7. No guessed code fills the old truncated region. `replay-observed-fix.py` records exact observed text replacements; running it during recovery wrote only final variants here and asserted historical prefixes. `patches/b0adffe-to-20104d6-engine.diff` is a newly serialized unified diff between those recovered, verified byte sequences; its contents are the exact final engine changes, though diff serialization itself is not claimed to be an original saved log.

## Other complete literal recoveries

- `crates/biome_project_analysis/src/resolution.rs`: complete literal read from 740ed6. No historical full-file digest/prefix is retained for independent matching.
- `crates/biome_project_analysis/tests/policy.rs`: complete 310-line literal suffix of f9aba9. No historical full-file digest/prefix retained. Unchanged by final fix.
- `crates/biome_project_analysis/tests/support/mod.rs`: complete literal read from 740ed6. No historical full-file digest/prefix retained.
- `.changeset/breezy-carpets-brake.md`: complete observed five-line release entry. No historical full-file digest retained.
- `xtask/project_analysis/policy/native-expectations.json`: complete observed text from aeff9f. SHA256 matches historical SHA256SUMS entry ec6090f2f102d9f6d8f4bb072f8f826e0d6c4a2707719171f3ded7ecdadd4b07.
- `xtask/project_analysis/policy/comparison-index.json`: all 30 literal records from aeff9f reserialized with the observed field order/indentation. SHA256 matches historical SHA256SUMS entry cc634fa476876d0fab91abb46fefb41761045dc9dec62cd506cd0232bfd3a037.

## Partial source and patches

`patches/e2dcace-to-b0adffe-observed-integration.diff` preserves the complete literal diff sections seen in a50750 for graph project_facts.rs, engine lib.rs, service project_analysis.rs and service snapshot.rs. It is not the whole B7 diff. Historical old/new blob prefixes are preserved in each section.

Service `crates/biome_service/src/project_analysis/policy.tests.rs` is incomplete as a whole. Literal pieces are under `excerpts/`:

- b0adffe-prefix: lines1–106, helper and first three tests, read in f7d6d0.
- all17-leading-cases: next 40 lines, starting at107, ending after the optional-peer positive fixture. The following text was truncated in the retained output.
- all17-types-case: exact 16-line Types case and NsExports header from the retained read; before the lines200–347 piece.
- all17-lines-200-347: exact 148 lines from 740ed6 and overlapping f7d6d0, through the all-category test closing brace.
- b0adffe-tail: exact 351 lines, original lines349–699, from f7d6d0/f9aba9 overlapping reads, starting with b7_tagged_imports_keep_credit_and_resolution_evidence and ending with b7_reexport_keeps_origin_tag_policy.
- final-added-lines-700-954: exact 255-line final append from 5a1899 plus the literal subsequent test-only is_empty correction. No source was inferred. The entire final file cannot be hash-verified because its base middle remains missing.

`patches/b0adffe-to-20104d6-service-test-append.diff` serializes that append against observed base line699. No invented blob IDs are supplied. Together with the engine diff it captures the complete five-file fix contents, subject to the service append's unverified whole-file base.

## Historical receipts

`receipts/original-b7-post-commit.json` and `original-fix1-post-commit.json` reproduce the JSON values printed by original commit tools d50621 and 710160. These identify lost original manifests/report/log digests; the manifests and complete reports themselves are not recovered here.

The focused GREEN log and final lint log are literal visible historical output sections from f1789b and 8105fa. `fix1-final-result-tool-output.txt` is the complete printed result summary from a19f59, not the full original08-final-engine-service.log. Trailing tool-wrapper newlines are not independently byte-verified. Historical final engine/service result: 613 passed, zero failures, nine ignored; no recovery-time tests run.

## Explicit gaps

- No literal complete engine inventory.rs or service project_analysis/inventory.rs survives the pre-fix compaction. No guessed inventory source is provided.
- No literal complete service policy.rs, provenance.rs, provenance.tests.rs, policy_corpus.tests.rs or owned_facts.rs survives here. Earlier summaries describe them but are not recoverable source text.
- Service policy.tests.rs lines147 onward before the retained Types block remain missing. Line348 was a separator between tests. No complete replacement file is fabricated.
- Main B7 changes to graph project_members.rs and remaining engine modules/Cargo/ledger are not fully represented by retained literal diffs in this thread.
- Original-fixtures.json and originals/observations.json raw bytes are absent here. Historical hashes are 020a2640a73ad321da68b86a6bee999862445284de68b187662a988cb591ef96 and 592eeeea31435889cda0288e7dda25d8671a80c711c79d1bf80f59ead7b82d40 respectively. Their contents were not recreated from expectations.
- Original manifests, full reports, full build logs, corpus README/provenance and final checksum file are not claimed recovered. Existing D4 root-selection extra child-tag-hint compatibility delta and C2/C3/C4 boundaries remain historical limitations, not newly adjudicated here.

All recovery files except the self-referential inventory itself are hashed in RECOVERY-MANIFEST.json. No Git commit was created; the parent owns recovery-branch checkpointing.
