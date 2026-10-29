"""Compose retained reader bytes with inert literal replacement arguments only."""
from pathlib import Path
import ast,hashlib,json,re,subprocess
ROOT=Path('/workspace/.biome-cloud/native-recovery');A3=ROOT/'recovered-a3';OUT=Path(__file__).parent;SOURCE=OUT/'source'
def digest(data):return hashlib.sha256(data).hexdigest()
def blob(data):return hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
inputs={
 'full-source/workspace_metadata.rs':'c05e3053d37eac65a620a68fdaeffe94f738b5af8e05280c22c1f4ba40801a82',
 'full-source/workspace_metadata.tests.rs':'d8001d11d31d1744ccdd109b51ab9d8ef42a7cf525c229dfb52b030e13c0522b',
 'patches/002-yaml-lint-command.txt':'07c3513ceed85c289598343fa2cb872a8d6dc3291a377915ed7dd1277e3f50b7',
 'patches/003-yaml-quotes-test-append.rs.txt':'88b78ee440bedfb141ae561f60d619ce3f7e181858571f1fa3b9eb0388cb4946',
 'patches/005-yaml-quotes-command.txt':'a1957ebe372a91c2bf923c74b325f1e02e6424896b99f866f7a852520275d30f',
}
for path,expected in inputs.items():assert digest((A3/path).read_bytes())==expected,path

def literal_replacements(node):
 if not isinstance(node,ast.Call):return
 if isinstance(node.func,ast.Attribute):
  yield from literal_replacements(node.func.value)
  if node.func.attr=='replace':
   assert not node.keywords
   args=[ast.literal_eval(arg) for arg in node.args]
   assert len(args) in [2,3] and isinstance(args[0],str) and isinstance(args[1],str)
   assert len(args)==2 or args[2]==1
   yield args

reader=(A3/'full-source/workspace_metadata.rs').read_text();transitions=[]
for relative,expected_count in [('patches/002-yaml-lint-command.txt',6),('patches/005-yaml-quotes-command.txt',1)]:
 archived=(A3/relative).read_text();assert archived.startswith("python3 - <<'PY'\n")
 body=archived.split('\n',1)[1].rsplit('\nPY',1)[0]
 parsed=ast.parse(body);count=0;before=digest(reader.encode())
 for statement in parsed.body:
  if not isinstance(statement,ast.Assign):continue
  if not any(isinstance(target,ast.Name) and target.id=='s' for target in statement.targets):continue
  for args in literal_replacements(statement.value):
   old,new=args[:2];occurrences=reader.count(old);assert occurrences==1,(relative,old,occurrences)
   reader=reader.replace(old,new,args[2] if len(args)==3 else -1);count+=1
 assert count==expected_count,(relative,count)
 transitions.append({'source':'recovered-a3/'+relative,'operation':'Statically parse literal .replace arguments; apply replacements in original evaluation order. Archived statements are never executed.','replacement_count':count,'before_sha256':before,'after_sha256':digest(reader.encode()),'all_preimage_literals_matched_once':True})

baseline=(A3/'full-source/workspace_metadata.tests.rs').read_bytes();append=(A3/'patches/003-yaml-quotes-test-append.rs.txt').read_bytes();tests=baseline+append
assert tests.decode().count('#[test]')==3
names=re.findall(r'#\[test\]\s*fn ([a-zA-Z0-9_]+)',tests.decode());assert names==['workspace_metadata_preserves_empty_providers_and_ranges','workspace_metadata_diagnoses_non_string_selectors_and_versions','workspace_metadata_preserves_escaped_quote_boundaries']
paths={
 'crates/biome_package/src/node_js_package/workspace_metadata.rs':reader.encode(),
 'crates/biome_package/tests/workspace_metadata.rs':tests,
}
records=[]
for path,data in paths.items():
 target=SOURCE/path;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
 records.append({'path':path,'bytes':len(data),'lines':len(data.splitlines()),'sha256':digest(data),'git_blob_sha1':blob(data),'independent_original_final_hash_available':False,'stage':'Original A3 reader plus its recorded lint and quote correction literals; final formatter output not authenticated.'})
