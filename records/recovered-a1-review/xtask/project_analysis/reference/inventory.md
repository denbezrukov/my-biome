# Knip parity inventory for native Biome analysis

Status: design research only. The user subsequently said **“let skip plugin for now”**. Framework/tool plugin implementation is outside phase one. This document retains the complete capability inventory as a roadmap and migration checklist; phase one must not claim full Knip replacement. Executable configuration and user extensions remain a separate capability decision, not permission implied by skipping plugins.

## Reproducible baseline

- Official repository: `webpro-nl/knip`; `main` resolved through the GitHub commit API to **`07121567e60b73d1e640f395028c525a2b1d9778`**, committed `2026-10-24T08:27:02Z`.
- Retrieval started `2026-10-25T17:45:39.272217+00:00`. Both pinned `packages/knip/package.json` and npm `latest` reported **6.43.0**. npm metadata did not expose `gitHead`: version equality does not prove the release and main have identical bytes.
- [sources.json](./sources.json) records exact source URLs, pinned GitHub links, SHA-256 hashes, byte lengths, and the complete 198-name plugin registry. Fifty bounded official source/documentation files were retrieved; no full clone, dependency install, execution of Knip, or repository test run occurred.
- Counts verified against source: **17 internal issue keys**, **198 built-in plugin names**, **10 public built-in reporters**. These are registry counts, not a claim that all 198 adapters have been behaviorally audited. Every adapter remains a separate future parity obligation.
- Documentation sometimes lags implementation. The discrepancies below must become explicit compatibility decisions and fixtures, rather than unexamined assumptions.

## Every issue category

Sources: [issue reference][issue-doc], [issue data model][issues], [filter implementation][filter], [fix implementation][fix]. “Fix” means upstream offers a fix, not that every possible syntax is safely fixable.

| Internal key | Observable finding / minimum distinguishing fixture | Default | Fix family |
|---|---|---|---|
| `files` | Project file unreachable from entries; include orphan import chains and an empty file | error | `files`, removal opt-in |
| `dependencies` | Declared production dependency with no credited reference | error | `dependencies` |
| `devDependencies` | Declared development dependency with no credited reference | error; disabled in production | `dependencies` |
| `optionalPeerDependencies` | An optional peer dependency is referenced | error | none |
| `unlisted` | Referenced package absent from the relevant accepted manifest dependency sets | error | none |
| `binaries` | Referenced executable without an acceptable providing package | error | none |
| `unresolved` | Unresolved import/reference specifier, distinct from an unlisted installed package | error | none |
| `exports` | Unused value export, including re-export chains and CommonJS conventions | error | `exports` |
| `types` | Unused exported type/interface/enum | error | `types` |
| `nsExports` | Value exports hidden by the used-namespace heuristic, considered individually when enabled | off | `exports` |
| `nsTypes` | Type exports hidden by the used-namespace heuristic | off | `types` |
| `duplicates` | Same binding exported more than once; honor `@alias` | error | none |
| `enumMembers` | Unused member of an exported enum | error | `types` |
| `namespaceMembers` | Unused member of an exported TypeScript namespace | error | `types` |
| `catalog` | Catalog package entry not referenced by manifests/overrides | error; disabled in production | `catalog` |
| `catalogReferences` | `catalog:` reference to a missing package/catalog entry | error; disabled in production | none |
| `cycles` | Representative circular runtime dependency paths; preserve edge locations/kinds | off; severity warn when enabled | none |

Public `dependencies` filtering groups `devDependencies` and `optionalPeerDependencies` outside production; rules can address their keys separately. Do not invent a `classMembers` issue: it is absent from this pinned model. Diagnostics retain type, file, workspace, symbol, parent symbol, symbol kind, import specifier, severity, positions, related symbols, available fixes and fixed status as applicable. Configuration/tag hints are additional result channels, not extra issue categories.

Cycle controls: ignore dynamic imports by default; `cycles.dynamicImports` enables them; `cycles.allow` accepts specified root-relative paths without the repeated closing vertex. Reports are representative paths, not every possible simple cycle. A compatible native implementation may choose a different valid representative traversal if normalization compares the same cycle semantics rather than text order. [cycles/reporters][reporters], [configuration][config]

## Native analysis contract and defaults

