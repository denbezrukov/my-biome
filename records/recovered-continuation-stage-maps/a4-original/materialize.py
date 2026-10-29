"""Compose coherent A4 final source from exact retained files and verified diffs."""
from pathlib import Path
import hashlib,json,re,subprocess,shutil
ROOT=Path('/workspace/.biome-cloud/native-recovery');A4=ROOT/'recovered-a4';OUT=Path(__file__).parent;SOURCE=OUT/'source'
BASE='6359a9b03670e85b50785d59d10eb087cf89fe3f';MAIN='c86455d85ecaacb6d76a3d09691882fd9a7ac1d9';FINAL='3c28dae197840dd6adee956e7d884531a073f7cc'
state={};records={};transitions=[]
def blob(data):return hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
def base_file(path):return subprocess.check_output(['git','show',BASE+':'+path],cwd='/workspace/my-biome')
def sections(patch):
 for part in re.split(r'(?=^diff --git )',patch,flags=re.M):
  if part.startswith('diff --git '):yield part.splitlines()[0].split(' b/')[1],part

def apply(raw,section,path):
 meta=re.search(r'^index ([0-9a-f]+)\.\.([0-9a-f]+)',section,re.M);assert meta,path
 pre=blob(raw);assert pre.startswith(meta[1]),(path,'preimage',pre,meta[1])
 lines=raw.decode().splitlines(keepends=True);output=[];cursor=0
 for hunk in re.split(r'(?=^@@ )',section,flags=re.M)[1:]:
  header,body=hunk.split('\n',1);m=re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@',header);assert m,(path,header)
  start=int(m[1])-1;oldcount=int(m[2] or '1');newcount=int(m[4] or '1');old=[];new=[]
  for line in body.splitlines(keepends=True):
   if line.startswith((' ','-')):old.append(line[1:])
   if line.startswith((' ','+')):new.append(line[1:])
  assert len(old)==oldcount and len(new)==newcount,(path,header,len(old),len(new))
  assert lines[start:start+oldcount]==old,(path,header,'context')
  assert cursor<=start,(path,header,'overlap')
  output.extend(lines[cursor:start]);output.extend(new);cursor=start+oldcount
 output.extend(lines[cursor:]);result=''.join(output).encode();post=blob(result)
 assert post.startswith(meta[2]),(path,'postimage',post,meta[2])
 return result,{'original_preimage_prefix':meta[1],'actual_preimage_blob':pre,'original_postimage_prefix':meta[2],'actual_postimage_blob':post,'preimage_and_postimage_match':True}

for filename in ['a4-01-resolver.complete.diff','a4-02-graph-and-resolver-tests.complete.diff']:
 source=A4/filename
 for path,section in sections(source.read_text()):
  assert path not in state,path
  raw=base_file(path);result,proof=apply(raw,section,path)
  state[path]=result;transition={'path':path,'source':str(source.relative_to(ROOT)),'from_stage':BASE,'to_stage':MAIN,**proof};transitions.append(transition)
  records[path]={'path':path,'original_base_blob':blob(raw),'source_evidence':[str(source.relative_to(ROOT))],'stage':MAIN,'authentication':[{'kind':'original_git_blob_prefix','value':proof['original_postimage_prefix'],'source':str(source.relative_to(ROOT))}]}
assert len(state)==12

