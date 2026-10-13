---
"@biomejs/biome": patch
---

Fixed parsing and formatting of non-color hash values in SCSS. Values such as `#app`, `#Brand`, and `#ABCG` remain case-sensitive unquoted strings, including when used in maps or next to interpolation, while hexadecimal colors retain their existing formatting.