1. **Whole-project graph.** Starting from entries, follow references recursively; `unused files = project candidates − reachable files`. Credit dependencies only from relevant reachable evidence. Keep import/export analysis distinct from local unused variables. Preserve origin and scope of every root and edge so a report can explain why a file or dependency is retained. [graph][graph]
2. **Default roots and candidates.** Entries are `{index,cli,main}.{js,cjs,mjs,jsx,ts,cts,mts,tsx}!` and `src/{index,cli,main}.{js,cjs,mjs,jsx,ts,cts,mts,tsx}!` per workspace; project is `**/*.{js,cjs,mjs,jsx,ts,cts,mts,tsx}!`. Explicit user arrays replace their defaults. Manifest roots also include nonempty string `main`, `module`, `browser`, string/map `bin`, both raw `types` and `typings`, `exports` targets, and literal relative non-wildcard targets under `#` imports; scripts/plugins add their own roots. Source reference targets retain their producing module’s reachability condition. Manifest root discovery is separate from selecting one resolver condition or preferring `types` over `typings`. [entries][entries]
3. **File scope is not suppression.** `ignore` hides findings, while the file can still participate in analysis; `ignoreFiles` only suppresses the unused-file finding; `ignoreIssues` suppresses selected categories by glob. `project` controls project candidates and scope, but explicit/plugin roots can still bring excluded files into the graph. `.gitignore` affects discovery unless `--no-gitignore`; it must not be equated with report suppression. The script-parser document separately says ignored package export targets are not added: preserve a dedicated fixture for this discovery path. Never translate Knip `ignore` mechanically into Biome scanner exclusions. [scope][scope], [scripts][scripts]
4. **Syntax and reference forms.** ESM named/default/namespace/type imports and exports; aliased imports; re-exports and star barrels; side effects; CommonJS `require`/`module.exports`; dynamic `import()` where statically recoverable; `require.resolve`, `import.meta.resolve`, `new URL(..., import.meta.url)`, Worker targets, `module.register`, child-process entry paths. Account for aliases, lexical scope, type/value namespaces, namespace member access, declarations used in exported signatures, and native custom-element registration. Source scripts and framework AST patterns are separate evidence producers. [FAQ][faq], [entries][entries], [CommonJS][cjs], [custom elements][elements]
5. **Namespace semantics.** Referencing a namespace object without property access credits all its exports by default; once a specific property is accessed, members are considered separately even if the whole object is also used. `nsExports`/`nsTypes` disable the fallback heuristic for their respective kinds. This behavior requires fixtures for spread, enumeration, assignments, argument passing, `typeof`, and namespace re-export. [namespace imports][namespace]
6. **Entry export policy.** Entry exports are exempt by default. `includeEntryExports` includes ordinary entry exports and enum/namespace members. Plugin roots remain exempt unless the producer explicitly allows inclusion. Files discovered only from package scripts are also exempt unless explicitly added to `entry`. Public-library roots cannot be treated like private application roots. [configuration][config], [plugin inputs][inputs]
7. **Resolution.** TS/JS extensions and registered compiled extensions, package exports/imports and conditions, Node builtins, workspace packages, tsconfig options/paths, explicit Knip paths and plugin aliases; dependency metadata must come from installed package manifests when required (especially `bin` and bundled types). Recognize asset query suffixes and loader prefixes without treating them as literal filenames. Config-loading resolution and static source resolution are distinct systems. [FAQ][faq], [handling issues][handling]
8. **Source-to-output mapping.** Map package/build targets back to source using tsconfig source/output settings; handle wildcard exports, null exclusions, cross-workspace builds and missing generated output. A failing workspace resolution can fall back to the first existing conditional source target. Native mapping must also eventually accept plugin-provided source/output directory mappings: the actual Svelte package adapter already provides these despite older documentation saying tsconfig-only. [mapping][mapping], [Svelte package adapter][svelte-package], [plugin API][plugin-api]
9. **Type dependencies.** Distinguish runtime use, type-only use, DefinitelyTyped companions, packages bundling their own types, and types exposed through existing public declaration output. In strict mode type-only source imports can remain development dependencies, but public declaration dependencies belong in dependencies or required peers. Inspect published `main`, `types`, `typings`, `exports`, `typesVersions`, and pnpm `publishConfig` overrides. Upstream skips private workspaces and `publishConfig.directory` for this public declaration analysis. Do not invent declaration output that is absent. [handling issues][handling]
10. **Unknowns.** Computed imports, runtime config branches, opaque formats and unsupported tools cannot silently establish that a dependency is unused. First-phase support must expose a coverage/unsupported-capability result and restrict destructive fixes where evidence is incomplete. This is a proposed safety/accuracy improvement, not a claim that Knip already does it.

