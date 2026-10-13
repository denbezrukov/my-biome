---
"@biomejs/biome": patch
---

Fixed parsing and formatting of SCSS `@at-root` statements directly inside `@keyframes` bodies. Ordinary selectors and media rules inside `@at-root` now retain their structure alongside the surrounding keyframe steps. Missing bodies are reported without consuming following keyframe steps or rules.
