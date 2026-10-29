"""Reconstruct recovery artifacts only; never imports or runs project code."""

import hashlib
import json
from pathlib import Path
import re

root = Path(__file__).resolve().parent
base = root.parent / "recovered-b5-fix1-review"
destination = root / "final-74f3d041"


def blob_hash(content):
    header = b"blob " + str(len(content)).encode() + b"\0"
    return hashlib.sha1(header + content).hexdigest()


def apply_literal_diff(original, patch):
    source = original.decode().splitlines(keepends=True)
    diff = patch.decode().splitlines(keepends=True)
    output = []
    cursor = 0
    i = 0
    while i < len(diff):
        match = re.match(r"@@ -(\d+),(\d+) \+(\d+),(\d+) @@", diff[i])
        if match is None:
            i += 1
            continue
        old_start, old_count, new_start, new_count = map(int, match.groups())
        assert old_start - 1 >= cursor
        output.extend(source[cursor:old_start - 1])
        cursor = old_start - 1
        assert len(output) == new_start - 1
        removed = added = 0
        i += 1
        while i < len(diff) and not diff[i].startswith("@@ "):
            line = diff[i]
            assert line[0] in " +-", repr(line)
            if line[0] in " -":
                assert source[cursor] == line[1:], (cursor + 1, source[cursor], line)
                cursor += 1
                removed += 1
            if line[0] in " +":
                output.append(line[1:])
                added += 1
            i += 1
        assert (removed, added) == (old_count, new_count)
    output.extend(source[cursor:])
    return "".join(output).encode()


records = []
for relative, patch_name, old_prefix, new_prefix in [
    ("crates/biome_project_analysis/src/dependencies.rs", "engine-dependencies.diff", "dca4f4809b0", "c86322efc16"),
    ("crates/biome_service/src/project_analysis/dependencies.rs", "service-dependencies.diff", "7b755a445c2", "4a4dc37304b"),
    ("crates/biome_service/src/project_analysis/dependencies.tests.rs", None, None, None),
]:
    original = (base / relative).read_bytes()
    if patch_name:
        assert blob_hash(original).startswith(old_prefix), (relative, blob_hash(original))
        result = apply_literal_diff(original, (root / patch_name).read_bytes())
        assert blob_hash(result).startswith(new_prefix), (relative, blob_hash(result))
    else:
        assert len(original.splitlines()) == 1511
        addition = (root / "dependencies.tests.append.rs").read_bytes()
        assert len(addition.splitlines()) == 173
        result = original + addition
    target = destination / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(result)
    records.append({"path": relative, "bytes": len(result), "lines": len(result.splitlines()), "sha256": hashlib.sha256(result).hexdigest(), "git_blob_sha1": blob_hash(result), "matches_retained_diff_blob_prefix": new_prefix})
(root / "reconstructed-sources.json").write_text(json.dumps(records, indent=2) + "\n")
print(json.dumps(records, indent=2))