## Workspaces, modes, package metadata and scripts

- Discover package-manager workspaces (`package.json.workspaces`, legacy `.workspaces.packages`, `pnpm-workspace.yaml.packages`) and additional configured workspaces; require a manifest for each workspace. The implementation gives pnpm's packages list precedence over package.json workspace definitions. An integrated monorepo with one manifest is one workspace, even when it has many projects/config files. Root `entry`/`project` are ignored once workspace configuration is in use: configure `workspaces["."]`. Test inherited plugin/settings behavior, exact vs glob workspace config, nesting and ownership. [workspaces][workspaces], [integrated monorepos][integrated], [options][options]
- Workspace selectors accept names, name globs, paths, path globs, repeats and negations. Include relevant ancestors, dependencies and dependents so ownership and external usage remain accurate; strict isolation and running inside a workspace change the scope. Root duplicate dependencies used only by a descendant that declares its own copy should remain removable from root. [workspaces][workspaces], [handling issues][handling]
- Normal mode includes development/test/config usage. Production uses trailing `!` patterns, production plugin roots, the `start` script and `@internal` suppression. Prefix `!` negates; suffix `!` marks production, so `!path/**!` has both roles. Strict implies production and enforces direct workspace dependencies while considering required peers. Production hides configuration hints, `devDependencies`, `catalog`, and `catalogReferences` in source. Treat normal and production as separate useful runs. [production][production], [filters][filter], [options][options]
- Binaries are attributed through installed dependency `bin` metadata, not by assuming executable name equals package name. Track scripts from package manifests, command arguments, source calls, and eventually CI/hooks/task configs. Handle shell composition and recursive script/wrapper invocation without executing scripts. Native phase one must say which command parsers exist; most tool-specific argument parsing belongs to deferred adapters. Examples: `node --import tsx run.ts`, `tsc -p`, `nodemon --exec`, forwarded CLI arguments. [scripts][scripts], [argument API][arguments]
- Script extractors include Bun/execa/zx tagged templates, execa function families, `node:child_process` exec/spawn/execFile/fork patterns and nano-spawn. Some are built-in plugin visitors upstream; deferring plugins must not silently imply this evidence exists. Respect working-directory attribution, optional commands, on-demand commands and common global binaries; `ignoreGlobalBinaries` defaults true and can be overridden per workspace. [scripts][scripts], [config][config]
- Manifest catalogs have four precedence candidates: pnpm YAML, Yarn YAML, package.json, nested package.json workspaces catalogs. Cover default and named catalogs, references in dependencies/dev/peer/optional dependencies, resolutions, root pnpm overrides and pnpm-workspace overrides; missing references point to the consumer. [catalogs][catalogs]

## Configuration and CLI compatibility surface

Static configuration locations: `knip.json`, `knip.jsonc`, `.knip.json`, `.knip.jsonc`, then executable `knip.ts`, `knip.js`, `knip.config.ts`, `knip.config.js`, or `package.json#knip`; explicit `--config` overrides lookup. Implementation shallow-merges manifest Knip configuration with the selected file. JSONC comments/trailing commas and schema validation matter. Executable configs export an object or synchronous/asynchronous function; source passes parsed CLI options to the function. `defineConfig` from `knip/config` is an identity helper; removal must migrate that runtime import and Knip type imports. Actual RegExp values and callbacks cannot be faithfully converted to JSON. [overview][overview], [dynamic config][dynamic], [options][options], [schema][schema]

Root schema keys, besides the 198 named plugin keys: `$schema`, `rules`, `entry`, `project`, `paths`, `ignore`, `ignoreFiles`, `ignoreBinaries`, `ignoreGlobalBinaries`, `ignoreDependencies`, `ignoreMembers`, `ignoreUnresolved`, `ignoreExportsUsedInFile`, `ignoreIssues`, `cycles`, `ignoreWorkspaces`, `includeEntryExports`, `compilers`, internal legacy `asyncCompilers`, `tags`, `treatConfigHintsAsErrors`, `treatTagHintsAsErrors`, `preprocessor`, `preprocessorOptions`, `include`, `exclude`, `workspaces`. [schema][schema]

