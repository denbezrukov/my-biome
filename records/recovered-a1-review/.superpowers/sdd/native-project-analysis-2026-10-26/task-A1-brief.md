# A1 task brief

### A1: Pin the oracle and establish the capability/result contract

**Files:** Create `crates/biome_project_analysis/Cargo.toml`, `src/lib.rs`, the six model modules above, `tests/contracts.rs`, `tests/support/mod.rs`; create `xtask/project_analysis/{Cargo.toml,src/main.rs,src/conformance.rs,src/artifact.rs,compatibility.json,reference/sources.json,reference/inventory.md,oracle/package.json,oracle/package-lock.json,oracle/identity.json}`; modify root `Cargo.toml` and `Cargo.lock` for workspace members/dependencies.

**Interfaces:** Produces the parent plan's model/result contracts; `CapabilityStatus::{Supported, Untested, Unsupported}`; `ArtifactIdentity { package: String, version: String, integrity: String, dependency_lock_digest: String, runtime_version: String }`; `validate_artifact(expected: &ArtifactIdentity, observed: &ArtifactIdentity) -> Result<(), ArtifactMismatch>` in the harness. CLI: `cargo run -p xtask_project_analysis -- conformance --case <name> --engine <binary>`; ordinary unit tests require no network or Node. The harness initially supports `--oracle-only` to record evidence before the engine exists.

- [ ] **Test:** Add `all_issue_keys_have_explicit_disposition` asserting the exact 17 design keys; `same_version_different_integrity_is_rejected` asserting `Err(ArtifactMismatch)`; `unsupported_is_not_success` asserting an unsupported capability cannot serialize as a passed case. Store unsupported cases separately from passed/failed expectations.

  Required assertion shape (fixture setup is described above):

  ```rust
  assert_ne!(expected.integrity, observed.integrity);
  assert!(validate_artifact(&expected, &observed).is_err());
  ```

- [ ] **Red:** Run `cargo test -p biome_project_analysis --test contracts` and `cargo test -p xtask_project_analysis artifact`; expect the new contracts or mismatch assertion to fail before implementation.
- [ ] **Implement:** Define contracts, fixture loading and capability ledger. Copy the existing research inventory/source hashes into the harness references, preserving pinned URLs and distinguishing source research from execution. Enumerate all root/workspace configuration keys, CLI flags, reporters, fix families and deferred plugin names from those records, each with owner and disposition. Inventory the pinned reference test groups for each native issue/root/evidence family as well, including negative exceptions; assign fixture owners in B1–B7 and mark unported cases Untested or explicitly Unsupported. Category-name coverage alone is not semantic fixture coverage. Pin `knip` exactly `6.43.0`; npm metadata observed on 2026-10-26 advertises `sha512-8l3cCOg/EXoILO0qcSoMra49yQM91+lEIR4fu5Ko8rrArGX8xKdKZiGI9psPiY6rhLmfLERcMfFEByF0wli5CA==` for `https://registry.npmjs.org/knip/-/knip-6.43.0.tgz`. Download and verify actual bytes before execution; matching metadata alone is insufficient. Lock all transitive dependencies and record the actual Node version satisfying `^20.19.0 || >=22.12.0`. Install only in the isolated harness, with lifecycle scripts disabled; never install into a consuming project. Preserve ISC notices for reused fixtures. An identity is recorded as validated only after its supported profile cases pass, not merely after installation.
- [ ] **Green:** Rerun unit tests; run oracle-only cases for `orphan-file`, `namespace-known-key`, and `production-defaults`, retaining executable identity, complete command, raw stdout/stderr/exit and normalized output. A corrupted artifact must fail before execution. Mark remaining cases untested. Expect real tests passed, three retained oracle observations and no claim that Biome already matches them.
- [ ] **Commit gate:** Apply parent verification policy; commit the tested harness/contracts, e.g. `test(project): establish native analysis conformance contracts`.



## Global Constraints

These quotations from the approved design apply to every linked plan:

- “Do not build a competing resolver or a disconnected second import graph.”
- “Suppress definitive unused findings and fixes wherever an uncertain reference can affect the conclusion.”
- “Reporting suppression is distinct from graph exclusion.”
- “Warm incremental results must equal a fresh analysis at the same revision.”
- “The project under analysis does not need Knip installed after a successful migration.”
- “No Node bridge or plugin engine is smuggled into earlier milestones.”
- “Partial feature delivery must be labeled partial.”

