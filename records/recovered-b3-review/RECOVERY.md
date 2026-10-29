# B3 original-source recovery

These artifacts were transcribed from literal original source and diff text retained in the B3 review agent's conversation. The original reviewed stage was commit `07d3c3e786f0e59434a7ee0f2e406b8e96592353`, against base `990a42e751d276e8b403a0de8d099864dea54bf0`. They are not claimed to represent later final commit `43c45353` or its review fixes. The current rebuilt repository was not used as original source.

## Complete files, original hashes matched

| Recovery file | Original repository path | SHA-256 |
| --- | --- | --- |
| `members.tests.rs` | `crates/biome_service/src/project_analysis/members.tests.rs` | `2a4af7986bc7676eb5ee82649a2ada93e56f1fcfa76f02a309f94cc85bb09959` |
| `owned_facts.rs` | `crates/biome_service/src/project_analysis/owned_facts.rs` | `09eaa4147ca4dde3db35c8a9fc1591f3ec18e49dc6db9b04b2344a75a305537f` |
| `project_analysis.rs` | `crates/biome_service/src/project_analysis.rs` | `989ff77ea2838e63de2d210ab91ee43beaa6191ca85828d089b3ad9651c5eddf` |
| `project_exports.rs` | `crates/biome_module_graph/src/db/queries/project_exports.rs` | `ae66f066c8bd621cd9392c0e3cbcfb75d9d8797af3dffff9d0c036e5234d8135` |

These four recovered file hashes were compared with the original SHA-256 manifest retained from the B3 implementation report. All four match exactly. No builds, tests, source execution, or current-repository source reads were used for recovery.

## Partial source and complete diff

- `project_facts.rs.lines-0001-1599.partial` contains literal original lines 1 through 1599 of `crates/biome_module_graph/src/project_facts.rs`. It ends after the complete `identity` helper and its following blank line. Its SHA-256 is `900982a7166db8c58ae6f463428ff3e703fa5cad7c04fb4bb6545fbcad035541`. This is deliberately isolated as a partial file and is not compilable or a full-file replacement.
- The original full `project_facts.rs` SHA-256 was `1019a18b006d8e485c8b1e2313e41ecb4df709a49da771998537d1f774bb087f`. The partial file therefore does not match the full-file hash; this is expected due to the missing tail, not evidence that the retained prefix was independently verified.
- `project_facts-b3.diff` is the complete retained B3 diff for that file, with original Git blob prefixes `4bef19f73d0..34c6abf6640`. Its recovered SHA-256 is `33b5ad13ca9fe69c9192beac25dfeae5008e955964567af3f8ec2a7787cf4d20`. No original standalone diff hash was retained, so its transcription has not been independently hash-verified.
- `original-07d3c3e7.sha256` is a literal transcription of the original 23-path SHA-256 manifest retained from the report. It describes the original commit, not the completeness of this recovery directory.

## Gaps

Context compaction occurred after saving `project_facts.rs` through line 1599. The literal remaining source and fixtures did not survive compaction. No missing text has been guessed or regenerated.

- Missing `project_facts.rs` original lines 1600 through EOF. The missing tail begins at `imported_name`; includes binding/namespace access helpers, literal decoding, registry/CommonJS helpers, and remaining collector helpers.
- Missing service `exports.tests.rs`.
- Missing engine fixture `duplicate_binding.json` and `duplicate_spelling.json`.
- Missing service fixture `members-oracle.json` and its fixture `README.md`.
- No additional standalone service `members.rs` source was recovered; the original 23-path manifest contains service `members.tests.rs` and engine `src/members.rs`, but no service `members.rs` entry.
- Other original manifest paths were not exported by this agent in this recovery pass. Parent reported separate recovery of the B3 engine members, graph member collector, facts, and engine tests.

`recovered.sha256` records checksums of the recovery artifacts and this metadata. It is a local integrity index, not an independent original-source verification.
