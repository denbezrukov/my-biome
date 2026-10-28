# Native Project Analysis Contracts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a small, explicitly unimplemented compatibility contract for native project analysis.

**Architecture:** An internal Rust crate owns the issue taxonomy. A checked-in fixture separately pins reference identities and records implementation/conformance status; no resolver, analysis graph, runtime bridge or CLI integration is introduced.

**Tech Stack:** Rust, existing workspace serde/serde_json, Cargo.

**Spec:** `docs/superpowers/specs/2026-10-28-native-project-analysis-recovery.md`.

## Global Constraints

- Changes and PRs stay in `denbezrukov/my-biome`; preserve the SCSS work and unrelated files.
- Partial feature delivery must be labeled partial. A category declaration is not an implemented check.
- No Node bridge or plugin engine is introduced.
- The shared reference does not establish actual-project migration eligibility.
- Source commit and published artifact identity remain separate; do not claim their correspondence without evidence.
- Follow AGENTS.md for tests, formatting, lint, independent review and contributor-authored PR prose.

## Review Focus

- Unknown or differently cased issue keys must fail deserialization.
- Similar names, particularly `namespaceMembers`, must retain the exact reference spelling.
- Every reference issue has exactly one explicit unimplemented/untested disposition.
- A registry integrity claim must not be represented as a locally verified artifact.
- The crate must not depend on service, CLI, resolver or Node and must introduce no runtime behavior.

### Task P1: Pin the issue taxonomy and compatibility inventory

**Files:** Create `crates/biome_project_analysis/{Cargo.toml,src/lib.rs,tests/issue_contract.rs,tests/fixtures/compatibility.json,README.md}` and normal crate license files if required by repository convention. Modify workspace Cargo.toml and Cargo.lock minimally. Include the recovery spec and this plan as continuation documentation.

**Interfaces:** `pub enum IssueKind` derives equality/order/hash/debug/copy and serde serialization/deserialization; expose `pub const ALL: [Self; 17]` on the enum. Variant names are Files, Dependencies, DevDependencies, OptionalPeerDependencies, Unlisted, Binaries, Unresolved, Exports, Types, NsExports, NsTypes, Duplicates, EnumMembers, NamespaceMembers, Catalog, CatalogReferences, Cycles. The serialized names respectively are `files`, `dependencies`, `devDependencies`, `optionalPeerDependencies`, `unlisted`, `binaries`, `unresolved`, `exports`, `types`, `nsExports`, `nsTypes`, `duplicates`, `enumMembers`, `namespaceMembers`, `catalog`, `catalogReferences`, `cycles`.

The internal crate is `publish = false`. Its only regular dependency is workspace serde; tests use workspace serde_json. No speculative snapshot or analysis APIs in this slice.

- [ ] Write `issue_keys_match_pinned_reference` using the checked-in fixture: assert 17 unique issue keys, complete enum coverage, exact serialization round trips, and the source/artifact identities remain separate fields.
- [ ] Write `unknown_issue_keys_are_rejected` covering `classMembers`, `Files`, empty and future names. Write `inventory_does_not_claim_implementation_or_conformance` asserting every issue has `implementation: "unimplemented"` and `conformance: "untested"`; no replacement claim.
- [ ] Establish the missing-contract failure with `cargo test -p biome_project_analysis`; an initial missing target/import compile failure is permissible for this new crate. Then implement only the enum and manifest/fixture contract and rerun the crate suite.
- [ ] Pin fixture metadata from `/workspace/.biome-cloud/native-recovery/reference-contracts.json` and immutable source evidence, never from memory. Include source URL/commit/SHA256 and published version/tarball URL/registry integrity, explicitly labeled registry-declared and unverified if the tarball has not been verified. Tests are offline and do not execute Knip. README documents provenance and unimplemented status concisely.
- [ ] Run `just f`, `just l`, final crate tests, and inspect generated lockfile changes. No unrelated dependency version churn. No changeset: internal unpublished contract with no end-user behavior.
- [ ] Fresh independent complete-diff review; resolve validated findings and rerun affected checks. Commit with a conventional title and push the verified feature branch to the fork. PR publication uses the unchanged repository template and contributor-authored prose.
