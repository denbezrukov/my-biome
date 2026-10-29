# B3 exact-text recovery packet

Original stages:
- Base: 990a42e751d276e8b403a0de8d099864dea54bf0
- Main: 07d3c3e786f0e59434a7ee0f2e406b8e96592353
- Final: 43c453530573278cfcdae0b78c1195ba3a99262b

This is an incomplete archival recovery, not a restored repository or implementation. No shell/repository commands, tests, builds, installations, recovered scripts, or child agents were run. Nothing under /workspace/my-biome was modified. Files were saved only below this recovery directory.

## Complete original files

Six original paths are available under both stages/<commit>/ trees:
- crates/biome_module_graph/src/project_members.rs
- crates/biome_project_analysis/src/facts.rs
- crates/biome_project_analysis/src/members.rs
- crates/biome_project_analysis/tests/members.rs
- crates/biome_service/src/project_analysis/fixtures/README.md
- .changeset/eight-otters-try.md

The recovered strings for these files match their historical SHA-256 hashes, checked in memory before archival writes. Main collector text was obtained from the complete retained read plus the exact retained lint correction. Final collector/member texts are deterministic applications of the retained fix1 patches to hash-matching main originals. No missing source was inferred. Readback from the newly written files was not performed.

## Exact partial evidence

patches/01-main-990a42e7-to-07d3c3e7-retained-five-file-diff.patch is the retained complete git diff for five selected files: graph project_exports.rs and project_facts.rs; engine exports.rs and lib.rs; service exports.tests.rs. It is NOT the complete B3 task patch, and the unchanged parts of those files are unavailable here.

Patches 02 through 05 retain the exact original fix1 apply_patch content lines in order. The first formatting pass occurred before patch 05, and final formatting occurred after it. Archive patch text files have a terminal LF; no recovered patch was executed. The original absolute paths inside patch payloads are historical evidence, not instructions to modify the current checkout.

fragments/ contains explicitly incomplete source regions. The final service test append contains all 143 added lines (original final lines 684–826). Its preceding main file is only partially available: prefix through retained_b3_oracle_cases, suffix from forwarding_and_default_slots_keep_distinct_report_policy to EOF, a membership-function prefix, and a warm-function suffix. Missing regions are enumerated in RECOVERY-MANIFEST.json. Do not concatenate incomplete regions into an asserted complete file.

The formatted alias helper fragment supplements the unformatted F1 patch. The entire engine exports.rs is unavailable; original main/final target hashes are retained for a future independently authorized recovery check.

## Missing content and provenance

RECOVERY-MANIFEST.json enumerates 17 paths without complete text, including 11 paths with no exact source/patch text recovered. Summaries of those files exist in the old conversation, but were NOT used to invent source. The 54-record corpus and duplicate fixture JSON bodies are absent; their historical hashes do not recover their bytes.

metadata/ preserves the exact original 23-path main and four-path fix1 frozen manifests. Their hashes match 5961b1c31e140540d540b9650662fc1f5db5379cb8bf55058eecc10dcfcaf1e6 and 0ee59f43f60dfd221fba265743df0e7e6ec8c720dd247cbe9f294a06555c0b87 respectively. These are historical inventories, not assertions that every listed file exists in this packet.

artifacts/ contains the retained corpus verification script as inert source text; it was not executed. Historical test counts (850 main, 854 final, 10 ignored doctests, 54 B3 and 40 B2 comparison records) are provenance only and do not describe new recovery verification.