The **actual workspace schema** accepts named plugin keys plus `entry`, `project`, `paths`, `ignore`, `ignoreFiles`, `ignoreBinaries`, `ignoreGlobalBinaries`, `ignoreDependencies`, `ignoreMembers`, `ignoreUnresolved`, `ignoreExportsUsedInFile`, `ignoreIssues`, `includeEntryExports`. It does not accept every other root key despite broad prose in the reference. Validate migration against schema, not that prose. [schema][schema]

Additional configuration semantics: string/array glob normalization; regex string and executable RegExp ignores; production-only ignore suffixes; ignore-workspace ordered negations; symbol-kind selective `ignoreExportsUsedInFile` (`class`, `enum`, `function`, `interface`, `member`, `namespace`, `type`, `variable`); severity `error`/`warn`/`off`; `@public`/`@beta`, production `@internal`, duplicate `@alias`, arbitrary include/exclude tags, regular block/standalone line comments, combined adjacent comments, excluded trailing comments, and stale-tag hints. [configuration][config], [tags][tags]

All parsed CLI flags, grouped by behavior; Biome may expose idiomatic equivalents, but migration must produce an explicit mapping rather than silently drop any used option. [CLI source][cli-args]

| Group | Flags |
|---|---|
| Basic/config | `--help/-h`, `--version/-V`, `--no-progress/-n`, `--config/-c`, `--use-tsconfig-files`, `--tsConfig/-t` |
| Mode | `--cache`, `--cache-location`, `--include-entry-exports`, `--no-gitignore`, `--production/-p`, `--strict/-s`, `--watch/-w` |
| Scope/filter | `--workspace/-W`, `--directory/-D`, `--include`, `--exclude`, `--dependencies`, `--exports`, `--files`, `--cycles`, `--tags` |
| Fix | `--fix/-f`, `--fix-type`, `--allow-remove-files`, `--format/-F` |
| Output | `--preprocessor`, `--preprocessor-options`, `--reporter`, `--reporter-options`, `--no-config-hints`, `--no-tag-hints`, `--treat-config-hints-as-errors`, `--treat-tag-hints-as-errors`, `--max-issues`, `--max-show-issues`, `--no-exit-code` |
| Diagnostics | `--debug/-d`, `--memory`, `--memory-realtime`, `--performance`, `--performance-fn`, `--duration/-u`, `--trace`, `--trace-dependency`, `--trace-export`, `--trace-file` |

- Repeat/comma semantics apply to include/exclude/fix types; reporters/preprocessors/workspaces can repeat. CLI filtering overrides conflicting config filters. Including only `nsExports`/`nsTypes` adds them to defaults; `cycles` alone narrows to cycles. Actual `--exports` uses `exports,types,enumMembers,namespaceMembers,duplicates`, while CLI help incorrectly also names `nsExports,nsTypes`. [filters][filter], [CLI source][cli-args]
- `--use-tsconfig-files` uses files/include/exclude plus project references and excludes node_modules/outDir, with normal project fallback where tsconfig is absent. These are intentional program files: upstream does not report them as unused files. Sessions without Knip config enable this implicitly. This differs materially from discovery mode. [CLI reference][cli-doc], [FAQ][faq], [options][options]
- Exit behavior: 0 success; 1 counted errors above threshold or opted-in visible hints; 2 configuration-load/internal analysis errors, even under `--no-exit-code`. The CLI argument-parser catch actually exits 1 on malformed flags: document as a source/documentation discrepancy. Warn findings do not count against `max-issues`; `max-show-issues` affects display only. Fixes alter remaining counters before exit evaluation. [CLI][cli], [fix][fix]
- `NO_COLOR`, CI/TTY progress, alternative Bun launcher and Node loader/env setup are existing operational affordances. Rust internals need equivalent observability, not Node-specific heap/timer names or the `KNIP_DISABLE_RAW_TRANSFER` workaround. Preserve capabilities relevant to CI without claiming byte-for-byte CLI identity. [CLI reference][cli-doc], [known issues][known]

## Output, fixes, watch and integrations

