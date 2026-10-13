---
"@biomejs/biome": patch
---

Fixed parsing and formatting of `at-rule()` supports queries in CSS and SCSS, including `at-rule(@function)` and arbitrary at-rule names. Balanced fallback queries such as `at-rule(@ property)` and `at-rule(@/**/property)` retain their token gaps when formatted.

```css
@supports at-rule(@function) {
  .card { padding: --double(8px); }
}
```
