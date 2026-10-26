#!/usr/bin/env python3
import gzip,hashlib,json,os,stat,struct,subprocess,tempfile
from pathlib import Path
import argparse
p=argparse.ArgumentParser(description="Instance CLI list/show only: synthetic metadata and unchanged disk")
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
cases=[('ascii',b'Synthetic',b'1.20',b'1.20'),('unicode','世界'.encode(),'版本'.encode(),'版本'.encode()),('controls',b'A\x00\nB',b'V\x00',b'V\x00'),('opaque-level',b'Name\xff',b'1.20',b'1.20'),('opaque-version-a',b'Name',b'V\xff',b'V\xff'),('opaque-version-b',b'Name',b'V\xff',b'V\xfe'),('opaque-version-c',b'Name',b'V\xff',b''),('both-opaque',b'\xed\xa0\x80',b'\xc0\x80',b'\xc0\x80'),('empty',b'',b'',b''),('missing',None,None,b'1.20')]
cases += [('wrong-tags',None,None,b'1.20'),('missing-data',None,None,b'1.20'),('wrong-data',None,None,b'1.20'),('wrong-version',b'Name',None,b'1.20'),('duplicates',b'First',b'FirstV',b'FirstV')]
results=[]
for label,name,ver,requestver in cases:
 directory=root/label; instance=directory/'demo'; world=instance/'.minecraft'/'saves'/'Alpha';world.mkdir(parents=True)
 (instance/'instance.json').write_text(json.dumps({'schema_version':1,'id':'demo','name':'Synthetic','version_id':'1.20','source':'official'}))
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
 for command in ['list','show']:
  for json_mode in [False,True]:
   args=['instance','worlds',command,'demo']+(['Alpha'] if command=='show' else [])+['--instances-directory',str(directory)]+(['--json'] if json_mode else [])
   records=[]
   relation='unknown' if not ver else 'same' if ver==b'1.20' else 'different'
   def expected(b):
    if b is None:return None
    try:return b.decode('utf-8')
    except UnicodeDecodeError:return list(b)
   for binary in bins:
    p=subprocess.run([str(binary),*args],capture_output=True,timeout=15)
    assert p.returncode==0 and p.stderr==b'' and snap(root)==before,(label,command,p.stderr)
    if json_mode:
     parsed=json.loads(p.stdout);obj=parsed['worlds'][0] if command=='list' else parsed['world']
     assert obj['name']=='Alpha' and obj['version_relation']==relation
     assert obj['level_name']==expected(name) and obj['version_name']==expected(ver)
     assert obj['size_bytes']==len((world/'level.dat').read_bytes()) and obj['has_icon']==False and obj['locked']==False
    elif command=='list':
     game=b'-' if label in ('missing-data','wrong-data') else b'creative'
     assert p.stdout==b'Alpha\t'+(name if name is not None else b'-')+b'\t'+(ver if ver is not None else b'-')+b'\t'+game+b'\t'+str(len((world/'level.dat').read_bytes())).encode()+b'\tidle\t'+relation.encode()+b'\n'
    elif label in ('missing-data','wrong-data'):
     assert b'level.dat: unavailable\n' in p.stdout and b'name: Alpha\n' in p.stdout
    else:
     assert p.stdout.startswith(b'name: Alpha\nlevel_name: '+(name if name is not None else b'-')+b'\nversion_name: '+(ver if ver is not None else b'-')+b'\n')
    records.append({'stdout_hex':p.stdout.hex(),'stderr_hex':p.stderr.hex(),'status':p.returncode})
   results.append({'case':label,'command':command,'json':json_mode,'equal':records[0]==records[1],'records':records})
assert not any('.jmcl-copy-' in p.name or '.jmcl-trash-' in p.name for p in root.rglob('*'))
(root/'evidence.json').write_text(json.dumps({'fixture':str(root),'cases':results,'final_snapshot':snap(root)},indent=2))
print('cases',len(results),'differences',sum(not x['equal'] for x in results))
assert all(x['equal'] for x in results), 'strict CLI mismatch; see '+str(root)
print('PASS: 60 exact instance CLI metadata commands; raw text/JSON and disk preservation')
