"""Reconstruct the recorded inventory from authenticated immutable inputs.

This is a new recovery helper. It does not execute historical command text.
It writes only in this recovery directory.
"""
from pathlib import Path
import hashlib
import json

ROOT = Path('/workspace/.biome-cloud/native-recovery')
OUT = ROOT / 'recovered-reference-regeneration'
TREE = Path('/tmp/recovery-reference/tree.json')
SELECTED = ROOT / 'recovered-a1-review/xtask/project_analysis/reference/tests.selected_semantics.partial.json'
COMMIT = '07121567e60b73d1e640f395028c525a2b1d9778'
TREE_SHA256 = 'e26fe31bc43594cc076d8662f15e7ab895aa07cfbcfb1462df553911ad60b026'

tree_bytes = TREE.read_bytes()
assert hashlib.sha256(tree_bytes).hexdigest() == TREE_SHA256
tree = json.loads(tree_bytes)
assert not tree['truncated']
tests = [item for item in tree['tree'] if item['path'].startswith('packages/knip/test/') and item['path'].endswith('.test.ts')]
assert len(tests) == 828
selected = json.loads(SELECTED.read_text())
assert len(selected) == 34
owners = {
    'entry': 'B1', 'infra': 'B1', 'exports': 'B2', 're-exports': 'B2',
    'imports': 'B2', 'namespaces': 'B3', 'types': 'B2', 'dependencies': 'B5',
    'resolution': 'B6', 'workspaces': 'B5', 'ignore': 'B7',
    'ignore-exports-used-in-file': 'B7', 'tags-hints': 'B7',
    'graph-explorer': 'B2', 'cli': 'C4', 'fix': 'D2', 'session': 'A5',
    'util': 'B7', 'e2e': 'D4',
}
rows = []
for item in tests:
    path = item['path']
    group = path.split('/')[3]
    unsupported = group in ['plugins', 'plugin-config', 'compilers'] or any(
        value in path for value in ['custom-compiler', 'from-plugin', 'svelte', 'compilers', 'plugin-', 'preprocessor']
    )
    status = 'unsupported' if unsupported else 'untested'
    owner = owners.get(group, 'deferred plugins')
    if unsupported and group != 'plugins':
        owner = 'deferred extensions'
    if any(value in path for value in ['catalog', 'cycles', 'circular-dependencies']):
        owner = 'B6'
    elif any(value in path for value in ['namespace', 'enum-members', 'duplicate-exports']):
        owner = 'B3'
    elif any(value in path for value in ['get-inputs-from-scripts', 'util/scripts', 'util/npx', 'util/npm-exec', 'binary-policy', 'worker-path', 'url-import-meta', 'module-register']):
        owner = 'B4'
    rows.append({'path': path, 'git_blob': item['sha'], 'owner': owner, 'status': status})

document = {
    'source_commit': COMMIT,
    'tree_sha256': TREE_SHA256,
    'scope': 'Test-path inventory; unported groups have not been executed. Selected test titles and negative assertions are source research, not native coverage.',
    'groups': rows,
    'selected_semantics': selected,
}

def write_stage(stage, expected_prefix):
    data = (json.dumps(document, indent=2) + '\n').encode()
    blob = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
    path = OUT / stage / 'xtask/project_analysis/reference/tests.json'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    result = {'stage': stage, 'bytes': len(data), 'lines': len(data.splitlines()), 'sha256': hashlib.sha256(data).hexdigest(), 'git_blob_sha1': blob, 'expected_historical_blob_prefix': expected_prefix, 'historical_blob_prefix_matches': blob.startswith(expected_prefix)}
    print(json.dumps(result))
    return result

results = [write_stage('3d051362-initial', 'a07cd5bf773')]
assert results[0]['historical_blob_prefix_matches'], 'Initial original blob mismatch; stop before ownership corrections'

# Exactly the nine owner replacements shown in round1-ownership.diff.txt.
language_owners = {
    'commonjs-tsconfig': 'B2', 'commonjs': 'B2', 'custom-elements': 'B2',
    'js-only': 'B1', 'jsdoc-exports': 'B2', 'jsdoc-import-tag': 'B2',
    'jsdoc': 'B5', 'jsx': 'B2', 'tagged-template-literal': 'B2',
}
for name, owner in language_owners.items():
    row = next(item for item in document['groups'] if item['path'] == f'packages/knip/test/language/{name}.test.ts')
    assert row['owner'] == 'deferred plugins'
    row['owner'] = owner
results.append(write_stage('b292e878-final', 'c1cc14c60b6'))
assert results[-1]['historical_blob_prefix_matches'], 'Final original blob mismatch'
(OUT / 'regeneration-results.json').write_text(json.dumps(results, indent=2) + '\n')
