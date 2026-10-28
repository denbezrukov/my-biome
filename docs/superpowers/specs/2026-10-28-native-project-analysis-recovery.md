# Native project analysis: approved scope and recovery

This records the requirements retained from the approved design after the original cloud workspace became unavailable. It is a reconstruction from the conversation, not a byte-for-byte recovery of the missing design or implementation. Previous test results do not validate reconstructed code.

The goal is to let a project replace Knip with Biome without losing any workflow that the project uses. Implementation is native Rust and reuses Biome's scanner, module graph and resolver. CLI, service and editor consume the same completed, revisioned analysis result. Do not introduce another resolver, disconnected import graph, runtime Node bridge or plugin engine.

All 17 reference issue categories require explicit implementation and conformance status. A declared category is not an implemented check. Reference source and a published artifact are distinct identities; the shared development reference is Knip 6.43.0. A consuming project's actual pinned executable, configuration and invocations must be validated before recommending removal. Passing a shared fixture never certifies that project.

Static JSON/JSONC configuration is the initial scope. Framework/tool plugins, executable configuration, custom compilers/reporters/preprocessors, non-JS preprocessing and Knip's JS API remain visible deferred requirements. Migration must preserve originals and comments, report unsupported settings, and never silently remove Knip or rewrite its invocation sites.

Coverage distinguishes complete, incomplete and failed acquisition. Unknown references and unavailable inputs remain evidence. Suppress definitive unused findings and fixes wherever uncertainty could affect their conclusion, including across workspace boundaries. Report filtering must not shrink the analysis scope or erase uncertainty. Warm and fresh results must agree at the same revision; source, file existence/membership, metadata, configuration and mode all invalidate relevant state.

Fixes require supported semantics, explicit destructive opt-in, stale-input checks, preserved side effects/comments/public APIs, and reanalysis. Changes to manifests do not silently update lockfiles. Benchmark evidence retains input/executable identity, scope and coverage; unavailable measurements are explicit.

## Delivery decision

The user requested small reviewable PRs in `denbezrukov/my-biome`. Each PR owns one independently testable contract or behavior and receives a fresh review after required verification. Push each completed slice to the fork instead of keeping the whole feature local. Dependent PRs may target their predecessor branch so each diff remains focused; no automatic merges or upstream PRs.

The first internal slice targets the fork's current `main`, following CONTRIBUTING.md for changes with no end-user behavior. User-facing integration targets the fork's feature-release line according to the same policy. Preserve existing SCSS branches and unrelated files.

## First slice

Add an internal Rust issue taxonomy and a checked-in compatibility inventory tied to immutable reference identities. Cover exactly the 17 machine keys observed in the pinned reference. Serialization is explicit and rejects unknown keys. Every category starts unimplemented and untested. This slice adds no analysis command, findings, compatibility certification or migration recommendation.

Later slices add snapshot/coverage contracts, static configuration and inventory, source facts and completed acquisition, individual analysis families, service/CLI/editor/report/watch integration, migration, guarded fixes, and actual-project qualification. These are delivery groups, not a promise that each group is only one PR. Split each group further where behavior can be reviewed independently.
