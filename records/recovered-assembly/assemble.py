"""Recovery-only source index and strict, hash-bound patch composition.

This script is authored during recovery. It never executes archived commands or
project code, and never changes the repository worktree, index, refs or objects.
"""
from pathlib import Path
import hashlib
import json
import re
import subprocess
from collections import defaultdict

ROOT = Path('/workspace/.biome-cloud/native-recovery')
OUT = ROOT / 'assembly'
REPO = Path('/workspace/my-biome')
BASE = '6359a9b03670e85b50785d59d10eb087cf89fe3f'
versions = defaultdict(dict)
known_changes = defaultdict(list)
errors = []
transitions = []

def sha(data): return hashlib.sha256(data).hexdigest()
def blob(data): return hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
def read_json(path): return json.loads((ROOT/path).read_text())
def rank_for(path):
    if 'recovered-d1-api-reconcile' in str(path):return 341
    m = re.search(r'recovered-([abc])(\d+)',str(path))
    return {'a':100,'b':200,'c':300}[m[1]]+int(m[2])*10 if m else 0

def add(path, source, stage, rank, proof=None, data=None, depth=0):
    assert not Path(path).is_absolute() and '..' not in Path(path).parts
    if data is None: data=(ROOT/source).read_bytes()
    bid=blob(data)
    rec={'path':path,'source':source,'stage':stage,'stage_order':rank,
         'bytes':len(data),'lines':len(data.splitlines()),'sha256':sha(data),
         'git_blob_sha1':bid,'historical_proof':proof,'patch_depth':depth}
    prior=versions[path].get(bid)
    if prior and prior['stage_order']>rank:return prior
    if prior and prior['stage_order']==rank and prior['patch_depth']>=depth:return prior
    versions[path][bid]=rec
    dest=OUT/'objects'/bid
    dest.parent.mkdir(parents=True,exist_ok=True)
    if dest.exists():assert dest.read_bytes()==data
    else:dest.write_bytes(data)
    return rec

def folder(directory,stage,rank,proof=None,expectations=None):
    root=ROOT/directory
    if not root.exists():return
    for p in sorted(root.rglob('*')):
        if not p.is_file():continue
        rel=str(p.relative_to(root))
        if not (rel.startswith(('crates/','packages/','xtask/','.changeset/')) or rel in ['Cargo.toml','Cargo.lock']):continue
        if any(x in rel for x in ['.partial','.fragment','.lines-','.transcription-']):continue
        b=p.read_bytes(); specific=proof
        if expectations and rel in expectations:
            exp=expectations[rel]
            if sha(b)!=exp['sha256']:
                errors.append({'source':str(p.relative_to(ROOT)),'reason':'historical_sha256_mismatch','actual':sha(b),'expected':exp['sha256']});continue
            specific={'kind':'original_sha256','sha256':exp['sha256']}
        add(rel,str(p.relative_to(ROOT)),stage,rank,specific,data=b)

# Full snapshots explicitly identified by each recovery agent.
for item in read_json('recovered-a1/MANIFEST.json')['products']:
    if item['coverage']=='full_final_source_snapshot':
        add(item['original_path'],'recovered-a1/'+item['evidence'][0],'b292e878',110)
add('crates/biome_project_analysis/tests/contracts.rs','recovered-a1/snapshots/contracts.rs.initial-commit-final.txt','3d051362',109,{'kind':'original_git_blob_prefix','value':'b416111c945'})
folder('assembly/derived/a1-literals','b292e878',110,{'kind':'literal_extraction_with_fixture_group_validation','manifest':'assembly/derived/a1-literals/provenance.json'})
folder('recovered-a1-review','3d051362 (A1 review; use exact final fix where changed)',110)
folder('recovered-reference-regeneration/b292e878-final','b292e878',110,{'kind':'original_git_blob_prefix_from_immutable_reference_regeneration','manifest':'recovered-reference-regeneration/manifest.json'})
folder('recovered-a2-review/source','4b06788c',120)
folder('recovered-a2-fix1-review/stage-65dd354834c7d42936a14f9fcf99255c1ab7ce43','65dd3548',121,{'kind':'agent_verified_original_blob_prefix','manifest':'recovered-a2-fix1-review/manifest.json'})
for item in read_json('recovered-a3/manifest.json')['artifacts']:
    if item['path'].startswith('full-source/'):
        add(item['original_target'],'recovered-a3/'+item['path'],'A3 historical: '+item['completeness'],129,{'kind':'original_blob_prefix','value':item['original_blob_prefix_if_available']} if item['original_blob_prefix_matches'] else None)
