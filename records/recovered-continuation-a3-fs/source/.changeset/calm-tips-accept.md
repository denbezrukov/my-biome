---
"@biomejs/biome": minor
---

Added `FileSystem::canonicalize_path` to the published `biome_fs` crate. Callers can resolve filesystem aliases while preserving case distinctions, with errors for missing paths and unsupported filesystems.
