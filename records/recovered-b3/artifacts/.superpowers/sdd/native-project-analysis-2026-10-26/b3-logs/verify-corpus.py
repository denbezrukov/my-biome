import json, hashlib
from pathlib import Path
root=Path('/workspace/.biome-cloud/knip-design')
corpus=Path('crates/biome_service/src/project_analysis/fixtures/members-oracle.json')
cases=json.loads(corpus.read_text())
assert len(cases)==54
keys={(case['batch'],case['id']) for case in cases}
assert len(keys)==54
for batch in ['b3-oracle-probes','b3-member-probes','b3-public-namespace-probes']:
 observations=json.loads((root/batch/'observations.json').read_text())
 assert {case_id for case_batch,case_id in keys if case_batch==batch}=={o['id'] for o in observations}
count=0
for case in cases:
 batch=root/case['batch']
 fixtures=json.loads((batch/('fixtures.json' if case['batch']=='b3-oracle-probes' else 'fixtures.before.json')).read_text())['cases']
 fixture=next(f for f in fixtures if f['id']==case['id'])
 observation=next(o for o in json.loads((batch/'observations.json').read_text()) if o['id']==case['id'])
 assert case['fixture_digest']==fixture['fixture_digest']==observation['fixture_digest']
 source=[{'path':f['path'],'source':f['utf8'],'sha256':f['sha256']} for f in fixture['snapshot'] if f['path']!='knip.json']
 assert case['files']==source
 for f in source:
  assert hashlib.sha256(f['source'].encode()).hexdigest()==f['sha256'];count+=1
 config={k:v for k,v in observation['config'].items() if k in ['entry','project','includeEntryExports','ignoreExportsUsedInFile','include','tags']}
 assert config==case['native_config']
 assert case['config_sha256']==hashlib.sha256(json.dumps(observation['config'],sort_keys=True,separators=(',',':')).encode()).hexdigest()
 assert case['oracle_items']==observation['issue_items']
 projected=[]
 for item in observation['issue_items']:
  kind=item['kind'];data=item['item']
  if kind=='duplicates': symbol='/'.join(sorted(d['name'] for d in data))
  elif kind in ['enumMembers','namespaceMembers']:symbol=data['namespace']+'.'+data['name']
  elif kind=='files':symbol=None
  else:symbol=data.get('name')
  projected.append([item['path'],symbol,kind])
 assert sorted(projected,key=json.dumps)==sorted(case['expected'],key=json.dumps)
print(f'PASS: 54 distinct retained observations; {count} exact per-case source files; native configuration projection and full-config digests; all raw issue items and expected path/category/symbol projections identical.')
print('Corpus SHA-256:',hashlib.sha256(corpus.read_bytes()).hexdigest())