for item in read_json('recovered-a4/manifest.json')['entries']:
    if item['file'].endswith('.complete.rs'):
        add(item['original_path'],'recovered-a4/'+item['file'],item['state'],140)
folder('assembly/derived/a4','c86455d8',140,{'kind':'original_git_blob_prefix_from_overlapping_fragments','manifest':'assembly/derived/a4/provenance.json'})
folder('recovered-a5-review','8fd05449',151)
folder('recovered-b1/main','a62fb487',210)
folder('recovered-b1-fix1-review/final','df22208c',211)
folder('assembly/derived/b1','df22208c',211,{'kind':'original_sha256_from_overlapping_fragments','manifest':'assembly/derived/b1/provenance.json'})
folder('recovered-b2-fix1-review/990a42e751d276e8b403a0de8d099864dea54bf0','990a42e7',221,{'kind':'agent_verified_original_sha256','manifest':'recovered-b2-fix1-review/manifest.json'})
folder('recovered-b3/stages/43c453530573278cfcdae0b78c1195ba3a99262b','43c45353',231,{'kind':'agent_verified_original_sha256','manifest':'recovered-b3/RECOVERY-MANIFEST.json'})
for name,path in [('members.tests.rs','crates/biome_service/src/project_analysis/members.tests.rs'),('owned_facts.rs','crates/biome_service/src/project_analysis/owned_facts.rs'),('project_analysis.rs','crates/biome_service/src/project_analysis.rs'),('project_facts.rs','crates/biome_module_graph/src/project_facts.rs'),('project_exports.rs','crates/biome_module_graph/src/db/queries/project_exports.rs'),('exports.tests.rs','crates/biome_service/src/project_analysis/exports.tests.rs'),('members-oracle.json','crates/biome_service/src/project_analysis/fixtures/members-oracle.json'),('duplicate_binding.json','crates/biome_project_analysis/tests/fixtures/duplicate_binding.json'),('duplicate_spelling.json','crates/biome_project_analysis/tests/fixtures/duplicate_spelling.json')]:
    src=ROOT/'recovered-b3-review'/name
    if src.exists():
        checksum_file=ROOT/'recovered-b3-review/original-07d3c3e7.sha256'
        expected=re.search(r'([a-f0-9]{64})  '+re.escape(path)+r'$',checksum_file.read_text(),re.M)[1]
        if sha(src.read_bytes())==expected:add(path,str(src.relative_to(ROOT)),'07d3c3e7',230,{'kind':'original_sha256','sha256':expected})
        else:errors.append({'source':str(src.relative_to(ROOT)),'reason':'historical_sha256_mismatch_or_export_not_finished','expected':expected})
folder('assembly/derived/b3','43c45353',231,{'kind':'original_sha256_from_exact_base_plus_append','manifest':'assembly/derived/b3/provenance.json'})
folder('recovered-b4-fix1-review/final','185e9089',241,{'kind':'agent_verified_original_sha256','manifest':'recovered-b4-fix1-review/README.md'})
folder('recovered-b4/main','3d400313',240)
folder('recovered-b5-fix1-review','e758ebf7',251)
folder('recovered-b5-fix2-review/final-74f3d041','74f3d041',252,{'kind':'agent_reconstructed_from_exact_diff','manifest':'recovered-b5-fix2-review/reconstructed-sources.json'})
folder('recovered-b6-fix1-review/final-e2dcace/full','e2dcace',261,{'kind':'agent_verified_original_sha256','manifest':'recovered-b6-fix1-review/manifest.json'})
folder('assembly/derived/b6','e2dcace',261,{'kind':'original_sha256','sha256':'eab578c1e86e144da42f310fa857123fa1217a77345d21ad266c87c79927f9c1'})
folder('recovered-b7/20104d6','20104d6',271,{'kind':'see_per_file_historical_proof','manifest':'recovered-b7/README.md'})
folder('recovered-b7-fix1-review/20104d6aeff4c50f6fb5d308270f747b6bffd878','20104d6',271)
folder('recovered-c1/complete','6c48ad8',310)
folder('recovered-c2-fix1-review','91e5c212',321)
c3=read_json('recovered-c3/final-precommit-bindings.exact-recovered.json')['inputs']
folder('recovered-c3-fix1-review/stages/12bf926317b7032e2ddc46124bf277b8409bf5fd/full','12bf9263',331,expectations=c3)
if (ROOT/'recovered-c3-review/c3-main').exists():
    for p in (ROOT/'recovered-c3-review/c3-main').rglob('*'):
        if not p.is_file():continue
        path=str(p.relative_to(ROOT/'recovered-c3-review/c3-main'));b=p.read_bytes()
        if path in c3 and sha(b)==c3[path]['sha256']:
            add(path,str(p.relative_to(ROOT)),'12bf9263 (main-stage bytes match final binding)',331,{'kind':'original_sha256','sha256':c3[path]['sha256']},data=b)
        else:add(path,str(p.relative_to(ROOT)),'540d2e1',330,data=b)
