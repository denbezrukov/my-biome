---
"@biomejs/biome": patch
---

Fixed parsing and formatting of SCSS identifiers containing non-ASCII characters such as `«` and `»`. These characters are accepted in Sass values and names while plain CSS keeps its existing identifier rules.
