# Original C3 main Git blob evidence

The original independent review retained the exact `git diff` index lines for these files. Read-only `git hash-object --no-filters` over the newly exported literal source produced the following full object IDs; each matches the complete original 11-character index prefix. No Git object was written.

| Original path | Retained original new-blob prefix | Recovered complete blob ID |
| --- | --- | --- |
| `crates/biome_lsp/src/server.rs` | `dbe6251be76` | `dbe6251be763c9ce965c2803d7e37e2c8debbb92` |
| `crates/biome_lsp/src/handlers/text_document.rs` | `b9ba1d9b741` | `b9ba1d9b7419ae43442075dbdeea261d8612e5d0` |
| `crates/biome_lsp/src/lib.rs` | `4e1988d8062` | `4e1988d8062eb897d9fbcff0483e8644c073a96d` |
| `crates/biome_lsp/Cargo.toml` | `d554ed7ad4c` | `d554ed7ad4cd0f172473e91296d54caa7f8bcd90` |
| `.changeset/modern-towns-allow.md` | `2f3b1f1ca4d` | `2f3b1f1ca4d6af042de4648c698cf574d6445857` |

Original stage: C3 main `540d2e1a7b8bda82c42ade70459dec4975899c88`.

Full source line counts: LSP `server.rs` 1011; `handlers/text_document.rs` 290; service `project_analysis/coordinator.rs` 637; `project_analysis/transport.rs` 128.

The service files were read in full as directly affected contracts during the same review, but their original blob IDs were not displayed because they were unchanged by the C3 diff. Their exported bytes are transcribed from that retained full output and are not independently Git-verified.

Service `workspace/server.rs` excerpt filenames encode their inclusive original line ranges. They are literal source excerpts, not whole files. No missing declarations, imports, derives, or closing scopes have been invented to make the excerpts compile.

No final C1 engine derives or converter source was read in this review. No `12bf926` source was read. Those are explicit gaps.
