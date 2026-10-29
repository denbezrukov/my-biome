"""Apply three exact A3 filesystem diffs against the recorded immutable base."""
from pathlib import Path
import hashlib,json,re,subprocess,shutil
ROOT=Path('/workspace/.biome-cloud/native-recovery'); OUT=Path(__file__).parent; SOURCE=OUT/'source'
BASE='6359a9b03670e85b50785d59d10eb087cf89fe3f'
PATCH=ROOT/'recovered-a3/evidence/fs-and-contracts.diff.txt'
def blob(data):return hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
records=[]
for section in re.split(r'(?=^diff --git )',PATCH.read_text(),flags=re.M):
 if not section.startswith('diff --git a/crates/biome_fs/'):continue
 path=section.splitlines()[0].split(' b/')[1]
 meta=re.search(r'^index ([0-9a-f]+)\.\.([0-9a-f]+)',section,re.M)
 raw=subprocess.check_output(['git','show',BASE+':'+path],cwd='/workspace/my-biome')
 assert blob(raw).startswith(meta[1]),path
 old=raw.decode().splitlines(keepends=True);new=[];cursor=0
 for part in re.split(r'(?=^@@ )',section,flags=re.M)[1:]:
  head,body=part.split('\n',1);m=re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@',head);assert m
  start=int(m[1])-1;old_count=int(m[2] or '1');new_count=int(m[4] or '1')
  before=[];after=[]
  for line in body.splitlines(keepends=True):
   if line.startswith((' ','-')):before.append(line[1:])
   if line.startswith((' ','+')):after.append(line[1:])
  assert len(before)==old_count and len(after)==new_count
  assert old[start:start+old_count]==before,(path,head)
  new.extend(old[cursor:start]);new.extend(after);cursor=start+old_count
 new.extend(old[cursor:]);data=''.join(new).encode()
 assert blob(data).startswith(meta[2]),path
 dest=SOURCE/path;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
 records.append({'path':path,'source':str(PATCH.relative_to(ROOT)),'base':BASE,'original_preimage_blob_prefix':meta[1],'original_postimage_blob_prefix':meta[2],'original_preimage_matches':True,'original_postimage_matches':True,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'git_blob_sha1':blob(data)})
assert len(records)==3
path='crates/biome_fs/tests/canonical_path.rs';src=ROOT/'recovered-a3/full-source/canonical_path.rs';dest=SOURCE/path;dest.parent.mkdir(parents=True,exist_ok=True)
shutil.copyfile(src,dest)
data=dest.read_bytes();assert hashlib.sha256(data).hexdigest()=='d886e5f48351f5c64955ba1708441b581e1839215244e3f1636876b3bec73416'
records.append({'path':path,'source':str(src.relative_to(ROOT)),'provenance':'complete final A3 source as printed in original tool result e18f05; unchanged in fix1','original_whole_file_hash_available':False,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'git_blob_sha1':blob(data)})
result={'status':'coherent-A3-filesystem-identity-subunit','original_base':BASE,'original_A3_initial':'52708197fbcac4abe8231e6c36bda61db6ff4ce2','original_A3_final':'af4b6125c3db4cb82953be7d2e4440c398b39add','scope':'Only filesystem identity trait/OS/memory and its complete tests. No engine contracts, discovery service, changeset, or later A4+ code.','files':records,'expected_verification':[{'command':'source /workspace/.biome-cloud/activate.sh && cargo test -p biome_fs --test canonical_path','expected':'On Linux, 2 tests pass with nonzero execution: memory case/missing errors and OS directory aliases/symlink cycles. Windows-specific case is cfg-gated.'},{'command':'source /workspace/.biome-cloud/activate.sh && cargo test -p biome_fs','expected':'Entire biome_fs suite passes; any platform/environment failures must be assessed explicitly.'},{'command':'source /workspace/.biome-cloud/activate.sh && just f && just l','expected':'Repository formatting/lint succeed before commit.'}],'verification_status':'Source reconstruction and checksum checks only; no builds/tests run.','gaps':[]}
(OUT/'manifest.json').write_text(json.dumps(result,indent=2)+'\n')
(OUT/'SOURCE-SHA256SUMS').write_text(''.join(f"{x['sha256']}  source/{x['path']}\n" for x in records))
(OUT/'README.md').write_text('''# A3 filesystem identity source subunit

Copy only source/ into the continuation worktree. The three production files are exact original A3 postimages from matching6359 base bytes, authenticated by original diff prefixes. The complete canonical_path test is a retained literal snapshot with a current SHA-256, not an independently recorded historical digest.

No engine contracts or other A3 discovery/inventory integration is included. These changes are an internal prerequisite and have no standalone user-facing changeset. No archived command was executed. New copied files have fresh mtimes. No product checkout/index/branch changed; tests/builds remain the parent’s verification responsibility.
''')
subprocess.run(['sha256sum','--check','--quiet','SOURCE-SHA256SUMS'],cwd=OUT,check=True)
print(json.dumps({'files':len(records),'bytes':sum(r['bytes'] for r in records),'manifest_sha256':hashlib.sha256((OUT/'manifest.json').read_bytes()).hexdigest()},indent=2))