Use JSON/JSONC only initially. Framework/tool plugins, executable configuration, custom compilers/reporters/preprocessors, non-JS preprocessing and Knip's JS API remain visible migration gaps. Implement all 17 named issue categories across the plans; never mark an unimplemented category as passed. Changes stay in `denbezrukov/my-biome`; preserve the existing SCSS branch. `AGENTS.md`, applicable local skills, and `CONTRIBUTING.md` govern generation, checks, changesets, reviews and PR prose.



## Shared interfaces and ownership

Everything below is a **planned interface**, not an assertion that it exists today. Types are introduced in A1 unless another task owns them. Internal IDs are valid only within their snapshot; external identities use normalized paths and declaration ranges.

| Owner / file | Planned contract |
| --- | --- |
| `crates/biome_project_analysis/src/model.rs` | `FileId(u32)`, `WorkspaceId(u32)`, `SnapshotRevision { session_id: String, project_root: String, epoch: u64, config_digest: String, input_digest: String }`; SHA-256 digests use lowercase hex. `Location { file: FileId, range: TextRange }`; `DeclarationId { location: Location, namespace: SymbolNamespace }`; `SymbolNamespace::{Value, Type}`. |
| `crates/biome_project_analysis/src/inventory.rs` | `Inventory { files: Vec<FileRecord>, workspaces: Vec<WorkspaceRecord>, roots: Vec<Root>, declarations: Vec<DependencyDeclaration> }`. Files distinguish reporting candidates from context files. Roots retain producer, location, default/production role and export-protection policy. Workspace records retain declaration ownership and metadata diagnostics. |
| `crates/biome_module_graph/src/project_facts.rs` (A4) | `ProjectModuleFacts` retains occurrence-based imports, exports, bindings/uses, tags, source ranges and unknown references; it uses syntax identities, not inferred `TypeId`. Existing graph owns collection and resolution. |
| `crates/biome_project_analysis/src/model.rs` | `AnalysisSnapshot { revision: SnapshotRevision, inventory: Inventory, modules: BTreeMap<FileId, ModuleFacts>, sources: BTreeMap<FileId, SourceRecord>, coverage: Coverage }`. `ModuleFacts` is the owned adapter output: resolved/failed/unknown edges, declarations, export slots, uses and tags. No mutable DB references. |
| `crates/biome_project_analysis/src/coverage.rs` | `Coverage::{Complete, Incomplete(Vec<CoverageGap>), Failed(Vec<CoverageGap>)}`. Gaps carry producer, affected target domain and reason; bounded/unbounded target domains can cross workspace boundaries. Capability coverage and dynamic-reference uncertainty remain distinguishable. |
| `crates/biome_project_analysis/src/options.rs` | `AnalysisOptions` contains compatibility profile, `AnalysisMode::{Default, Production, Strict}`, issue severities, entry-export/local-use/tag/ignore policies and runtime-cycle options. `ReportSelection` filters workspaces/paths/issues after analysis. Default exclusions: `nsExports`, `nsTypes`, `cycles`; enabled cycles warn. Production additionally excludes `devDependencies`, `catalog`, `catalogReferences` and configuration hints. |
| `crates/biome_project_analysis/src/result.rs` | `ProjectAnalysisResult { revision: SnapshotRevision, context: AnalysisContext, sources: BTreeMap<FileId, SourceRecord>, workspaces: BTreeMap<WorkspaceId, WorkspaceRecord>, coverage: Coverage, capabilities: BTreeMap<IssueKind, CapabilityStatus>, issues: Vec<ProjectIssue>, hints: Vec<AnalysisHint>, evidence: EvidenceGraph }`. All 17 `IssueKind` names serialize exactly as Knip keys. `AnalysisContext` records mode, profile ID, semantic-options digest and declared analysis scope. `SourceRecord` maps each referenced snapshot-local FileId to logical path, canonical URI, content digest and the snapshot text needed to render its ranges; unreadable sources carry an explicit unavailable reason and file-level fallback. Issues carry severity, identity, location, workspace, related evidence and optional eligible fix kinds. Include mappings for related evidence, coverage gaps and workspace/manifest locations, not only primary issues. |
| `crates/biome_project_analysis/src/lib.rs` (B1) | `analyze(snapshot: &AnalysisSnapshot, options: &AnalysisOptions) -> ProjectAnalysisResult`; `select_report(result: &ProjectAnalysisResult, selection: &ReportSelection) -> ProjectAnalysisResult`. Filtering never upgrades completeness or capability support. |
| `crates/biome_service/src/project_analysis.rs` (A5) | Owns acquisition, cache, cancellation, snapshot conversion and publication. `build_project_snapshot(project: ProjectKey, request: &ProjectAnalysisRequest) -> Result<Arc<AnalysisSnapshot>, WorkspaceError>` is a server method, with discovery/options in `ProjectAnalysisRequest`. No analysis is run over live mutable module maps. |
| `crates/biome_service/src/workspace.rs` (C1) | `pull_project_analysis_states(&self, params: PullProjectAnalysisStatesParams) -> Result<Vec<ProjectAnalysisState>, WorkspaceError>` reconciles the requested project keys, including Closed states; `pull_project_diagnostics(&self, params: PullProjectDiagnosticsParams) -> Result<ProjectAnalysisResult, WorkspaceError>`; params contain project key, request and report selection. A result is snapshot-consistent; superseded work is cancelled/retried rather than published as current. |
| `crates/biome_project_analysis/tests/support/mod.rs` (A1) | `run_case(name: &str) -> serde_json::Value` reads a checked-in owned-facts fixture and runs the engine; before B1 only fixture/model loading is enabled. `expected_case(name: &str) -> serde_json::Value` reads reviewed expectations. Never substitute oracle output for engine output. Source-to-result cases live in service tests, exercising collection/resolution too. The fixture normalizer emits `coverage` as `complete`/`incomplete`/`failed`, an `issues` array with `kind`, `path`, `symbol`, severity and location, plus separate gaps/capabilities/evidence; this is a test projection, not a promise about the RPC wire format. |