full_sources={
 'crates/biome_module_graph/src/db/queries/project_exports.rs':A4/'a4-project_exports.complete.rs',
 'crates/biome_module_graph/src/db/queries/dependencies.rs':A4/'a4-dependencies.complete.rs',
 'crates/biome_module_graph/src/resolution.rs':A4/'a4-resolution.complete.rs',
 'crates/biome_module_graph/src/project_facts.rs':ROOT/'assembly/derived/a4/crates/biome_module_graph/src/project_facts.rs',
 'crates/biome_module_graph/tests/spec_tests/project.test.rs':ROOT/'assembly/derived/a4/crates/biome_module_graph/tests/spec_tests/project.test.rs',
}
original_preimages={
 'crates/biome_module_graph/src/project_facts.rs':'aaeec90614d',
 'crates/biome_module_graph/tests/spec_tests/project.test.rs':'fe70beeca51',
}
for path,src in full_sources.items():
 assert path not in state,path
 raw=src.read_bytes();state[path]=raw
 proof=[]
 if path in original_preimages:
  assert blob(raw).startswith(original_preimages[path]),path
  proof=[{'kind':'original_git_blob_prefix','value':original_preimages[path],'source':'Original A4 fix diff preimage; assembled fragments checked against that identity.'}]
 base_present=subprocess.run(['git','cat-file','-e',BASE+':'+path],cwd='/workspace/my-biome',stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0
 records[path]={'path':path,'original_base_blob':blob(base_file(path)) if base_present else None,'source_evidence':[str(src.relative_to(ROOT))],'stage':MAIN,'authentication':proof,'provenance':'Complete original A4 main source, unchanged by fix1 unless a later transition is recorded; no later B-stage source used.'}
assert len(state)==17

for filename in ['fix1-01-production.complete.diff','fix1-02-tests.complete.diff']:
 source=A4/filename
 for path,section in sections(source.read_text()):
  result,proof=apply(state[path],section,path);state[path]=result
  transition={'path':path,'source':str(source.relative_to(ROOT)),'from_stage':MAIN,'to_stage':FINAL,**proof};transitions.append(transition)
  records[path]['source_evidence'].append(str(source.relative_to(ROOT)));records[path]['stage']=FINAL
  records[path]['authentication']=[{'kind':'original_git_blob_prefix','value':proof['original_postimage_prefix'],'source':str(source.relative_to(ROOT))}]
assert len(transitions)==16

for path,data in state.items():
 dest=SOURCE/path;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
 records[path].update({'bytes':len(data),'lines':len(data.splitlines()),'sha256':hashlib.sha256(data).hexdigest(),'git_blob_sha1':blob(data),'valid_at_final_stage':FINAL,'stage_note':'Original main sources unaffected by the four-file fix remain valid at final A4.'})
assert {str(p.relative_to(SOURCE)) for p in SOURCE.rglob('*') if p.is_file()}==set(state)

def test_names(path):return re.findall(r'#\[test\]\s*fn ([a-zA-Z0-9_]+)',state[path].decode())
graph_names=test_names('crates/biome_module_graph/tests/spec_tests/project.test.rs');resolver_names=[n for n in test_names('crates/biome_resolver/tests/spec_tests.rs') if 'resolution_' in n]
assert len(graph_names)==28,graph_names
assert len(resolver_names)==6,resolver_names
manifest={
 'status':'coherent-original-A4-final-source-overlay',
 'base':BASE,'original_A4_main':MAIN,'original_A4_final':FINAL,
 'scope':'17 Rust source/test paths only. The original changeset is excluded for parent scope assessment. No A3 service, later B-stage facts, or new behavior composed.',
 'prerequisites':[{'from_stage':'A2 final65dd354834c7d42936a14f9fcf99255c1ab7ce43','interfaces':['PackageJson.project_metadata.diagnostics','TsConfigJson.metadata_diagnostics'],'engine_prerequisite':False,'full_A3_service_prerequisite':False}],
 'gaps':[],
 'files':[records[p] for p in sorted(records)],
 'exact_patch_transitions':transitions,
 'verification_status':'Source hashes and exact original patch transitions verified; no builds/tests executed.',
 'focused_tests':[
  {'command':'cargo test -p biome_module_graph --test spec_tests project_','expected_count':28,'names':graph_names},
  {'command':'cargo test -p biome_resolver --test spec_tests resolution_','expected_count':6,'names':resolver_names}],
 'additional_verification':['source /workspace/.biome-cloud/activate.sh','cargo test -p biome_module_graph -p biome_resolver','just f','just l','Fresh repository code review before declaring ready.'],
 'limitations':['Three unchanged full-file source snapshots lack an independently retained historical whole-file hash; their provenance is recorded, and current checksums are integrity checks only.','Historical successful tests are not proof this reconstructed continuation builds or passes today.','No original Git commit object was reconstructed.'],
}
(OUT/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
(OUT/'SOURCE-SHA256SUMS').write_text(''.join(f"{records[p]['sha256']}  source/{p}\n" for p in sorted(records)))
(OUT/'README.md').write_text('''# Coherent A4 final source overlay

Copy only source/ into the continuation after A2 passes. The source is original A4 final3c28dae on base6359, with all 12 original existing-file diffs and all 4 fix transitions authenticated by their original pre/post Git blob prefixes. The two previously fragmented files were already independently reconstructed and authenticated as the fix preimages. Three other full sources are literal retained snapshots without an independently recorded original whole-file hash; their checksums establish copy integrity.

The overlay includes exactly17Rust source/test paths, no laterB-stage data or A3 service. It requires A2 package diagnostic metadata fields and existing resolver/graph dependencies, with no biome_project_analysis crate dependency. The original pre-trim changeset is intentionally excluded; assess the actual user-facing scope before choosing its release entry.

Fresh tests should execute28project graph tests and6resolver resolution tests, then the complete affected crates. Format/lint and fresh review remain necessary. No builds/tests or old command execution occurred while materializing this source. Files have fresh mtimes, and no product checkout, index, or branch was modified.
''')
subprocess.run(['sha256sum','--check','--quiet','SOURCE-SHA256SUMS'],cwd=OUT,check=True)
print(json.dumps({'files':len(records),'bytes':sum(v['bytes'] for v in records.values()),'authenticated_final_files':sum(bool(v['authentication']) for v in records.values()),'verified_patch_transitions':len(transitions),'graph_tests':len(graph_names),'resolver_tests':len(resolver_names),'manifest_sha256':hashlib.sha256((OUT/'manifest.json').read_bytes()).hexdigest()},indent=2))
