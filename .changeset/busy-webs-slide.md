---
"@biomejs/biome": patch
---

Fixed parsing and formatting of standalone Sass interpolation in modern `if()` conditions, such as `if(#{$condition}: red; else: blue)`. Interpolation also works inside parentheses and with `not`, `and`, and `or`, while preserving legacy Sass `if()` calls.
