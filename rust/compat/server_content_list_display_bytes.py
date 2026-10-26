#!/usr/bin/env python3
"""Server content LIST uses read-only display metadata; no lifecycle operations."""
from pathlib import Path
import json,tempfile,subprocess,os,argparse,sys
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
args=parser.parse_args()
assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins={'zig':Path(args.zig).resolve(),'rust':Path(args.rust).resolve()}
if sys.platform!='linux':
 for exe in bins.values():
  o=subprocess.run([str(exe),'rpc'],input=b'{"protocol":1,"id":"t","method":"server.mods.list","params":{}}\n',capture_output=True,check=True)
  assert not o.stderr and json.loads(o.stdout)['error']['code']=='UNSUPPORTED_PLATFORM'
 print('PASS: non-Linux gate, no metadata or process fixtures')
 raise SystemExit(0)
def snap(root):
 out={}
 for p in [root,*root.rglob('*')]:
  s=p.lstat();out[str(p.relative_to(root))]=(p.read_bytes().hex() if p.is_file() else None,s.st_dev,s.st_ino,s.st_mode,s.st_mtime_ns,s.st_nlink)
 return out
rows=[]
with tempfile.TemporaryDirectory(prefix='owned-server-list-name-') as td:
 for label,name in [('string','Name'),('ascii',[78,97,109,101]),('unicode',[195,169]),('opaque',[255]),('control',[0,10,255]),('empty',[]),('long',[65]*257),('overflow',[256]),('null',None)]:
  for state in ['normal','missing_version','invalid_id','id_mismatch','missing_version_bad_id','schema','source']:
   root=Path(td)/(label+'-'+state);home=root/'sample';(home/'World/datapacks').mkdir(parents=True);(home/'mods').mkdir();m={'id':'sample','name':name,'version_id':'fake'}
   if state in ['missing_version','missing_version_bad_id']:del m['version_id']
   if state in ['invalid_id','missing_version_bad_id']:m['id']='bad id'
   if state=='schema':m['schema_version']=2
   if state=='source':m['source']='bad'
   if state=='id_mismatch':m['id']='other'
   (home/'server.json').write_text(json.dumps(m));(home/'server.properties').write_bytes(b'level-name=World\n');(root/'sibling').write_bytes(b'owned preserved');os.link(root/'sibling',root/'peer')
   for method in ['server.mods.list','server.datapacks.list']:
    req=json.dumps({'protocol':1,'id':'t','method':method,'params':{'directory':str(root),'id':'sample'}})+'\n';before=snap(root);row={'name':label,'state':state,'method':method}
    for engine,exe in bins.items():
     o=subprocess.run([str(exe),'rpc'],input=req.encode(),capture_output=True,check=True,timeout=15);assert not o.stderr;row[engine]=json.loads(o.stdout);assert snap(root)==before
    rows.append(row)
for row in rows:assert row['zig']==row['rust'],row
print('PASS:',len(rows),'exact server LIST display-name/error-order pairs; full snapshots unchanged')
