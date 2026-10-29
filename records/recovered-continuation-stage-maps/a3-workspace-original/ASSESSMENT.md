# A3 workspace metadata reader subunit

The source overlay has two recovered files: the workspace reader under biome_package/src/node_js_package and its three-test integration suite under biome_package/tests. The244-line baseline is transformed only by the six literal lint replacements and one literal quote-boundary replacement retained in the original history. The third test is the exact retained append. No archived command is executed: Python AST parsing extracts constant replacement operands, verifies every old string occurs exactly once, and applies those strings in order.

These are coherent recovered reader semantics before fresh formatting. No independent historical final whole-file hashes survive for the reader/tests, so current hashes are not a claim of exact original final formatting.

## New integration required

The original module-registration/re-export patches are absent. Do not claim those edits as restored original bytes. The parent needs to add a workspace_metadata module declaration and re-export WorkspaceMetadata/read_workspace_metadata in crates/biome_package/src/node_js_package/mod.rs, then expose those names through crates/biome_package/src/lib.rs. No registration files have been placed in source/.

The reader uses A2 MetadataValue through super::MetadataValue. The immutablebase6359 package Cargo.toml already has biome_yaml_parser,biome_yaml_syntax,biome_json_parser,biome_json_value,biome_deserialize and indexmap, so this subunit requires no new dependency. It does not depend on A3 filesystem identity, engine inventory or discovery service.

## Fresh verification

Run cargo test -p biome_package --test workspace_metadata after loading /workspace/.biome-cloud/activate.sh; expect exactly3 tests, not zero. They cover empty catalog providers and source ranges, invalid/non-string YAML selectors and catalog versions, and single-quoted values retaining escaped boundary quotes. Then run the entire biome_package suite, repository formatting/lint and fresh review. No fresh test/build result is claimed here.

Only copy source/ after creating the regression target; add the missing registration as explicitly new integration. All copied bytes have fresh mtimes. No product worktree, Git index or branch was changed while preparing this overlay.