manifest={
 'status':'coherent-recovered-A3-workspace-reader-needs-new-registration',
 'original_A3_initial':'52708197fbcac4abe8231e6c36bda61db6ff4ce2',
 'original_A3_final':'af4b6125c3db4cb82953be7d2e4440c398b39add',
 'base_input_provenance':{'reader':'Original tool-output c3fd82,244-line intermediate reader','tests':'Original tool-output eb4dcd,complete two-test baseline','corrections':'Literal archived lint replacements002,quote regression003,andquote decoder005; no archived command execution.'},
 'input_copy_integrity':[{'path':'recovered-a3/'+p,'sha256':v} for p,v in inputs.items()],
 'source_transformations':transitions,
 'files':records,
 'required_new_integration':[
  {'path':'crates/biome_package/src/node_js_package/mod.rs','action':'Declare workspace_metadata submodule and re-export WorkspaceMetadata/read_workspace_metadata at node_js_package scope.','original_bytes_recovered':False},
  {'path':'crates/biome_package/src/lib.rs','action':'Re-export WorkspaceMetadata and read_workspace_metadata from the crate root alongside other node_js_package exports.','original_bytes_recovered':False}],
 'prerequisites':['A2 MetadataValue re-export in node_js_package','Existing biome_package YAML parser/syntax,JSON parser/value and deserialize dependencies; no new dependency identified.'],
 'expected_verification':[
  {'command':'source /workspace/.biome-cloud/activate.sh && cargo test -p biome_package --test workspace_metadata','expected_count':3,'tests':names,'expected_behavior':'Empty providers/ranges preserved; invalid selector/version YAML diagnosed; escaped single-quote boundaries retained.'},
  {'command':'source /workspace/.biome-cloud/activate.sh && cargo test -p biome_package','expected':'Full package regression suite passes.'},
  {'command':'source /workspace/.biome-cloud/activate.sh && just f && just l','expected':'Formatting/lint gates succeed before commit.'}],
 'verification_status':'Input checksums,literal matching,and output checksums only. No builds/tests or Git mutations.',
 'limitations':['Neither final full source has an independently retained original final hash. Current hashes establish copy/derivation integrity only.','Final rustfmt output is not reconstructed; parent must format and inspect the actual continuation.','The two registration edits are intentionally not materialized because their original bytes are missing. Parent must record those edits as new integration.','No workspace discovery/service behavior is included or claimed.'],
}
(OUT/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
(OUT/'SOURCE-SHA256SUMS').write_text(''.join(f"{r['sha256']}  source/{r['path']}\n" for r in records))
(OUT/'ASSESSMENT.md').write_text('''# A3 workspace metadata reader subunit

The source overlay has two recovered files: the workspace reader under biome_package/src/node_js_package and its three-test integration suite under biome_package/tests. The244-line baseline is transformed only by the six literal lint replacements and one literal quote-boundary replacement retained in the original history. The third test is the exact retained append. No archived command is executed: Python AST parsing extracts constant replacement operands, verifies every old string occurs exactly once, and applies those strings in order.

These are coherent recovered reader semantics before fresh formatting. No independent historical final whole-file hashes survive for the reader/tests, so current hashes are not a claim of exact original final formatting.

## New integration required

The original module-registration/re-export patches are absent. Do not claim those edits as restored original bytes. The parent needs to add a workspace_metadata module declaration and re-export WorkspaceMetadata/read_workspace_metadata in crates/biome_package/src/node_js_package/mod.rs, then expose those names through crates/biome_package/src/lib.rs. No registration files have been placed in source/.

The reader uses A2 MetadataValue through super::MetadataValue. The immutablebase6359 package Cargo.toml already has biome_yaml_parser,biome_yaml_syntax,biome_json_parser,biome_json_value,biome_deserialize and indexmap, so this subunit requires no new dependency. It does not depend on A3 filesystem identity, engine inventory or discovery service.

## Fresh verification

Run cargo test -p biome_package --test workspace_metadata after loading /workspace/.biome-cloud/activate.sh; expect exactly3 tests, not zero. They cover empty catalog providers and source ranges, invalid/non-string YAML selectors and catalog versions, and single-quoted values retaining escaped boundary quotes. Then run the entire biome_package suite, repository formatting/lint and fresh review. No fresh test/build result is claimed here.

Only copy source/ after creating the regression target; add the missing registration as explicitly new integration. All copied bytes have fresh mtimes. No product worktree, Git index or branch was changed while preparing this overlay.
''')
subprocess.run(['sha256sum','--check','--quiet','SOURCE-SHA256SUMS'],cwd=OUT,check=True)
print(json.dumps({'source_files':len(records),'bytes':sum(r['bytes'] for r in records),'literal_replacements':sum(t['replacement_count'] for t in transitions),'tests':len(names),'manifest_sha256':digest((OUT/'manifest.json').read_bytes())},indent=2))
