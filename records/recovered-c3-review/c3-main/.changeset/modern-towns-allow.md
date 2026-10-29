---
"@biomejs/biome": minor
---

Added editor diagnostics for experimental project analysis when `projectAnalysis.enabled` is `true`, including findings in closed files and package manifests. Diagnostics use captured source locations and clear when their project inputs become stale. Project analysis remains partial; framework and tool plugins are not supported.