- Ten public reporters: `symbols` (default), `compact`, `codeowners`, `cycles`, `disclosure`, `codeclimate`, `json`, `markdown`, `sarif`, `github-actions`. Preserve structured issues, locations, severities, related cycle edges, owner attribution and format contracts where users consume them; terminal glyph/spacing identity is not required. JSON top level is `{ "issues": [...] }`, grouped by relative file path, with keys for every enabled issue type (empty arrays included), and item names/namespace/positions. SARIF is 2.1.0. [reporter registry][reporter-index], [reporter guide][reporters]
- Local/npm JS/TS reporters receive `ReporterOptions`; preprocessors return the same result shape and run in order before reporters. Source additionally runs preprocessing **before fixes** and computes error totals from final counters. A compatibility bridge must preserve Maps/Sets/regex/position conventions or explicitly translate them; ordinary JSON serialization is insufficient for the existing typed object API. No built-in preprocessors. This is deferred executable extension compatibility, not native graph analysis. [issue API][issues], [reporters][reporters], [CLI][cli]
- Fix families: dependencies removes dependency/devDependency manifest entries; exports removes export modifiers/default keywords/export-list and re-export items; types also covers enum/namespace members; catalog removes unused JSON/YAML catalog entries; files requires explicit removal permission. Source batches disjoint edits, preserves a module marker if removing the last import/export, marks fixed findings and adjusts counters. Formatting delegates to installed Biome/deno/dprint/Prettier via Formatly. Package-manager lockfile refresh is a separate step. No automatic installation of unlisted packages or binary providers; no arbitrary duplicate winner selection. [fix guide][fix-doc], [fix source][fix]
- **Do not copy unsafe fixes.** The official auto-fix page acknowledges CommonJS assignment removal can delete RHS side effects. Native fixes must preserve effects/module semantics and enum numbering, validate overlapping edits, and withhold unsafe deletion. Benchmark desired code behavior, not exact buggy upstream output. [fix guide][fix-doc]
- Watch updates source import/export reports on changes/additions/removals; upstream documents that manifest/node_modules changes may not refresh. Cache uses mtime+size and can be stale after config/alias/unresolved-target/gitignore changes. Native watch/cache must match a fresh native full scan after all relevant invalidations; stale Knip behavior is an acknowledged limitation, not a compatibility requirement. [CLI reference][cli-doc], [known issues][known]
- Configuration hints include unconfigured project/workspace, unused root workspace settings, stale workspace/ignore entries, empty/redundant patterns, missing package entries, unregistered/excluded compiler extensions. Tag hints detect now-unneeded suppressions. Include provenance, separate suppression controls and optional CI failure. [hint model][issues], [hint guide][hints]
- Public package entrypoints include `knip`, `knip/config`, `knip/session`; the session API exports graph-backed file/package descriptors, usage and contention types. Knip integrations offer diagnostics, fixes, usage hovers, import/export trees, dependency hovers, circular edges and ambiguous/shadowed/converged re-export contention, plus MCP configuration tools. A project using these has a wider replacement requirement than its CLI script. These integrations are roadmap capabilities, not automatically phase-one deliverables. [package][package], [session][session], [integrations][integrations], [language server][lsp]

## Deferred adapter and extension inventory

**No framework/tool plugin implementation in phase one.** The pinned `pluginNames` array has 198 entries, all listed in [sources.json](./sources.json) and linked to [registry source][plugin-names]. A future adapter matrix must have one row per name, version-sensitive config variants, evidence inputs, reference fixture IDs, native owner, and parity status. Supporting only Next/Vite/React would not satisfy this full registry obligation.

For each adapter, inventory each exported behavior rather than equating a default glob with a complete plugin: enablement by dependency/manifest/context; root-only gating; config locations including manifest subpaths; normal/production entries and project patterns; config-load policy and fallback; async setup; `resolveConfig`, `resolveFromAST`, `resolve`; source/output maps; transitive filtering; compiler registration; AST visitor registration; binary names and argument grammars. Config formats include JSON/YAML/TOML/JS/TS and extensionless plain text. Recursive outputs can activate other config readers. [plugin API][plugin-api], [plugin tutorial][plugin-writing]

