# Exact retained B1 recovery material

Recovery-only archive; no implementation, tests, builds, installation or recovered shell commands were executed. `/workspace/my-biome` was not modified.

Historical base: `b3f4b0f077050576c5c6fe874c87dfa353f10048`.
Main B1 commit: `a62fb4875d3d9aca9fc3ffcc74b22b56913b9aed`.
Final B1 fix commit: `df22208cc013659d537dc12e6d95cbf3c25ba11c`.

Files under `main/` reproduce exact retained source text from historical tool reads at the main B1 stage, except files explicitly placed under `intermediate/`. Fix patches are retained separately in chronological order. The production reachability source did not change in the final tagged-template fix.

Available exact history:

- Complete reachability production file assembled from the historical full read and overlapping `sed -n '490,720p'` read.
- Complete service reachability test source at main commit, from three consecutive historical reads (1–260, 260–540, 540–800).
- Complete engine reachability test source immediately before the final explicit-scope ordering assertion, from historical reads 1–240 and 240–360. The final ordering edit is retained separately as its exact apply_patch input. Subsequent rustfmt wrapping of that edit was not read in full.
- Exact final tagged-template correction patch and exact service regression patch, followed by rustfmt (two added lines of formatting in test code not fully retained).
- Selected exact production/test/documentation diffs retained separately where complete file bases were not retained.

Missing, not reconstructed or invented:

- Six new owned fixture JSON bodies (only names and summary/stat metadata retained), and all original full base files not explicitly archived here.
- Exact original 29-file SHA-256 values. The historical manifest path was `.superpowers/sdd/native-project-analysis-2026-10-26/b1-logs/final-source-hashes.txt`; the fix-round manifest was `b1-logs/fix1-source-hashes.txt`. Their contents are not in retained conversation text.
- Complete original report and log files. Partial report text and selected outputs survive in conversation but are not substituted for original full files.
- Full exact final formatting of newly edited scope/regression test blocks. Archived apply_patch inputs are exact pre-format edits; no formatting is guessed.
- Main collector diff header/initial additions preceding the visible import-collection hunk, and old base contents required to apply all archived hunks.

Historical validation metadata (summary, not reconstructed logs): main affected suite 781 passed; final fix affected suite 757 passed (395 module graph, 362 service), 18 source tests passed; original unchanged engine 26 passed. Main/fix formatting, lint, pending-snapshot and source-hash gates passed. This recovery performed no verification of historical claims or archive hashes.

## Archive inventory and order

Complete retained main-stage files:

- `main/crates/biome_project_analysis/src/reachability.rs` (historically 650 lines; unchanged by fix 1).
- `main/crates/biome_service/src/project_analysis/reachability.tests.rs` (historically 762 lines; fix 1 follows separately).
- `main/crates/biome_project_analysis/Cargo.toml`.
- `main/.changeset/witty-things-attend.md`.
- `main/crates/biome_project_analysis/tests/fixtures/unreachable_chain_and_cycle.expected.json`.
- `main/crates/biome_project_analysis/tests/fixtures/empty.expected.json`.
- `intermediate/pre-final-scope-order/crates/biome_project_analysis/tests/reachability.rs` (historically 332 lines; followed by patch 01 and rustfmt, yielding 346 lines at main commit).

Main-stage diffs against historical B1 base (disjoint groups, all numbered 00): engine coverage, engine interfaces, analyze, service adaptation/registration, contracts/support, lock, conformance description. `00-main-collector-partial.diff` is explicitly incomplete: its leading source/type/constructor hunks were truncated from the historical tool output. No missing prefix was fabricated. Main new-file source is stored in the mirrored paths above, not repeated as addition patches.

Exact chronological late edit inputs:

1. `patches/01-main-final-scope-order.apply_patch.txt`, then historical `just f` (not executed in recovery).
2. Main commit `a62fb4875d3d9aca9fc3ffcc74b22b56913b9aed`.
3. `patches/02-fix1-tests.apply_patch.txt`; historical causal RED ran here. Its exact retained tail is `excerpts/fix1-red-tagged.log.tail25.txt` (not the entire original log).
4. `patches/03-fix1-production.apply_patch.txt` (same production edit also preserved as historical git diff `03-fix1-production.diff`, including abbreviated Git blob hashes).
5. Historical formatting added two wrapping lines to the new service assertions; exact final test wrapping unavailable. Historical gates followed; none were executed during recovery.
6. Final commit `df22208cc013659d537dc12e6d95cbf3c25ba11c`.

`excerpts/main-project_facts-literal-helpers.rs.txt` preserves the exact unchanged helper definitions read while verifying the tagged-template finding; it is a source excerpt, not the full collector file. All absolute paths inside saved apply_patch inputs are historical inert text; they were not applied during recovery.

No archived source was run, compiled, formatted, reimplemented or used to create substitute fixtures. New recovery metadata is clearly distinguished from historical source. Original full hashes remain unavailable, so byte identity with lost Git objects is not independently verified by this agent.