c4=read_json('recovered-c4/original-final-commit-binding.transcribed.json')['committed_files']
# C4 fix changed these files; the historical d574 hash map cannot authenticate
# their e645 versions. Preserve all other usable d574 bindings.
c4_fix_paths=['crates/biome_console/src/lib.rs','crates/biome_cli/src/main.rs','crates/biome_cli/src/runner/project_analysis.rs','crates/biome_cli/src/runner/impls/finalizers/default.rs','crates/biome_cli/tests/project_analysis_exit.rs']
e645_checks={k:v for k,v in c4.items() if k not in c4_fix_paths}
folder('recovered-c5/base-e645266','e645266',341,expectations=e645_checks)
folder('recovered-c5-api-prep','e645266',341,expectations=e645_checks)
folder('recovered-c4-fix1-review','e645266',341,{'kind':'agent_verified_original_git_blob_prefix','manifest':'recovered-c4-fix1-review/PROVENANCE.md'})
for p in (ROOT/'recovered-c4-review').rglob('*lint_help.snap*'):
    if not p.is_file():continue
    target='crates/biome_cli/tests/snapshots/main_cases_help/lint_help.snap'
    if sha(p.read_bytes())==c4[target]['sha256']:
        add(target,str(p.relative_to(ROOT)),'d574a1a',340,{'kind':'original_sha256','sha256':c4[target]['sha256']})
