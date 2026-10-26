#!/usr/bin/env python3
"""Server LIST-only owned no-state metadata fixtures. No lifecycle or network."""
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
 print('PASS: non-Linux gate before parameters; no filesystem fixtures')
 raise SystemExit(0)
def snap(root):
 out={}
 for p in [root,*root.rglob('*')]:
  s=p.lstat();out[str(p.relative_to(root))]=(p.read_bytes().hex() if p.is_file() else None,s.st_dev,s.st_ino,s.st_mode,s.st_mtime_ns,s.st_nlink)
 return out
rows=[]
with tempfile.TemporaryDirectory(prefix='owned-server-content-list-') as td:
 root=Path(td);(root/'sibling').write_bytes(b'owned unchanged');os.link(root/'sibling',root/'peer');(root/'blocked').write_bytes(b'not directory')
 roots={}
 for label,name in [('normal','servers'),('unicode','目录'),('raw',os.fsdecode(b'raw-\xff'))]:
  base=root/name;base.mkdir();roots[label]=base;home=base/'sample';(home/'World/datapacks').mkdir(parents=True);(home/'mods').mkdir();(home/'server.json').write_text(json.dumps({'id':'sample','name':'Name','version_id':'fake'}));(home/'server.properties').write_bytes(b'level-name=World\n')
 for label,m in [('invalid_id',{'id':'bad id','name':'Name','version_id':'fake'}),('mismatch',{'id':'other','name':'Name','version_id':'fake'}),('missing_version',{'id':'sample','name':'Name'})]:
  base=root/label;base.mkdir();roots[label]=base;home=base/'sample';home.mkdir();(home/'server.json').write_text(json.dumps(m));(home/'server.properties').write_bytes(b'level-name=World\n')
 for label,properties in [('no_world',b'level-name=World\n'),('bad_properties',b'level-name=../outside\n')]:
  base=root/label;base.mkdir();roots[label]=base;home=base/'sample';home.mkdir();(home/'server.json').write_text(json.dumps({'id':'sample','name':'Name','version_id':'fake'}));(home/'server.properties').write_bytes(properties)
 def check(method,label,d,id,extra=""):
  wire='{"protocol":1,"id":"t","method":'+json.dumps(method)+',"params":{"directory":'+d+',"id":'+id+extra+'}}\n';before=snap(root);row={'method':method,'label':label,'id':id}
  for name,exe in bins.items():
   o=subprocess.run([str(exe),'rpc'],input=wire.encode(),capture_output=True,check=True,timeout=15);assert not o.stderr;row[name]=json.loads(o.stdout);assert before==snap(root),(name,label,'disk changed')
  rows.append(row)
 for method in ['server.mods.list','server.datapacks.list']:
  for blocked in [False,True]:
   path=root/'blocked' if blocked else roots['normal']
   for dform,d in [('string',json.dumps(str(path))),('array',json.dumps(list(os.fsencode(path)))),('empty','[]'),('nul','[0]')]:
    for ilabel,id in [('string','"sample"'),('array','[115,97,109,112,108,101]'),('opaque','[255]'),('bad','[98,97,100,32,105,100]'),('empty','[]'),('missing','"missing"')]:check(method,f'{dform}/{ilabel}/blocked={blocked}',d,id)
   for id in ['[115.0,97,109,112,108,101]','[115.000000000000001,97,109,112,108,101]','["115.000000000000001",97,109,112,108,101]','["+1__15",97,109,112,108,101]','[115.5]','[256]','[-1]','[null]','[[115]]']:
    check(method,'numeric-id/blocked='+str(blocked),json.dumps(str(path)),id)
  for label in ['unicode','raw','invalid_id','mismatch','missing_version','no_world','bad_properties']:
   path=roots[label]
   for dform,d in [('array',json.dumps(list(os.fsencode(path))))]+([] if label=='raw' else [('string',json.dumps(str(path)))]):
    for id in ['"sample"','[115,97,109,112,108,101]']:
     check(method,label+'/'+dform,d,id)
  for extra in [',"unknown":1e400',',"command":{},"java_paths":true',',"id":[115,97,109,112,108,101]']:
   check(method,'ignored-or-duplicate',json.dumps(list(os.fsencode(roots['normal']))),'[115,97,109,112,108,101]',extra)
  for id in ['null','{}','[true]','[0]','[195,169]']:
   check(method,'additional-invalid-id',json.dumps(list(os.fsencode(roots['normal']))),id)
  for label,params in [('absent',None),('null','null'),('array','[]'),('object','{}'),('missing-id','{"directory":'+json.dumps(str(roots['normal']))+'}'),('missing-directory','{"id":"sample"}')]:
   before=snap(root);row={'method':method,'label':label}
   extra='' if params is None else ',"params":'+params
   wire='{"protocol":1,"id":"t","method":'+json.dumps(method)+extra+'}\n'
   for name,exe in bins.items():
    o=subprocess.run([str(exe),'rpc'],input=wire.encode(),capture_output=True,check=True,timeout=15);assert not o.stderr;row[name]=json.loads(o.stdout);assert snap(root)==before
   rows.append(row)
for row in rows:assert row['zig']==row['rust'],row
print('PASS:',len(rows),'exact server LIST target pairs; complete no-state fixture snapshots unchanged')