`ModuleFacts`, file/workspace records and options are defined alongside these contracts in A1–A4, with only fields needed by the approved requirements. Use existing Biome path, text-range and diagnostic primitives. `biome_project_analysis` must not depend on service, CLI, LSP or Node. Configuration translates into engine options in service; module_graph must not depend on configuration or the new engine. This prevents dependency cycles. RPC results are self-contained: consumers must not reinterpret FileId/WorkspaceId through current mutable maps or reread current disk contents to render snapshot byte ranges. `select_report` retains mappings and evidence for all surviving references, as well as analysis context and completeness. Snapshot identity is scoped by service session and project; restart, close/reopen and unrelated projects cannot reuse a freshness token. Semantic comparison helpers `normalize_snapshot(&AnalysisSnapshot) -> serde_json::Value` and `normalize_result(&ProjectAnalysisResult) -> serde_json::Value` remap local IDs to paths/declaration identities and normalize ordering. They exclude only session/epoch bookkeeping and measured timings, while retaining content/config/options digests, scope, all findings, locations, capabilities, evidence and coverage. Fresh and warm services may have different freshness tokens; never implement equality that ignores those tokens for stale-fix validation.



## Verification and review policy

Each task has a failing behavioral case, implementation and green check. A newly introduced test target may fail compilation initially; thereafter use a failing assertion that demonstrates the missing behavior. Test commands below are **future commands**, not tests already run. A zero-test run is not a pass. Review expected outputs and snapshots; do not bulk-accept reference differences.

Use `source /workspace/.biome-cloud/activate.sh` in this environment before cargo/just/pnpm. Run narrow cases first, then all affected crate suites at each review unit. Before each commit: required code generation, `just f`, `just l`, and review the resulting diff. Use `just gen-bindings` (schema plus types) for top-level configuration and workspace RPC changes; `just gen-configuration` is for analyzer registry-derived configuration, not a substitute. Inspect generated JS/WASM transport changes. Do not edit generated outputs independently. Internal dev-dependencies use relative `path`, as required by AGENTS.md.

Every task's final checkbox means these gates have passed, affected snapshots were reviewed, and a conventional commit can be made; it is not an instruction to commit unverified work. User-facing increments require the changeset skill and a minor feature changeset. Run the repository's `biome-code-review` workflow with a fresh reviewer before proposing a completed branch. Resolve findings and rerun affected checks. Preserve the PR template; contributor-authored PR prose and AI disclosure remain required by repository policy.
