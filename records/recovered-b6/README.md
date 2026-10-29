# B6 retained-history recovery

Recovery only. No source was read from or written into `/workspace/my-biome`.

Original branch: `feat/native-project-analysis`.
Original B6 base: `74f3d041af8d8f8986c087b78d9882a86be6f758`.
Original B6 implementation commit / fix base: `b07d051a6260282912d5cf0ada1369d2ba1a182f`.
Latest B6 fix commit: `e2dcaceab06c4870fa5e9c9eeef13e31db8c8aee`.
Fix commit title: `fix(project): preserve catalog uncertainty and provider roles`.
Fix diff stat: 4 files changed, 340 insertions(+), 18 deletions(-).

Files `01-fix1-production.patch` and `02-fix1-tests.patch` contain literal complete `git diff` output retained in this agent's history immediately before the final fix commit. Apply both to fix base b07d051a; their paths are disjoint, production then tests is the export order. They are not the original B6 feature patch and cannot reconstruct the missing b07d051a base by themselves. No guessed context or code has been added.

Missing from retained literal history: complete original B6 production files; complete service inventory/acquisition file; original feature patch (31 files); full original test file before the appended fix tests; full original and fix JSON hash manifests. A compacted prose summary exists but is not source text and has not been turned into code.

Historical manifest SHA256 values (the manifest bytes themselves are missing):

- Original `task-B6-final-sha256.json`: `3ed6f39d0ac5022a451e3f6d90964faa3036efa3d0069d5841df0504eae4e8a9`.
- Fix `task-B6-fix1-sha256.json`: `f63c62e74b78c7d53d12bbaeaef901d9314b2d8132dd9c2a153b51f6baa72d3d`.

Original manifest described 31 source/test/data/changeset files and 43 logs. Fix manifest described the same aggregate 31 files, 4 changed files and 7 fix logs. Those contents must not be inferred from their hashes.

Historical verification facts: initial fix RED was four behavioral test failures; focused B6 GREEN was 13 tests. Final affected `biome_project_analysis` plus `biome_service` suites reported 578 passed, 0 failed, 9 ignored. `just f` and `just l` passed. No gates have been rerun during recovery.
