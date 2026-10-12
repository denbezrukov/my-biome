---
"@biomejs/biome": patch
---

Fixed parsing and formatting of computed Sass property names in `@supports` declarations and plain CSS import `supports()` clauses.

```scss
$prefix: overflow-;
$axis: x;

@supports ($prefix + $axis: clip) {
  .scroller { overflow-x: clip; }
}

@import "theme.css" supports($prefix + $axis: clip);
```
