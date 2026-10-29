"""Materialize A1 source from retained literals; never execute archived commands."""
from pathlib import Path
import hashlib,json,re,subprocess
R=Path('/workspace/.biome-cloud/native-recovery'); A=R/'recovered-a1'; V=R/'recovered-a1-review'
OUT=Path(__file__).parent; SOURCE=OUT/'source'; SOURCE.mkdir(exist_ok=True)
BASE='6359a9b03670e85b50785d59d10eb087cf89fe3f'; HEAD='b292e878464503e93cba3f1d47a10be6e5e3dc41'
manifest=json.loads((A/'MANIFEST.json').read_text())
rows=[]
def blob(data):return hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
def store(path,data,evidence,transformation=None,proof=None):
 if isinstance(data,str):data=data.encode()
 target=SOURCE/path; target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
 if proof:
  for k,v in proof.items():
   if k=='original_git_blob_prefix':assert blob(data).startswith(v),(path,blob(data),v)
   if k=='original_sha256':assert hashlib.sha256(data).hexdigest()==v,path
 rows.append({'path':path,'source_evidence':evidence,'transformation':transformation,'bytes':len(data),'lines':len(data.splitlines()),'sha256':hashlib.sha256(data).hexdigest(),'git_blob_sha1':blob(data),'historical_proof':proof})
def block(diff,path):
 for part in re.split(r'(?=^diff --git )',diff,flags=re.M):
  if part.startswith(f'diff --git a/{path} b/{path}\n'):return part
 raise ValueError(path)
def apply(data,patch,path):
 data=data.decode() if isinstance(data,bytes) else data
 meta=re.search(r'^index ([0-9a-f]+)\.\.([0-9a-f]+)',patch,re.M); assert meta,path
 assert blob(data.encode()).startswith(meta[1]),(path,'preimage',blob(data.encode()),meta[1])
 lines=data.splitlines(keepends=True);output=[];cursor=0;changes=re.split(r'(?=^@@ )',patch,flags=re.M)[1:]
 for change in changes:
  header,body=change.split('\n',1);match=re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@',header);assert match
  start=int(match[1])-1;oldcount=int(match[2] or '1');newcount=int(match[4] or '1');old=[];new=[]
  for line in body.splitlines(keepends=True):
   if line.startswith((' ','-')):old.append(line[1:])
   if line.startswith((' ','+')):new.append(line[1:])
  assert len(old)==oldcount and len(new)==newcount,(path,header)
  assert lines[start:start+oldcount]==old,(path,header,'context')
  assert cursor<=start
  output.extend(lines[cursor:start]);output.extend(new);cursor=start+oldcount
 output.extend(lines[cursor:]);result=''.join(output).encode();assert blob(result).startswith(meta[2]),(path,'postimage',blob(result),meta[2])
 return result,{'original_git_blob_prefix':meta[2]}
