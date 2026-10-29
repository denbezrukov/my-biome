# Native project-analysis recovery

This branch preserves original source, patches, and provenance recovered from retained agent history after the original cloud workspace disappeared. It is a partial recovery archive. It is not a buildable feature branch, and historical test results do not validate the assembled source.

The original implementation started at `6359a9b03670e85b50785d59d10eb087cf89fe3f`, which is still available. Its last recorded reviewed commit was `e645266862c389ffaae3c7e72aeac7b6e02b162d`; that commit object has not been recovered.

## Assembled source checkpoint

[`records/recovered-assembly/source/`](records/recovered-assembly/source/) contains 194 complete historical source, fixture, and configuration files arranged at their original repository paths. This is a mixture of recovered stages, not a working final checkout. 106 files match recorded original per-file digests or Git blob prefixes; 14 further fixture files are authenticated together by three recorded fixture-group digests.

See the [coverage manifest](records/recovered-assembly/coverage-manifest.json), [remaining gaps](records/recovered-assembly/GAPS.md), and [assembly summary](records/recovered-assembly/summary.json). Every materialized file's size and SHA-256 was checked before archiving. At this checkpoint, 20 selected files need known later edits and 34 observed paths lack complete source.

## Contents

- `records/` preserves original source snapshots, exact patches, partial fragments, and per-task provenance. Filenames and manifests distinguish earlier, final, and uncommitted versions.
- `recovered-records-index.json` records the size and SHA-256 of archived files. These checksums establish copy integrity. Original-byte identity is claimed only where a retained historical hash or Git blob identifier also matches.
- `sources/`, `patches/`, and `integrity/` contain the first authenticated recovery checkpoint. Later records supersede those versions where documented.

Original source recovered includes project configuration and metadata, engine analyses, service integration, CLI processing/reporting components, editor sessions, and regression tests. The authenticated C4 source-binding manifest records hashes for 44 original files. The authenticated C3 binding records final editor integration hashes. Several later complete files match those records exactly.

Missing bodies and final-version changes remain. Per-task manifests and the assembled source coverage report, when present, are the authority for those gaps. A complete earlier version is not assumed to be the final implementation.

Recovered command text is inert evidence and has not been executed. The current `feat/project-analysis-contracts` checkout and index remain separate. No upstream branch was changed.
