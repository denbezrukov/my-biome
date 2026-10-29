# C1 reviewer retained-text recovery

Recovery only; not a new review, reconstructed implementation, or original-byte verification.

Original review base: `20104d6aeff4c50f6fb5d308270f747b6bffd878`.
Original review HEAD: `6c48ad8332ff1e3c6e4e4cf2936a6fff969233cc`.

All source fragments under `partial/` were transcribed literally from retained tool outputs of the original review at HEAD. Ripgrep line-number/context prefixes were removed; source tabs and text were retained. They are partial files, not complete originals. Hashes in `SHA256SUMS` certify these recovery bytes only; no original per-file hashes remain in this reviewer's context.

The original Rust reads were compacted into summaries before this recovery request. Exact source for service project_analysis request/policy definitions, engine result.rs, typed service client, workspace method registry, LSP production registry, and Rust configuration/RPC types is no longer retained. No Rust source has been invented from those summaries.

The retained generated TypeScript provides exact wire-contract text for diagnostic parameters, PolicyOverrides, ProjectAnalysisRequest, ReportSelection, revisions, full result records, source/evidence records, and project state types. It is not a substitute for recovering their Rust implementations. ProjectAnalysisConfiguration itself appeared earlier in the generated file and its literal definition is not retained in the current context.

File ranges refer to the original 14,170-line generated workspace.ts and 7,871-line Cargo.lock. The `lines-13081-13465.ts.txt` and `lines-13466-state-end.ts.txt` fragments are adjacent. The latter ends at the ProjectAnalysisStatus definition, before ProcessFileParams. The Workspace-methods and createWorkspace-methods fragments are separate nonadjacent original snippets; do not concatenate them directly onto the type fragments as a whole file.

The Cargo.lock fragments retain the exact three relevant internal-package blocks, including the new optional schema, LSP test, and percent-encoding dependency edges. Cargo.toml feature/dependency declarations cannot be inferred byte-for-byte from these blocks.

Only the recovery directory was written. No repository source, Git state, project code, builds, tests, or review gates were changed or executed.
