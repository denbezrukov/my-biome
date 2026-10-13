---
"@biomejs/biome": patch
---

Fixed parsing and formatting of bare `%` strings in Sass expressions, such as `$unit: %` and mixin arguments. Trailing percent strings in lists like `10 %` are preserved alongside modulo expressions and numeric percentages.
