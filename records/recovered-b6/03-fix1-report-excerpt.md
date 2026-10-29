# B6 fix round 1 — F1–F3 catalog acquisition boundaries

Fix base: `b07d051a6260282912d5cf0ada1369d2ba1a182f`. The fresh main review and controller validation are retained verbatim in `task-B6-review-1.md`. All three findings were independently checked against the current acquisition, owned inventory and catalog pass before implementation. This fix does not change unresolved/cycle behavior, the 24 retained observations, native corpus expectations or compatibility status.

## Fixes and covering behavior

**F1 addressed:** Acquisition now retains typed `Inventory.catalog_consumer_gaps` for unavailable or invalid manifests belonging to the root or declared/configured workspace patterns, including manifests omitted before workspace construction. The catalog pass carries these located gaps into its coverage and protects catalog negatives independently of source membership. A missing consumer manifest can name any shared catalog/package pair, so an unbounded catalog target domain is the defensible bound; no unknown workspace ID or fabricated reference is introduced. Positive missing references from separate valid metadata still use the known selected provider. Invalid manifests outside admitted workspace patterns remain generic acquisition gaps and do not blanket-suppress catalog findings.

**F2 addressed:** An unavailable selected YAML source now records invalid default and named selections at that exact captured source and container, and returns before claiming a fallback definition selection. The raw manifest fallback metadata remains captured in the immutable workspace/source records. Recovery either selects actual YAML definitions or, once the winning fields are proven absent, uses the supported manifest fallback. Source availability is checked separately from empty parser diagnostics; invalid UTF-8 fixture bytes exercise the actual `FileSystem::read_file_from_path` failure path through `MemoryFileSystem::read_to_string`, not a parser-error stand-in.

**F3 addressed:** YAML acquisition records local `WorkspaceYamlKind::{Pnpm,Yarn}` provenance at the existing presence-based selection. Only the Pnpm variant supplies container `overrides` consumers. The engine does not guess provider roles from canonical filenames. Root `package.json` `pnpm.overrides` remains a consumer, and nested workspace overrides remain excluded. No public schema or RPC is changed.

Four source/test paths changed: `crates/biome_project_analysis/src/{catalogs,inventory}.rs`, `crates/biome_service/src/project_analysis/{inventory.rs,b6.tests.rs}`. No new checked-in path, resolver/import-graph change, Node execution, oracle/network research or generated output was added. The existing minor feature changeset already describes selected providers and uncertainty and remains applicable. The internal serde-default inventory extension does not require grammar/configuration/RPC binding generation.

The four new service tests cover:

- Invalid and unavailable child manifests in metadata-only projects, both protecting unused definitions with gaps located at the child manifest while a valid root missing pair still reports.
- Three revisions per consumer failure: broken consumer, recovered literal reference, recovered nonconsumer. An unrelated invalid manifest remains throughout, proving overall Incomplete is not used as a blanket catalog suppression switch. Every revision equals an independent fresh service result.
- Actual unreadable selected pnpm YAML both with and without root fallback, invalid selection/source provenance, no false unused or missing findings, valid YAML recovery, proven-absent-field fallback recovery, and warm/fresh equality at all six states.
- Yarn generic override mappings ignored as consumers, actual pnpm-workspace overrides and root pnpm overrides retained, nested overrides excluded, and consumer source locations/fields checked.

## Exact fix verification ledger

All commands ran from `/workspace/my-biome`, after `source /workspace/.biome-cloud/activate.sh`; Cargo was serialized. Logs are in `.superpowers/sdd/native-project-analysis-2026-10-26/b6-fix1-logs/`.

```sh
INSTA_UPDATE=new cargo test -p biome_service --features stable b6_review1_
```

`red.log`: exit 101, **0 passed, 4 causal behavioral failures**, 483 filtered. Invalid and unavailable child manifests emitted `default.keep` and `default.unused` instead of no unused findings. Unreadable pnpm YAML emitted one false missing reference. Yarn overrides retained `keep`, incorrectly omitting its unused finding. No compilation failure or zero-test run was counted as RED.

```sh
INSTA_UPDATE=new cargo test -p biome_service --features stable consumer_manifest_retains_catalog_uncertainty
INSTA_UPDATE=new cargo test -p biome_service --features stable b6_review1_unavailable_yaml_preserves_unknown_selection_and_recovers
INSTA_UPDATE=new cargo test -p biome_service --features stable b6_
```

- `green-f1.log`: exit 0, **2 passed**, 485 filtered; F1 invalid/unavailable and recovery controls.
- `green-f2.log`: exit 0, **1 passed**, 486 filtered; F2 both fallback choices and recovery states.
- `green-all.log`: exit 0, **13 passed**, 474 filtered; F1–F3 plus all prior B6 service tests and the unchanged 24-case native/reference corpus projection.
- No failed intermediate fix attempt followed the initial causal RED. The only subsequent semantic-output wording adjustment states explicitly that invalid selected metadata may be unavailable; final gates cover that text.

Final formatted-code gates, self-review and frozen fix/aggregate hashes follow below.