Typed evidence must cover entry, production entry, project, dependency, production dependency, deferred file/package resolution, entry-only deferred resolution, config handoff, binary, alias. Preserve directory/workspace, optionality, type-only use, production status, and permission to include root exports. Argument grammars cover aliases, booleans/strings, raw-arg transforms, config flags/transforms, command forwarding, Node preload/loader args, positional entries, resolver args and custom extracted inputs. [inputs][inputs], [arguments][arguments]

Non-JS extraction remains independent from framework roots. Official compiler docs list `.astro`, `.css` (Tailwind-enabled), `.mdx`, `.prisma`, `.sass`, `.scss`, `.less`, `.styl`, `.stylus`, `.svelte`, `.tsrx`, `.vue`; source additionally registers `.marko` via its plugin, so the docs list is not a verified exhaustive compiler count. Default lightweight extractors primarily recover imports, not complete export semantics. Dependency-driven activation, explicit `true`, user overrides, custom extensions and a no-op compiler for tracking assets are capabilities. Custom compilers are `(source, filename) => string | PromiseLike<string>` and legacy `asyncCompilers` still normalizes in source. Do not conflate upstream extractor limitations with Biome's ability to parse richer syntax. [compilers][compilers], [compiler normalization][compiler-index], [Marko adapter][marko]

The runtime boundary remains explicit: native Rust owns parsing, resolution, graph, reachability, diagnostics and fixes. A future Node helper, if authorized, may evaluate executable configs and invoke user compiler/reporter/preprocessor callbacks with a versioned protocol. Static analysis alone cannot reproduce arbitrary JS effects, filesystem/network reads, environment branches or asynchronous functions. Deferred adapters may still need evaluated framework objects. Until that boundary is accepted and implemented, a migration using these features is incomplete.

## Differential acceptance and removal gate

Create a checked-in compatibility ledger keyed by this source pin and target release; run the pinned reference only in future dedicated fixture work. No reference execution has been performed for this design.

1. **Issue conformance:** every one of the 17 keys has positive, negative, suppression, severity, location and workspace-ownership fixtures where applicable. Test category interaction (dead files/dependencies, namespace fallbacks, type/enum classification, duplicate aliases, optional peers, catalog ownership, cycle dynamics).
2. **Graph evidence:** assert roots and their reasons, resolved edges and their kinds, manifests, dependency/bin attribution, re-export provenance and unreachable sets. Compare normalized graph evidence as well as final issue sets so a zero-issue report cannot hide missing analysis.
3. **Configuration matrix:** defaults vs replacement arrays; static/config manifest precedence; every schema key and used CLI flag; package vs integrated monorepos; named/path/glob/negative workspace selectors; normal/production/strict; discovery vs tsconfig file mode; ignore vs project vs gitignore; executable features produce explicit unsupported results until implemented.
4. **Resolution matrix:** ESM/CJS/type-only, aliases, exports/imports conditions, scoped package subpaths, builtins, bundled vs external types, pnpm-style topology, symlink paths, Windows paths and casing, source/output maps, absent/generated declarations and build outputs, query/loader syntax, every supported script evidence producer.
5. **Adapter matrix:** after the deferred phase begins, one row for each of the 198 registry names, including plugin enable/disable/override, config branches and recursive config handoff. Port documented behavior and applicable reference fixtures; independently inspect the tool's true semantics where reference outputs conflict. Missing adapters block the full-replacement claim for projects that use them.
6. **Fix semantics:** parse and validate changed files, preserve runtime side effects/public declarations/module status/enum values, make edits idempotent, ensure finding removal on a fresh scan, restrict deletion to explicit consent and complete coverage, verify comments and manifest/catalog integrity. Exact whitespace is not the oracle.
7. **Incremental consistency:** edit/add/delete/rename source, exports, manifest deps/bin/scripts/workspaces, configs, tsconfig references/aliases, gitignore, package metadata, compiler settings and generated files; incremental results equal a fresh scan. Record invalidation dependencies. Warm/cold results must be equivalent.
8. **Output and CI:** normalize issue ordering and absolute paths while preserving file, key, symbol, kind, namespace, severity and location; assert reporter schema/owner data, preprocess ordering, error counts/thresholds, hint behavior and exit classifications. All enabled categories must be represented even if empty where the format requires it.
9. **Differences require adjudication:** label each mismatch as native defect, intentional supported improvement, upstream limitation/bug, source/documentation drift, or out-of-scope/deferred capability. Require a fixture, rationale and migration note for intentional differences. A bug-compatible implementation is not the goal.
10. **Project-specific removal gate:** inventory actual Knip version/config/scripts/workspaces/plugins/compilers/reporters/preprocessors/CI/editor/API usage; prepare a concrete migration report listing mappings and unsupported features; run native and reference baselines on the same dependency/build/config environment; explain every material delta; exercise existing CI/build/tests after eventual migration. Only remove Knip when every capability the project relies on is preserved or the user explicitly accepts a named loss. Phase-one native core alone does not meet that gate for plugin-dependent projects.