for product in manifest['products']:
 path=product['original_path'];coverage=product['coverage'];ev=[str((A/e).relative_to(R)) for e in product['evidence']]
 if path in ['Cargo.toml','Cargo.lock']:
  before=subprocess.check_output(['git','show',BASE+':'+path],cwd='/workspace/my-biome')
  data,proof=apply(before,block((A/'diffs/initial-Cargo.diff.txt').read_text(),path),path)
  store(path,data,[BASE+':'+path,*ev],'Exact unified diff with matching original preimage and postimage Git blob prefixes',proof)
 elif path=='crates/biome_project_analysis/tests/contracts.rs':
  before=(A/'snapshots/contracts.rs.initial-commit-final.txt').read_bytes()
  data,proof=apply(before,block((A/'diffs/round1-ownership.diff.txt').read_text(),path),path)
  store(path,data,ev,'Exact final ownership fix diff; snapshot already contains earlier EvidenceId fix',proof)
 elif path=='crates/biome_project_analysis/src/result.rs':
  data=(A/'snapshots/result.rs.before-evidence-fix.txt').read_text()
  for before,after,count in [('pub related_evidence: Vec<u32>','pub related_evidence: Vec<EvidenceId>',2),('pub nodes: BTreeMap<u32, Evidence>','pub nodes: BTreeMap<EvidenceId, Evidence>',1),('pub related: Vec<u32>','pub related: Vec<EvidenceId>',1)]:
   assert data.count(before)==count;data=data.replace(before,after)
  needle='#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]\npub struct EvidenceGraph'
  addition='''/// A producer-qualified logical path and range, with a role suffix when needed.
/// Evidence identities are stable across snapshots and must not embed local numeric IDs.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct EvidenceId(pub String);

'''
  assert data.count(needle)==1;data=data.replace(needle,addition+needle)
  store(path,data,ev,'Four exact literal replacements from command140; archived command not executed')
 elif path in ['xtask/project_analysis/src/artifact.rs','xtask/project_analysis/src/conformance.rs']:
  store(path,(V/path).read_bytes(),[str((V/path).relative_to(R)),*ev],'Complete reviewed3d051362 source unchanged by final b292 ownership-only fix; artifact includes map_or_else; both test modules moved to end')
 elif path=='xtask/project_analysis/reference/tests.json':
  src=R/'recovered-reference-regeneration/b292e878-final'/path
  store(path,src.read_bytes(),[str(src.relative_to(R)),*ev],'Exact immutable-input regeneration independently matched original final Git blob',{'original_git_blob_prefix':'c1cc14c60b6'})
 elif coverage.startswith('full_final_source') or coverage=='full_final_source_text_from_printed_output':
  source=A/product['evidence'][0]
  proof={'original_sha256':'2f654153561e7d8b90fdbf448a7de09725f936533fc9c7b485190c036d089341'} if path.endswith('LICENSE.knip') else None
  store(path,source.read_bytes(),ev,None,proof)
 elif (V/path).is_file():
  proof={'original_sha256':'77ebf0ac8ef14a4f872b929665f6a1c458bee1345c8e81448c9fdd77101d3e74'} if path.endswith('package-lock.json') else None
  store(path,(V/path).read_bytes(),[str((V/path).relative_to(R)),*ev],'Complete initial reviewed source unchanged by final b292 ownership-only fix',proof)
 else:raise ValueError(('missing',path,coverage))
assert len(rows)==51
actual={str(p.relative_to(SOURCE)) for p in SOURCE.rglob('*') if p.is_file()};assert actual=={x['path'] for x in rows}
# Cross-check all source paths that have independent copies in the reviewer export.
crosschecks=[]
for record in rows:
 other=V/record['path']
 if other.is_file():
  same=other.read_bytes()==(SOURCE/record['path']).read_bytes()
  assert same,record['path']
  crosschecks.append(record['path'])
result={'status':'coherent-original-A1-final-source-overlay','base':BASE,'original_head':HEAD,'warning':'Original source restoration only. No builds/tests executed here; historical observations are not fresh validation. Per-file hash proofs distinguish independently authenticated bytes from literal transcriptions.','paths':len(rows),'gaps':[],'independent_reviewer_copy_matches':crosschecks,'files':rows}
(OUT/'manifest.json').write_text(json.dumps(result,indent=2)+'\n')
(OUT/'SOURCE-SHA256SUMS').write_text(''.join(f"{x['sha256']}  source/{x['path']}\n" for x in rows))
(OUT/'README.md').write_text('''# Coherent A1 source overlay

This directory materializes all 51 product paths of original A1 final b292e878464503e93cba3f1d47a10be6e5e3dc41 on base6359a9b03670e85b50785d59d10eb087cf89fe3f. Copy only `source/` into an isolated checkout based on that original base. Do not copy the later mixed-stage recovery assembly.

The result.rs evidence identities are reconstructed from exact retained replacements. normalize.rs and the initial contracts snapshot already contain those earlier changes and must not receive them twice. contracts.rs then receives the final ownership fix. Full reviewed artifact/conformance source includes the documented clippy/test-order changes. Later B/C files are excluded.

The root Cargo files and final contracts/test inventory match retained original Git blob prefixes. The npm lock and license match retained original SHA-256. Reviewer duplicates match all corresponding materialized paths. Other source snapshots have provenance but no independent historical whole-file hash; `manifest.json` makes this distinction explicit. Three fixture-group digests are separately authenticated by the original review recovery.

No archived command executed, no product checkout/index/branch modification, no build/install/network/test run. The parent must format, test, inspect and review the coherent overlay before treating it as a working continuation. Historical observation and validation files are historical records only.
''')
print(json.dumps({'paths':len(rows),'bytes':sum(x['bytes'] for x in rows),'reviewer_duplicates_matching':len(crosschecks),'manifest_sha256':hashlib.sha256((OUT/'manifest.json').read_bytes()).hexdigest()},indent=2))