# Recover unchanged baseline files only where original later manifests prove
# their whole-file SHA-256; an available base file alone is not final-source proof.
for binding,stage,rank in [(c3,'12bf9263',331),(e645_checks,'e645266',341)]:
    for path,expected in binding.items():
        cmd=subprocess.run(['git','show',BASE+':'+path],cwd=REPO,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
        if cmd.returncode==0 and sha(cmd.stdout)==expected['sha256']:
            add(path,'git:'+BASE+':'+path,stage,rank,{'kind':'original_sha256','sha256':expected['sha256']},data=cmd.stdout)

# Parse only literal unified diffs. Match exact preimage and postimage Git blob
# prefixes, then exact hunk offsets/context. No fuzzy matching or command replay.
patches=[]
for group in sorted(ROOT.glob('recovered-*')):
    if group.name in ['recovered-a2','recovered-b5','recovered-c2','recovered-c3']:continue
    for p in sorted(group.rglob('*')):
        if not p.is_file() or p.stat().st_size>2_000_000:continue
        if not any(x in p.name for x in ['diff','patch','package']):continue
        raw=p.read_bytes()
        if b'diff --git ' not in raw:continue
        for block in re.split(br'(?=^diff --git )',raw,flags=re.M):
            h=re.match(br'diff --git a/(\S+) b/(\S+)\n',block)
            idx=re.search(br'^index ([a-f0-9]+)\.\.([a-f0-9]+)',block,re.M)
            if not h or not idx or h[1]!=h[2]:continue
            path=h[1].decode();before,after=idx[1].decode(),idx[2].decode()
            if set(after)=={'0'}:continue
            lines=block.splitlines(keepends=True);hunks=[];i=0;valid=True
            while i<len(lines):
                hm=re.match(br'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@',lines[i])
                if not hm:i+=1;continue
                a,n,b,m=map(int,[hm[1],hm[2] or b'1',hm[3],hm[4] or b'1']);i+=1;old=[];new=[]
                while i<len(lines) and (len(old)<n or len(new)<m):
                    line=lines[i];i+=1
                    if line[:1] not in [b' ',b'-',b'+']:valid=False;break
                    if line[:1] in [b' ',b'-']:old.append(line[1:])
                    if line[:1] in [b' ',b'+']:new.append(line[1:])
                if len(old)!=n or len(new)!=m:valid=False;break
                if i<len(lines) and lines[i].startswith(b'\\ No newline'):valid=False;break
                hunks.append((a,n,old,new))
            if valid and hunks:
                q={'path':path,'before':before,'after':after,'source':str(p.relative_to(ROOT)),'stage_order':rank_for(p),'hunks':hunks}
                patches.append(q)
                known_changes[path].append({'stage_order':rank_for(p),'source':q['source'],'blob_after':after})

# Selected original manifests identify final required versions and later edits.
for path, val in c4.items():known_changes[path].append({'stage_order':340,'source':'recovered-c4/original-final-commit-binding.transcribed.json','sha256':val['sha256']})
for path, val in c3.items():known_changes[path].append({'stage_order':331,'source':'recovered-c3/final-precommit-bindings.exact-recovered.json','sha256':val['sha256']})
for path in (ROOT/'recovered-c1/evidence/original-changed-paths.txt').read_text().splitlines():known_changes[path].append({'stage_order':310,'source':'recovered-c1/evidence/original-changed-paths.txt'})
for item in read_json('recovered-a1/MANIFEST.json')['products']:
    known_changes[item['original_path']].append({'stage_order':110,'source':'recovered-a1/MANIFEST.json','coverage':item['coverage']})
for path in read_json('recovered-a2-review/manifest.json')['covered_original_paths']:
    known_changes[path].append({'stage_order':120,'source':'recovered-a2-review/manifest.json'})
changed_in_b3_fix={line.split('  ',1)[1] for line in (ROOT/'recovered-b3/metadata/original-fix1-frozen-files.sha256').read_text().splitlines()}
for path in read_json('recovered-b3/RECOVERY-MANIFEST.json')['missing_full_text_paths']:
    if isinstance(path,str) and path.startswith(('crates/','packages/','xtask/','.changeset/')):
        known_changes[path].append({'stage_order':231 if path in changed_in_b3_fix else 230,'source':'recovered-b3/RECOVERY-MANIFEST.json'})
# These later fragments establish changed source if their bytes are absent from
# a selected earlier full file. Do not silently present B3 owned_facts as final.
for item in read_json('recovered-b5-fix1-review/manifest.json')['files']:
    if item.get('completeness')=='partial' and item.get('original_path')=='crates/biome_service/src/project_analysis/owned_facts.rs':
        fragment=(ROOT/'recovered-b5-fix1-review'/item['recovered_path']).read_bytes()
        if versions[item['original_path']] and all(fragment not in (OUT/'objects'/v['git_blob_sha1']).read_bytes() for v in versions[item['original_path']].values()):
            known_changes[item['original_path']].append({'stage_order':251,'source':'recovered-b5-fix1-review/'+item['recovered_path'],'evidence':'later exact fragment absent from earlier complete recovered sources'})

base_cache={}
attempted=set()
for _ in range(30):
    progress=False
    for p in patches:
        path=p['path']; candidates=list(versions[path].values())
        if not any(v['git_blob_sha1'].startswith(p['before']) for v in candidates):
            if set(p['before'])=={'0'}:base=b''
            else:
                if path not in base_cache:
                    cmd=subprocess.run(['git','show',BASE+':'+path],cwd=REPO,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
                    base_cache[path]=cmd.stdout if cmd.returncode==0 else None
                base=base_cache[path]
            if base is not None and (set(p['before'])=={'0'} or blob(base).startswith(p['before'])):
                candidates.append({'git_blob_sha1':blob(base),'stage_order':0,'patch_depth':0,'base_data':base,'source':'git:'+BASE+':'+path})
        for candidate in candidates:
            if not (candidate['git_blob_sha1'].startswith(p['before']) or (set(p['before'])=={'0'} and candidate.get('base_data')==b'')):continue
            key=(path,p['before'],p['after'],p['source'])
            if key in attempted:continue
            attempted.add(key)
            data=candidate.get('base_data')
            if data is None:data=(OUT/'objects'/candidate['git_blob_sha1']).read_bytes()
            lines=data.splitlines(keepends=True);out=[];pos=0;valid=True
            for a,n,old,new in p['hunks']:
                at=max(a-1,0)
                if at<pos or lines[at:at+n]!=old:valid=False;break
                out.extend(lines[pos:at]);out.extend(new);pos=at+n
            if not valid:
                errors.append({'source':p['source'],'path':path,'reason':'exact_hunk_context_mismatch','preimage':candidate['git_blob_sha1']});continue
            out.extend(lines[pos:]);result=b''.join(out);bid=blob(result)
            if not bid.startswith(p['after']):
                errors.append({'source':p['source'],'path':path,'reason':'historical_postimage_blob_mismatch','expected':p['after'],'actual':bid});continue
            if bid in versions[path]:continue
            proof={'kind':'original_git_blob_prefix_before_and_after','before':p['before'],'after':p['after'],'base_source':candidate['source'],'patch':p['source']}
            rec=add(path,p['source'],'exact diff postimage from '+p['source'],p['stage_order'],proof,data=result,depth=candidate['patch_depth']+1)
            transitions.append({'path':path,'before':candidate['git_blob_sha1'],'after':bid,'patch':p['source']})
            progress=True
    if not progress:break

# Authenticate complete recovered versions against retained original bindings
# or actual index prefixes, including files whose source-agent manifest only
# claimed literal transcription. Recovery checksums alone are not evidence.
original_digests=defaultdict(list)
for mapping,label in [(c3,'C3 historical binding'),(c4,'C4 historical binding')]:
    for path,item in mapping.items():original_digests[path].append((item['sha256'],label))
for fn,flag in [('recovered-b2-fix1-review/manifest.json','matches_original_record'),('recovered-b6-fix1-review/manifest.json','matches_original_frozen_manifest')]:
    for item in read_json(fn)['files']:
        if item.get(flag):original_digests[item['original_path']].append((item['sha256'],fn))
for fn in ['recovered-b3/metadata/original-main-frozen-files.sha256','recovered-b3/metadata/original-fix1-frozen-files.sha256','recovered-b4/original-main-frozen-files.sha256','recovered-b1-fix1-review/historical-source-hashes.txt']:
    for line in (ROOT/fn).read_text().splitlines():
        match=re.match(r'([a-f0-9]{64})\s+\*?(?:/workspace/my-biome/)?(\S+)',line)
        if match:original_digests[match[2]].append((match[1],fn))
for path,expected in [('crates/biome_project_analysis/src/scripts.rs','6b58c0975cd6b6b15b0a0e02ad84e96c61c733bdddda474ef9f3e3189de8de98'),('crates/biome_service/src/project_analysis/scripts.tests.rs','0b6cd5b9926ad2d03c933665cc032dfee0bea67e96e92590cda4375b1d449edb'),('crates/biome_module_graph/src/project_commands.rs','baf1f8298cf968307c1e290f11187eaaaeb4112912761ff648ba7c3820c08301'),('crates/biome_project_analysis/tests/scripts.rs','3fcbab0250bfe9936b0e411796c290f6193a4aedc4ed155d1f2f4ff016138604'),('crates/biome_service/src/project_analysis/inventory.rs','eab578c1e86e144da42f310fa857123fa1217a77345d21ad266c87c79927f9c1')]:
    original_digests[path].append((expected,'original B4/B6 frozen manifest'))
original_digests['xtask/project_analysis/oracle/package-lock.json'].append(('77ebf0ac8ef14a4f872b929665f6a1c458bee1345c8e81448c9fdd77101d3e74','A1 original oracle identity'))
for path,values in versions.items():
    for v in values.values():
        auth=[]
        for expected,label in original_digests[path]:
            if v['sha256']==expected:auth.append({'kind':'original_sha256','source':label,'value':expected})
        for p in patches:
            if p['path']!=path:continue
            for side in ['before','after']:
                if len(p[side])>=10 and v['git_blob_sha1'].startswith(p[side]):auth.append({'kind':'original_git_blob_prefix','source':p['source'],'side':side,'value':p[side]})
        proof=v['historical_proof']
        if proof and proof.get('kind')=='original_git_blob_prefix' and v['git_blob_sha1'].startswith(proof['value']):auth.append(proof)
        v['authentication']=auth
literal_provenance=OUT/'derived/a1-literals/provenance.json'
if literal_provenance.exists():
    for item in json.loads(literal_provenance.read_text())['files']:
        if not item.get('matches_original_observation'):continue
        for v in versions[item['path']].values():
            if v['sha256']==item['sha256']:
                v['fixture_group_authentication']={'kind':'original_fixture_group_digest','value':item['original_fixture_group_digest'],'source':'assembly/derived/a1-literals/provenance.json','note':'Original digest binds all sorted paths and bytes in the fixture group; current per-file SHA256 was not independently retained.'}

# Build latest-observed source view. Known later edits and same-stage competing
# versions stay explicit; this is not asserted to be a coherent buildable tree.
records=[]
for path in sorted(set(versions)|set(known_changes)):
    options=list(versions[path].values())
    if not options:
        records.append({'path':path,'status':'missing_complete_source','known_changes':known_changes[path]});continue
    descendants={t['before'] for t in transitions if t['path']==path}
    preferred=[v for v in options if v['git_blob_sha1'] not in descendants] or options
    selected=max(preferred,key=lambda v:(v['stage_order'],v['patch_depth'],bool(v['historical_proof'])))
    competitors=[v['git_blob_sha1'] for v in preferred if v['stage_order']==selected['stage_order'] and v['git_blob_sha1']!=selected['git_blob_sha1']]
    later=[c for c in known_changes[path] if c['stage_order']>selected['stage_order']]
    dest=OUT/'source'/path;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes((OUT/'objects'/selected['git_blob_sha1']).read_bytes())
    records.append({'path':path,'status':'later_known_changes_missing' if later else 'latest_available_complete_source','selected':selected,'known_later_changes':later,'same_stage_competing_blobs':competitors,'versions':sorted(options,key=lambda v:(v['stage_order'],v['patch_depth']))})
manifest={'status':'partial_original_source_assembly','original_base':BASE,'lost_final_head':'e645266862c389ffaae3c7e72aeac7b6e02b162d','warning':'Source view selects latest available complete historical files; versions differ and known later edits remain missing. Not a buildable or verified feature tree. Current checksums are not historical authentication unless authentication records an original digest/prefix match. No archived commands/project code executed. Coverage universe is observed recovered files/diffs/manifests, not a claim to include every originally changed path.','files':records,'exact_patch_transitions':transitions,'rejected_inputs':errors}
(OUT/'coverage-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
counts={s:sum(r['status']==s for r in records) for s in sorted({r['status'] for r in records})}
summary={'statuses':counts,'selected_complete_files':sum('selected' in r for r in records),'selected_authenticated_files':sum(bool(r.get('selected',{}).get('authentication')) for r in records),'selected_observed_at_e645':sum(r.get('selected',{}).get('stage')=='e645266' for r in records),'selected_e645_authenticated':sum(r.get('selected',{}).get('stage')=='e645266' and bool(r.get('selected',{}).get('authentication')) for r in records),'versions':sum(map(len,versions.values())),'exact_patch_transitions':len(transitions),'rejected_inputs':len(errors)}
summary['selected_fixture_group_authenticated_files']=sum(bool(r.get('selected',{}).get('fixture_group_authentication')) for r in records)
(OUT/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
(OUT/'README.md').write_text('# Original-source assembly (partial)\n\n'+manifest['warning']+'\n\n`source/` contains original relative paths; `objects/` preserves all selected source versions by Git blob identity. `coverage-manifest.json` records exact provenance, original stage, hashes, known gaps and competing versions. `assemble.py` is newly authored safe recovery tooling, not an archived task command.\n\n'+json.dumps(summary,indent=2)+'\n')
gaps=['# Current recovery gaps\n','This is an observed-path inventory, not a complete historical diff. No missing source has been invented. Exact-source gaps remain recoverable if another original transcript contains their bytes; they are not declared irretrievable.\n']
for state in ['later_known_changes_missing','missing_complete_source']:
    gaps.append('## '+state.replace('_',' ')+'\n')
    gaps.extend('- `'+r['path']+'`'+(' — latest complete source: '+r['selected']['stage'] if 'selected' in r else '') for r in records if r['status']==state)
    gaps.append('')
gaps.append('## Mechanical regeneration limits\n\nAll composed diffs require matching original preimage and postimage blob identities. Missing generated TS/schema bindings may be regenerated only after full matching Rust input contracts are restored; that would be new generation, and original identity still needs hash comparison. Missing fixture/lock/reference inventories require their retained literal inputs and immutable source versions. Algorithms, service plumbing and reporter bodies cannot be reconstructed as original bytes from an API summary.\n')
(OUT/'GAPS.md').write_text('\n'.join(gaps))
(OUT/'SOURCE-SHA256SUMS').write_text(''.join(r['selected']['sha256']+'  source/'+r['path']+'\n' for r in records if 'selected' in r))
print(json.dumps(summary,indent=2))
