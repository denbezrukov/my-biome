# C3 fix-review source recovery

Recovery only. No new review, project execution, repository edits, child agents, or inferred missing source.

Original workspace: `/workspace/my-biome` (no longer present).
Original reviewed range: `540d2e1a7b8bda82c42ade70459dec4975899c88..12bf926317b7032e2ddc46124bf277b8409bf5fd`, exactly three changed files.
Source: literal original tool outputs retained in the `project_c3_fix1_review` conversation. The original review confirmed HEAD was the latter commit and the three files had no worktree diff.

## Complete final files

Under `stages/12bf926317b7032e2ddc46124bf277b8409bf5fd/full/`:

- `crates/biome_lsp/src/project_analysis.rs` — 23,954 bytes.
- `crates/biome_lsp/src/project_analysis.tests.rs` — 49,265 bytes.
- `crates/biome_lsp/src/session.rs` — 66,215 bytes.
- `crates/biome_lsp/src/documents.rs` — 734 bytes.

All four files exactly match both SHA-256 and byte count in the supplied authenticated historical manifest, `/workspace/.biome-cloud/native-recovery/recovered-c3/final-precommit-bindings.exact-recovered.json`.

Session and test source were transcribed into numbered literal sections first, then concatenated without transformation. Those sections remain under `partial/` as recovery evidence even though their corresponding assembled files are complete.

## Complete base files

Under `stages/540d2e1a7b8bda82c42ade70459dec4975899c88/full/` are the three changed source files. These were mechanically recovered by reversing the retained literal fix diff against the authenticated final files. Every hunk matched the final source exactly, all stated hunk counts matched, and each resulting Git blob SHA-1 matches the historical 11-character blob prefix printed in the original diff. See `base-stage-verification.json`. These are mechanical derivations from exact source and literal diff, not guessed code.

## Partial files

Under the final stage's `partial/`:

- `crates/biome_lsp/src/handlers/text_document.rs.lines-26-161`
- `crates/biome_lsp/src/handlers/text_document.rs.lines-187-290`
- `crates/biome_lsp/src/server_test_utils.rs.lines-342-397`

These contain only the exact indicated source lines. Missing source is not filled. They have current export hashes, but no historical fragment hashes were available.

## Diff and verification

`diffs/540d2e1a7b8bda82c42ade70459dec4975899c88..12bf926317b7032e2ddc46124bf277b8409bf5fd.literal.diff` preserves the original three-file fix diff with three lines of context. Individual file diffs are also saved. The test diff was assembled mechanically from its retained literal header, unchanged context, and the authenticated final source's exact appended lines.

The combined diff is 19,422 bytes. It is not the former 24,964-byte Markdown review package, which used different surrounding text/context. No historical hash of this exact three-context-line diff was retained; base/final Git blob checks authenticate its source transformation.

`export-manifest.json` records current sizes, SHA-256 hashes, Git blob IDs, and historical matches where available. `export-sha256.txt` records export checksums. No project build or test was run during recovery.
