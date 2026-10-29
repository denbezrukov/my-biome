# Exact A1 reference inventory recovery

Recovered `xtask/project_analysis/reference/tests.json` at both original A1 stages, including all 828 groups and 34 selected semantic records.

The newly fetched immutable Knip recursive tree body matches the original recorded tree SHA-256 exactly. The selected semantic records are literal original review output. A new, inspected recovery helper used the retained deterministic field ordering, grouping rules, and exact nine ownership corrections. It did not execute any historical commands.

Both complete resulting files match the Git blob prefixes printed in the original ownership diff:

- Initial `3d051362`: `a07cd5bf7735050e434c622e5a5ddcb17a13fbd7` (5537 lines).
- Final `b292e878`: `c1cc14c60b6826943cf2812ad4286386afae4c31` (5537 lines).

`manifest.json` distinguishes computed full hashes from the retained historical prefixes. `input-receipts.json` records immutable input provenance. `regenerate_tests.py` is a new recovery helper and expects the recorded tree body at its temporary input path. No product checkout, index, dependencies, or build state was changed.

The additional source inventory and license were already recovered by other agents; this package does not duplicate them.
