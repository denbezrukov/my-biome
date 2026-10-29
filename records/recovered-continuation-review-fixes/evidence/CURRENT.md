# Native Knip replacement: durable continuation state

## Product branches in denbezrukov/my-biome

| Branch | Latest pushed commit | Scope |
| --- | --- | --- |
| feat/native-project-foundation | e92f1c2d6f67582a2f594305ff87a4950352edb9 | Restored A1 contracts and pinned comparison harness, followed by reviewed failure-retention fix. |
| feat/native-project-config | e67e1d9516b | Restored A2 static JSON/JSONC configuration, package and tsconfig metadata, generated schema and bindings. Based on foundation. |

Both product branches are pushed. A subsequent context-isolated review found two A2 metadata defects; their corrections are currently undergoing final verification before a follow-up commit. Foundation has 8 contract tests and 11 harness tests passing, plus three fresh real pinned Knip smoke cases. A2 has 10 configuration and 12 metadata focused tests passing, and full affected-crate suites pass with --all-features. Workspace `just f` and `just l` pass. The failed-run regression tests were observed failing before their fixes.

Current isolated worktree: /workspace/.biome-cloud/native-continuation/worktree. Current branch: feat/native-project-filesystem, based on configuration. This pending small increment restores canonical filesystem identity, with 12 existing unit tests and 4 Linux integration tests passing after correcting root/parent normalization and stored aliases. Formatting, workspace lint and a fresh final review passed. Commit/push follows the independent A2 metadata correction so branches remain separable. Windows-only test was not run here.

## Recovery archive

`recovery/native-project-analysis-history` is pushed through 97c947305751e5d8ada23cc9a211eee585f92bcb. It preserves recovered original sources, stage maps, provenance and fresh continuation evidence. Archives are source/evidence collections, not buildable product checkouts. A mixed-stage 194-file historical assembly is not a validated feature implementation.

## Remaining work

This is still preparation for native analysis; it does not replace Knip yet. Plugins and executable configuration are deferred by user request. No native conformance pass is claimed from oracle smoke runs.

Next coherent units are A3 workspace metadata parsing and A4 resolver/module-graph facts. The A4 final original overlay is ready under a4-original/source, with exact transition checks in its manifest. A5 acquisition and snapshot consistency still have historical source gaps; restore or implement the missing mutation/watch hooks before integrating later analysis/CLI/editor code. See next-stages.md for exact paths and boundaries.

## Continuing safely

Keep product changes in this isolated worktree; preserve /workspace/my-biome and all recovery folders. Push each validated milestone to the fork. Never force-push or write upstream. Use source writes with fresh mtimes; copying old source mtimes once caused Cargo to reuse a RED-test stub. Use activate.sh, the continuation target directory, debug=0, four build jobs, and INSTA_UPDATE=no. Configuration full tests need --all-features for existing language-registry tests.

PRs have not been opened for these continuation branches. Repository instructions require contributor-authored PR prose; the latest user request authorized continued work and fork pushes. No cleanup/deletion of recovery material.
