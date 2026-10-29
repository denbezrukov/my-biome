---
"@biomejs/biome": minor
---

Added dependency attribution to the partial native project analysis engine, including workspace declaration ownership, command providers, and existing public declaration requirements. Unsupported installation and publication inputs retain incomplete coverage; CLI and editor integration remain pending.

Fixed package lookup from nested source directories to prefer their nearest installed dependency, while preserving package-relative imports, self-exports, and TypeScript mappings.
