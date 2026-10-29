# B5 fix round 2 literal recovery

Recovery stage: final reviewed commit `74f3d041af8d8f8986c087b78d9882a86be6f758`.
Base stage: `e758ebf7c4d937283c3c1b4ac8a6fa5d9c9043e4`.
Recovered by the original scoped fix-round2 reviewer from retained tool outputs on 2026-10-29. This is artifact recovery, not a new review or verification run of project code.

## Literal retained material

- `engine-dependencies.diff`: complete exact per-file diff retained from the original read-only Git output, including original blob prefixes and all hunks. Target `crates/biome_project_analysis/src/dependencies.rs`.
- `service-dependencies.diff`: complete exact per-file diff retained from the original read-only Git output, including original blob prefixes and all hunks. Target `crates/biome_service/src/project_analysis/dependencies.rs`.
- `dependencies.tests.append.rs`: exact 173-line addition retained from the original final test-source read. Append, including its leading blank line, after line 1511 of the recovered base `crates/biome_service/src/project_analysis/dependencies.tests.rs`. No edits precede this addition in the three-file fix. Both new test functions are complete.

## Complete reconstructed final sources

The three complete final files are under `final-74f3d041/`, preserving their repository-relative paths. Baseline input files came from sibling directory `../recovered-b5-fix1-review/crates/` and were only read.

`reconstruct_recovery.py` is a recovery-artifact utility, not historical project code. It checks each diff context line and old/new hunk count before producing final files. It verifies original and final production-file Git blob hashes against the exact prefixes retained in the original diff output. It appends the literal test addition to the 1511-line baseline. It runs no project code and modifies no repository checkout.

Completeness:

| Repository path | Final lines | Final bytes | Final SHA-256 | Identity evidence |
| --- | ---: | ---: | --- | --- |
| `crates/biome_project_analysis/src/dependencies.rs` | 876 | 31271 | `5fdb940e90065292d91d674b875b5368e86c43a9f0ea2770ce6f224abc3a26a5` | Reconstructed Git blob `c86322efc1674361abed48d779cb09c2dc87316c` matches retained final prefix `c86322efc16`. |
| `crates/biome_service/src/project_analysis/dependencies.rs` | 858 | 32480 | `54f16fde6054fcefc4d476f0ff99729e20e92be5993f93e6d8d9f3dc9c96d7e4` | Reconstructed Git blob `4a4dc37304b671fd7e9672bb678cdb719060e864` matches retained final prefix `4a4dc37304b`. |
| `crates/biome_service/src/project_analysis/dependencies.tests.rs` | 1684 | 62557 | `e7fd3e2557d72e6a67c8823aede3847f53699bd2416da816201a9d330c4c5931` | Complete recovered 1511-line base plus exact retained 173-line final addition. Computed blob `785b377fcff14a0e3d15104d9d216cd61b013fba`; the original final test-file blob prefix was not available in retained bounded Git output, so no independent prefix match is claimed. |

`reconstructed-sources.json` contains machine-readable source lengths and hashes. `SHA256SUMS` hashes all recovery files except itself.

No missing source regions remain in these three reconstructed final files. No project tests/builds/lints, historical shell commands, new code review, current checkout edits, or child agents were used for this recovery. Hashes and reconstruction assertions describe these recovery artifacts only; historical test results were not rerun.
