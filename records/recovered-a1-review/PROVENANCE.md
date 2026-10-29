# A1 review-session recovery

Source: verbatim file-read outputs retained in the original `/root/project_a1_review` conversation. Original reviewed range: `6359a9b03670e85b50785d59d10eb087cf89fe3f..3d051362dd431c1206c97074664f4c91089e15ff` in `/workspace/my-biome`. New files in that range had full-file diff context.

This recovery does not read the rebuilt checkout, execute archived commands, or reconstruct missing content from guesses. The original functions.store entries `inventory`, `sources`, `compatibility`, and `testInventory` were checked and are unavailable in the resumed session.

Stages: files under their original relative paths are exports from retained literal outputs; hashes are recorded separately after writing. Any partial extracts are explicitly named and are not replacements for complete source files.

Original expected npm package-lock.json SHA-256: `77ebf0ac8ef14a4f872b929665f6a1c458bee1345c8e81448c9fdd77101d3e74` (recorded in identity.json, observations, and implementation report). This provides an independent exact-byte check for the recovered lock.

## Exported ranges and stage

All exports refer to original reviewed head `3d051362dd431c1206c97074664f4c91089e15ff`, before the review finding was fixed. In particular, the original language-fixture ownership classification is not repaired here.

- `xtask/project_analysis/oracle/package-lock.json`: full original lines 1–927; recovered SHA-256 matches the independently retained original digest above.
- `xtask/project_analysis/src/artifact.rs`: full original lines 1–231, from complete cat output.
- `xtask/project_analysis/src/conformance.rs`: full original lines 1–286, from complete cat and numbered rg outputs.
- `xtask/project_analysis/src/main.rs`: full original lines 1–40, from complete cat output.
- `xtask/project_analysis/reference/inventory.md`: full original lines 1–190, combined overlapping literal cat output and stored-output text slices shown during the original review.
- `xtask/project_analysis/reference/sources.json`: full original lines 1–602, literal fields from overlapping original outputs, reserialized with the displayed two-space indentation and original field order; 50 source records and 198 plugin names.
- `xtask/project_analysis/compatibility.json`: full original lines 1–2177, literal entries from original complete/overlapping outputs; repeated identical owner/status records reserialized without changing recorded values.
- `xtask/project_analysis/reference/additional-sources.json`: full original lines 1–252, literal metadata from complete output; 41 records.
- `xtask/project_analysis/reference/tests.header.partial.json`: only original tests.json fields at lines 2–4; not a replacement tests.json.
- `xtask/project_analysis/reference/tests.selected_semantics.partial.json`: complete selected_semantics array (34 records) beginning at original tests.json line 4975; standalone reserialization of the literal original tool output. Original full tests.json had 5537 lines. Most of its 828 group git_blob fields are not recoverable from this session's transformed display, so no full tests.json is claimed.
- All three fixture directories: full literal files from original globbed cat output; all three aggregate fixture digests match the original recorded values. See `FIXTURE-DIGEST-VERIFICATION.json`.
- Expectations, retained successful observations, oracle identity/validation/package metadata, Cargo.toml, .gitignore, and ISC license: full literal original files. No pending observation is exported as a successful observation.
- `xtask/project_analysis/README.md`: full original lines 1–78.
- `.superpowers/sdd/native-project-analysis-2026-10-26/task-A1-brief.md`: complete literal task brief read first in the original review. No complete parent design/implementation-plan document was read or retained in this review session.

The ISC license SHA-256 also matches its original recorded value: `2f654153561e7d8b90fdbf448a7de09725f936533fc9c7b485190c036d089341`.

No independent original whole-file hash is retained for the other files; SHA256SUMS records the recovered bytes, not an unsupported original-hash match. The only filesystem reads during recovery have been of newly exported recovery files. No original source command, oracle, build, test, package manager, or current-checkout inspection was executed.