## Source/documentation discrepancies to carry into the design

- Actual `--exports` excludes namespace add-ons; CLI help says otherwise.
- Production filter source removes catalog categories too; production prose mainly discusses devDependencies.
- Workspace schema is narrower than the reference's “all except three root-only options” statement.
- CLI parser errors exit 1, despite generic bad-input documentation saying 2.
- Source supports plugin source maps (e.g. `@sveltejs/package`) although mapping prose says tsconfig-only.
- Marko compiler is present in source but omitted from the compiler list.
- FAQ lists exposing a programmatic API/graph as future ideas despite exported session interfaces and active editor integrations.

These observations are based on pinned code/documentation reading; they are not runtime tests. Resolve ambiguities through targeted reference fixtures during implementation and keep design claims narrower than evidence.

## Pinned sources

[package]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/package.json
[issue-doc]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/reference/issue-types.md
[issues]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/types/issues.ts
[filter]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/util/get-included-issue-types.ts
[fix]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/IssueFixer.ts
[reporters]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/features/reporters.md
[config]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/reference/configuration.md
[graph]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/explanations/how-knip-works.md
[entries]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/explanations/entry-files.md
[scope]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/guides/configuring-project-files.md
[scripts]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/features/script-parser.md
[faq]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/reference/faq.md
[cjs]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/guides/working-with-commonjs.md
[elements]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/features/custom-elements.md
[namespace]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/guides/namespace-imports.md
[inputs]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/writing-a-plugin/inputs.md
[handling]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/guides/handling-issues.mdx
[mapping]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/features/source-mapping.md
[svelte-package]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/plugins/sveltejs-package/index.ts
[plugin-api]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/types/config.ts
[workspaces]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/features/monorepos-and-workspaces.md
[integrated]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/features/integrated-monorepos.md
[options]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/util/create-options.ts
[production]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/features/production-mode.md
[arguments]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/writing-a-plugin/argument-parsing.md
[catalogs]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/features/catalogs.md
[overview]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/overview/configuration.md
[dynamic]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/reference/dynamic-configuration.mdx
[schema]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/schema/configuration.ts
[tags]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/reference/jsdoc-tsdoc-tags.md
[cli-args]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/util/cli-arguments.ts
[cli-doc]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/reference/cli.md
[cli]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/cli.ts
[known]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/reference/known-issues.md
[reporter-index]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/reporters/index.ts
[fix-doc]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/features/auto-fix.mdx
[hints]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/reference/configuration-hints.md
[session]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/session/index.ts
[integrations]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/reference/integrations.md
[lsp]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/language-server/README.md
[plugin-names]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/types/PluginNames.ts
[plugin-writing]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/writing-a-plugin/index.md
[compilers]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/docs/src/content/docs/features/compilers.md
[compiler-index]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/compilers/index.ts
[marko]: https://github.com/webpro-nl/knip/blob/07121567e60b73d1e640f395028c525a2b1d9778/packages/knip/src/plugins/marko/index.ts

## Research correction, 2026-10-26

The default entry patterns above now retain the trailing production marker omitted in the initial prose inventory. Pinned `ConfigurationChief.ts:42–46` marks both defaults with `!`; `WorkspaceWorker.ts:223–227` selects production entries using that marker. This is a source-verified correction, not an executed conformance result.

The manifest-entry inventory was expanded after tracing `util/package-json.ts:249–287` through `graph/build.ts:214–218`. Both raw type fields and literal local `#` import targets are discovery inputs; `graph/build.ts:261` also credits bare package targets from `#` imports independently of source usage, without itself issuing `unlisted`. These are source-verified planning obligations; runtime oracle fixtures remain future implementation work.
