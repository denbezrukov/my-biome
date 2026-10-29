# Export/type reference corpus

`exports-oracle.json` contains 40 retained Knip observations: 24 alias/CommonJS/registry cases, 12 declaration/local-use/entry-policy cases, 3 non-object CommonJS default cases, and 1 expression-arrow return assertion. Each record keeps exact source bytes and their SHA-256 hashes, the original full fixture digest, the original export/type finding projection (`oracle`), and a separate native expectation (`expected`) with a named disposition. The source test executes native collection and analysis against those inputs and checks source hashes.

The comparison is limited to export/type category, source path, and symbol name. It does not assert full CLI output, positions, evidence, coverage, or parity for other categories. File reachability controls are separate native assertions. Native analysis supports files, exports, types, namespace exports/types, enum and namespace members, and duplicate exports; other categories remain explicitly unsupported.

## Reference identity and execution

- npm artifact: Knip 6.43.0.
- Archive SHA-256: `ea3cbb5ef5a30ca2ea221f84f077dea98953487d748225bbacc4c8c18ad3caf1`.
- Archive SRI: `sha512-8l3cCOg/EXoILO0qcSoMra49yQM91+lEIR4fu5Ko8rrArGX8xKdKZiGI9psPiY6rhLmfLERcMfFEByF0wli5CA==`.
- Dependency lock SHA-256: `77ebf0ac8ef14a4f872b929665f6a1c458bee1345c8e81448c9fdd77101d3e74`.
- Node v24.19.0 SHA-256: `bc17c508ffeed0ec622934f9b7fa72f8e78da65350e63c3eceb56fa688aa5e12`.
- CLI SHA-256: `53e4293b8109ecbeb3955ee4ce138d99cb6a2358fa3c547f4e916d6459a36003`.
- Separate explanatory source pin: `07121567e60b73d1e640f395028c525a2b1d9778`.

Reference invocations used the verified absolute Node and CLI paths with `--config knip.json --reporter json --no-progress --no-config-hints --no-tag-hints --no-gitignore`. The `tsconfig-program-unprotected` reference additionally used `--use-tsconfig-files`. The `arrow-expression-assertion` observation selected `--include files,exports,types,unresolved`. The reference JSON explicitly disabled all 198 runtime plugin keys from the archive-matching `dist/schema/plugins.js`; no executable configuration or source fixture was executed. The environment contained activated `PATH`, `NO_COLOR=1`, `LANG=C.UTF-8`, and a fixture-isolated `TMPDIR`. Archive verification checked all 1,087 installed Knip regular files before and after execution, plus unchanged fixtures and runners. Independent dependency archive equivalence is not claimed.

The corpus `config` is the supported native configuration projection. It omits the 198 false plugin keys because the native core has no plugin engine. It translates the reference `--use-tsconfig-files` option to native `useTsconfigFiles` where applicable. It is not byte-identical to the original `knip.json`. `fixture_digest` identifies the original full fixture, including original configuration; it is not a digest of this reduced JSON record.

## Authoritative retained artifacts

The implementation-session evidence is retained under `/workspace/.biome-cloud/knip-design/` in `b2-oracle-probes/`, `b2-policy-probes/`, `b2-cjs-default-probes/`, and `b2-arrow-expression-probe/`. Each directory contains `report.md`, `fixtures.before.json`, `artifact-manifest.json`, `observations.json`, per-case `<id>.cli.json`, verification manifests, and the exact scratch fixture tree. Per-case CLI records retain argv, cwd, environment, exit code, raw stdout/stderr, base64 bytes and hashes, parsed output and original effective configuration. These external evidence artifacts are not embedded in this repository. A batch name and digest do not claim full raw artifact equivalence of the reduced native corpus.

The named corrections preserve reference findings while distinguishing native explicit-export precedence, unavailable-origin uncertainty, lexical require provenance, opaque exports, registry provenance, and possible unsupported registry calls. They are bounded implementation dispositions, not a claim of full Knip compatibility or acceptance of an individual migration. Existing reference observations are reused without rerunning the oracle.

Static CommonJS objects retain named property origins alongside their whole-object default export. Their named facts remain available for resolution/member analysis while the pinned export-reporting policy reports the default slot. A property literally named `default` currently collides with that whole-object slot in the snapshot representation. Native controls retain both identities, bounded uncertainty, incomplete coverage, and provenance even for a side-effect-only consumer. This is a representation limitation, not JavaScript semantic ambiguity or a reference parity claim; workflows affected by it require explicit migration mapping or further representation support.

The expression-arrow observation retains both its parameter type and asserted result type. The native `Signature` role includes syntactically collected return-expression dependencies; it does not mean only explicit annotation sites. The earlier observed block-body annotations follow a different traversal rule. This distinction requires no whole-module type inference.

## Native precision limits outside the reference corpus

Used dynamic-import results currently retain uncertainty over the resolved target's export surface and origins. The native engine does not yet recover precise bindings or member selections from those results, so affected exports remain protected from definitive unused findings without being marked definitely consumed. Directly discarded import expressions, including parentheses and `await`, only retain module reachability.

Custom-element registration requires a proven local constructor identity for definite consumption. Conditional, reassigned, destructured, and imported constructor identities can instead require unbounded uncertainty. Ordinary import consumption remains independent, but even a simple imported-class registration can have incomplete coverage until graph-origin constructor proof is available. Pre-bound registry methods remain possible registrations; spreads or mutable aliases invalidate a positional constructor bound.

These are conservative native precision limits, not additional observed Knip differences or changes to the 40 reference projections. Capability support alone does not qualify affected workflows for migration acceptance (D4); their uncertainty and limitations require explicit review.

## Namespace/member reference corpus

`members-oracle.json` retains 54 distinct observations: 24 namespace option configurations and eight duplicate forms from `b3-oracle-probes`, 20 member configurations from `b3-member-probes`, and two corrected-scope public namespace configurations from `b3-public-namespace-probes`. These existing observations were reused without new oracle execution. The separate source pin and artifact identities above also apply to this corpus.

Each record preserves exact source bytes and hashes, original full-fixture digest, raw issue items including reference positions, and the unmodified path/category/symbol projection. `native_config` keeps supported options from the original configuration; `config_sha256` hashes its original complete configuration object using sorted keys and compact JSON separators. Neither reduced object is the original knip.json byte representation. All original raw execution artifacts remain in the named directories under `/workspace/.biome-cloud/knip-design/`.

The native source test compares all issue path/category/symbol triples, plus member and namespace finding start positions. Duplicate groups use sorted names joined by `/`; native issues expose one primary location and evidence rather than the reference reporter's nested group shape. This does not assert full reporter, coverage, or evidence parity. The original narrow-entry public namespace observations remain distinct from the two corrected-entry observations; their unreachable main.ts finding is preserved.

Namespace-only fallback is determinate profile evidence and does not turn arbitrary computed accesses, used dynamic-import results, or possible imported registrations into definite member consumption. Member uncertainty remains explicit even for context providers, and report selection retains its payload. Local member references protect their own declarations independently of parent-export local-use options. Member deletion and enum-numbering fix preconditions are not implemented.

`AnalysisOptions.ignore_members` applies member-name glob patterns in the owned engine. Configuration already retains `ignoreMembers`; effective configuration-to-analysis option translation remains owned by the later configuration integration task. Capability flags describe implemented native checks, not compatibility qualification. No compatibility-ledger status or migration acceptance claim is changed by these comparisons.
