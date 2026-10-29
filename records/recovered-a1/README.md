# A1 recovery evidence

These files were copied from the surviving agent conversation on 2026-10-29. No recovered command was executed. Nothing under `/workspace/my-biome` was modified during recovery.

The `commands` directory contains verbatim original mutation-command excerpts, in their original chronological order. These are evidence, not scripts to execute without review. Some commands also refer to external source files or generated data that are not present in this recovery bundle.

The `snapshots` directory contains exact source text that appeared in original tool outputs or file-writing commands. Snapshot names distinguish intermediate states where later edits are retained separately. Formatting performed by `just f` is not reproduced by inference.

The `diffs` directory contains exact original Git diff text where available. No diff has been regenerated against the current repository.

Original workdir: `/workspace/my-biome`.
Original branch: `feat/native-project-analysis`.
Original base: `6359a9b03670e85b50785d59d10eb087cf89fe3f`.
Original initial A1 commit: `3d051362dd431c1206c97074664f4c91089e15ff`.
Original ownership-fix commit/head: `b292e878464503e93cba3f1d47a10be6e5e3dc41`.

This is an incomplete recovery. In particular, the complete npm package-lock bytes and the complete generated reference inventories were never retained as full text in this conversation. Their known hashes, generation commands, and gaps are recorded separately. No missing code or data has been guessed.
