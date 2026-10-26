#!/usr/bin/env python3
import gzip,hashlib,json,os,stat,struct,subprocess,tempfile
from pathlib import Path
import argparse
p=argparse.ArgumentParser(description="List/get only: synthetic NBT metadata bytes and unchanged disk")
for key in ('zig','rust','oracle-source'):p.add_argument('--'+key,required=True)
a=p.parse_args()
assert subprocess.check_output(['git','-C',a.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins=[Path(a.zig).resolve(),Path(a.rust).resolve()]
print('binary SHA256:',{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in bins})
def text(b): return struct.pack('>H',len(b))+b
def tag(t,n,v): return bytes([t])+text(n)+v
def snap(root):
 return {str(p.relative_to(root)):(p.read_bytes().hex() if p.is_file() else None,stat.S_IMODE(s.st_mode),s.st_ino,s.st_nlink,s.st_mtime_ns) for p in [root,*root.rglob('*')] for s in [p.lstat()]}
root=Path(tempfile.mkdtemp(prefix='owned-world-meta-'))
cases=[('ascii',b'Synthetic',b'1.20',b'1.20'),('unicode','世界'.encode(),'版本'.encode(),'版本'.encode()),('controls',b'A\x00\nB',b'V\x00',b'V\x00'),('opaque-level',b'Name\xff',b'1.20',b'1.20'),('opaque-version-same',b'Name',b'V\xff',b'V\xff'),('opaque-version-different',b'Name',b'V\xff',b'V\xfe'),('opaque-version-empty-input',b'Name',b'V\xff',b''),('both-opaque',b'\xed\xa0\x80',b'\xc0\x80',b'\xc0\x80'),('empty',b'',b'',b''),('missing',None,None,b'1.20')]
cases += [('wrong-tags',None,None,b'1.20'),('missing-data',None,None,b'1.20'),('wrong-data',None,None,b'1.20'),('wrong-version',b'Name',None,b'1.20'),('duplicates',b'First',b'FirstV',b'FirstV')]
results=[]
for label,name,ver,requestver in cases:
 directory=root/label; world=directory/'saves'/'Alpha';world.mkdir(parents=True)
 data=(tag(8,b'LevelName',text(name)) if name is not None else b'')+(tag(10,b'Version',tag(8,b'Name',text(ver))+tag(3,b'Id',struct.pack('>i',7))+b'\0') if ver is not None else b'')+tag(3,b'GameType',struct.pack('>i',1))
 if label=='wrong-tags':data=tag(3,b'LevelName',struct.pack('>i',42))+tag(10,b'Version',tag(3,b'Name',struct.pack('>i',42))+b'\0')+tag(3,b'GameType',struct.pack('>i',1))
 if label=='wrong-version':data+=tag(8,b'Version',text(b'not compound'))
 if label=='duplicates':data+=tag(8,b'LevelName',text(b'Second'))+tag(10,b'Version',tag(8,b'Name',text(b'SecondV'))+b'\0')
 payload=tag(10,b'',tag(10,b'Data',data+b'\0')+b'\0')
 if label=='missing-data':payload=tag(10,b'',b'\0')
 if label=='wrong-data':payload=tag(10,b'',tag(8,b'Data',text(b'not compound'))+b'\0')
 (world/'level.dat').write_bytes(gzip.compress(payload,mtime=0))
 (directory/'peer').write_bytes(b'inert');os.link(directory/'peer',directory/'sibling');(directory/'peer').chmod(0o444)
 before=snap(root)
 for method in ['worlds.list','worlds.get']:
  params={'directory':str(directory),'minecraft_version':list(requestver)}
  if method=='worlds.get':params['world']='Alpha'
  wire=(json.dumps({'protocol':1,'id':'meta','method':method,'params':params})+'\n').encode()
  records=[]
  for binary in bins:
   p=subprocess.run([str(binary),'rpc'],input=wire,capture_output=True,timeout=15)
   assert p.returncode==0 and p.stderr==b'' and snap(root)==before
   parsed=json.loads(p.stdout);assert 'result' in parsed
   obj=parsed['result']['worlds'][0] if method=='worlds.list' else parsed['result']['world']
   assert obj['name']=='Alpha' and obj['game_mode']==(None if label in ('missing-data','wrong-data') else 'creative')
   assert obj['size_bytes']==len((world/'level.dat').read_bytes()) and obj['has_icon']==False and obj['locked']==False
   records.append({'stdout_hex':p.stdout.hex(),'object':obj})
  def expected(b):
   if b is None:return None
   try:return b.decode('utf-8')
   except UnicodeDecodeError:return list(b)
  assert records[0]['object']['level_name']==expected(name)
  assert records[0]['object']['version_name']==expected(ver)
  relation='unknown' if not ver or not requestver else 'same' if ver==requestver else 'different'
  assert records[0]['object']['version_relation']==relation
  results.append({'case':label,'method':method,'equal':records[0]['stdout_hex']==records[1]['stdout_hex'],'records':records})
assert not any('.jmcl-copy-' in p.name or '.jmcl-trash-' in p.name for p in root.rglob('*'))
(root/'evidence.json').write_text(json.dumps({'fixture':str(root),'cases':results,'final_snapshot':snap(root)},indent=2))
print('cases',len(results),'differences',sum(not x['equal'] for x in results))
for r in results:
 if not r['equal']:print(r['case'],r['method'],[(a['object']['level_name'],a['object']['version_name'],a['object']['version_relation']) for a in r['records']])

assert all(x['equal'] for x in results), 'strict metadata wire mismatch; see retained fixture '+str(root)
print('PASS: 30 exact metadata list/get cases; full disk identity and auxiliary fields preserved')
