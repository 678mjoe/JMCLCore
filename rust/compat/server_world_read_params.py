#!/usr/bin/env python3
import gzip,hashlib,json,os,stat,struct,subprocess,tempfile
from pathlib import Path
import argparse,sys,errno
p=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:p.add_argument('--'+key,required=True)
a=p.parse_args()
assert subprocess.check_output(['git','-C',a.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
root=Path(tempfile.mkdtemp(prefix='owned-server-world-params-'));directory=root/'servers';server=directory/'demo';(server/'Alpha').mkdir(parents=True)
(server/'server.json').write_bytes(b'{"id":"demo","name":"Synthetic","version_id":"1.20"}');(server/'server.properties').write_bytes(b'level-name=Alpha\n')
def text(b):return struct.pack('>H',len(b))+b
def tag(t,n,v):return bytes([t])+text(n)+v
level=gzip.compress(tag(10,b'',tag(10,b'Data',tag(10,b'Version',tag(8,b'Name',text(b'1.20'))+b'\0')+b'\0')+b'\0'),mtime=0)
(server/'Alpha/level.dat').write_bytes(level);(root/'blocked').write_bytes(b'inert');(root/'empty-root').mkdir();(root/'peer').write_bytes(b'inert');os.link(root/'peer',root/'sibling');(root/'peer').chmod(0o444)
bins=[Path(a.zig).resolve(),Path(a.rust).resolve()]
print('binary SHA256:',{str(b):hashlib.sha256(b.read_bytes()).hexdigest() for b in bins})
def snap():
 return {str(p.relative_to(root)):(p.read_bytes().hex() if p.is_file() else None,s.st_mode,s.st_ino,s.st_nlink,s.st_mtime_ns) for p in [root,*root.rglob('*')] for s in [p.lstat()]}
def obj(fields):return '{'+','.join(json.dumps(k)+':'+v for k,v in fields.items())+'}'
rows=[]
for method in ['server.worlds.list','server.worlds.get']:
 base={'directory':json.dumps(str(directory)),'id':'"demo"','minecraft_version':'"1.20"'}
 if method.endswith('get'):base['world']='"Alpha"'
 cases=[('normal',obj(base),True),('default-directory',obj({k:v for k,v in base.items() if k!='directory'}),True)]
 for field in base:
  value=json.loads(base[field]);arr=json.dumps(list(value.encode()))
  cases += [(field+'-array',obj({**base,field:arr}),True),(field+'-null',obj({**base,field:'null'}),field=='minecraft_version'),(field+'-type',obj({**base,field:'true'}),False),(field+'-empty',obj({**base,field:'[]'}),field=='minecraft_version'),(field+'-overflow',obj({**base,field:'[256]'}),False)]
 for field,value,raw,quoted in [('id','demo','[100.0000000000000001,101,109,111]','["100.0000000000000001",101,109,111]'),('minecraft_version','1.20','[49.0000000000000001,46,50,48]','["49.0000000000000001",46,50,48]')]+([('world','Alpha','[65.0000000000000001,108,112,104,97]','["65.0000000000000001",108,112,104,97]')] if method.endswith('get') else []):
  cases += [(field+'-raw-round',obj({**base,field:raw}),True),(field+'-quoted-precise',obj({**base,field:quoted}),False)]
 cases += [('version-quoted-legacy',obj({**base,'minecraft_version':'["4_9",46,50,48]'}),True),('version-opaque',obj({**base,'minecraft_version':'[255]'}),True),('id-opaque',obj({**base,'id':'[255]'}),False),('id-array-root-file',obj({**base,'directory':json.dumps(str(root/'blocked')),'id':'[255]'}),False),('unknown-huge',obj(base)[:-1]+',"unused":1e400}',True),('irrelevant-huge',obj(base)[:-1]+',"new_name":1e400}',True),('unknown-surrogate',obj(base)[:-1]+',"unused":"\\ud800"}',False),('duplicate-id',obj(base)[:-1]+',"id":42}',False),('scalar','42',False),('null','null',False),('sequence',json.dumps([str(directory),'demo','Alpha','1.20']),False),('lexical',obj(base)[:-1]+',"unused":!}',False),('missing-id',obj({k:v for k,v in base.items() if k!='id'}),False)]
 if method.endswith('list'):cases += [('ignored-world',obj({**base,'world':'[256]'}),True)]
 else:cases += [('world-root-before-name',obj({**base,'directory':json.dumps(str(root/'empty-root')),'world':'[46,46]'}),False),('world-invalid-name',obj({**base,'world':'[46,46]'}),False)]
 for label,params,positive in cases:
  wire=('{"protocol":1,"id":"p","method":'+json.dumps(method)+',"params":'+params+'}\n').encode();before=snap();reports=[]
  for i,b in enumerate(bins):
   p=subprocess.run([str(b),'rpc'],input=wire,capture_output=True,timeout=15,cwd=root);assert p.returncode==0 and not p.stderr and snap()==before
   if i==0:
    value=json.loads(p.stdout)
    if sys.platform!='linux':assert value['error']['code']=='UNSUPPORTED_PLATFORM' or value['error']['code']=='INVALID_REQUEST'
    elif positive:
     info=value['result']['worlds'][0] if method.endswith('list') else value['result']['world'];assert info['name']=='Alpha' and info['version_name']=='1.20'
     assert info['version_relation']==('different' if label=='version-opaque' else 'unknown' if label in ('minecraft_version-null','minecraft_version-empty') else 'same')
    else:assert 'error' in value
   reports.append({'stdout_hex':p.stdout.hex(),'status':p.returncode})
  rows.append({'method':method,'label':label,'positive':positive,'equal':reports[0]==reports[1],'reports':reports})
# Physical opaque paths matter only on Linux: other platforms gate the RPC
# before decoding and must not be treated as physical path success evidence.
physical=[]
if sys.platform=='linux':
 raw_directory=root/os.fsdecode(b'raw-root-\xff')
 try:raw_directory.mkdir()
 except OSError as e:
  if e.errno!=errno.EILSEQ:raise
  print('physical raw directory unavailable: EILSEQ; exact rejection required')
  physical.append(('raw-directory-rejected',{'directory':list(os.fsencode(raw_directory)),'id':'demo','world':'Alpha'},False))
 else:
  d=raw_directory/'demo';(d/'Alpha').mkdir(parents=True)
  (d/'server.json').write_bytes((server/'server.json').read_bytes())
  (d/'server.properties').write_bytes(b'level-name=Alpha\n')
  (d/'Alpha/level.dat').write_bytes(level)
  physical.append(('raw-directory',{'directory':list(os.fsencode(raw_directory)),'id':'demo','world':'Alpha'},True))
 raw_world=os.fsdecode(b'raw-world-\xff')
 try:(server/raw_world).mkdir()
 except OSError as e:
  if e.errno!=errno.EILSEQ:raise
  print('physical raw world unavailable: EILSEQ; exact rejection required')
  physical.append(('raw-world-rejected',{'directory':str(directory),'id':'demo','world':list(os.fsencode(raw_world))},False))
 else:
  (server/raw_world/'level.dat').write_bytes(level)
  physical.append(('raw-world',{'directory':str(directory),'id':'demo','world':list(os.fsencode(raw_world))},True))
else:print('physical raw server lookup not exercised: RPC is Linux-only; platform gate remains strict')
physical += [('nul-directory',{'directory':list(os.fsencode(directory))+[0],'id':'demo','world':'Alpha'},False)]
for label,params,positive in physical:
 wire=(json.dumps({'protocol':1,'id':'native','method':'server.worlds.get','params':params})+'\n').encode();before=snap();reports=[]
 for i,b in enumerate(bins):
  p=subprocess.run([str(b),'rpc'],input=wire,capture_output=True,timeout=15,cwd=root)
  assert p.returncode==0 and not p.stderr and snap()==before
  value=json.loads(p.stdout)
  if i==0:
   if sys.platform!='linux':assert value['error']['code']=='UNSUPPORTED_PLATFORM'
   elif positive:
    info=value['result']['world'];assert info['version_name']=='1.20'
    actual=info['name'].encode() if isinstance(info['name'],str) else bytes(info['name'])
    expected=params['world'].encode() if isinstance(params['world'],str) else bytes(params['world'])
    assert actual==expected
   else:assert 'error' in value
  reports.append({'stdout_hex':p.stdout.hex(),'status':p.returncode})
 rows.append({'method':'server.worlds.get','label':label,'positive':positive,'equal':reports[0]==reports[1],'reports':reports})
assert not any(p.name=='server.pid' or '.jmcl-copy-' in p.name or '.jmcl-trash-' in p.name for p in root.rglob('*'))
(root/'evidence.json').write_text(json.dumps({'fixture':str(root),'rows':rows,'snapshot':snap()},indent=2))
print(len(rows),'cases;',sum(not r['equal'] for r in rows),'differences')
for r in rows:
 if not r['equal']:print(r['method'],r['label'],[bytes.fromhex(p['stdout_hex']).decode() for p in r['reports']])

assert all(r['equal'] for r in rows), 'strict parameter mismatch; see '+str(root)
print(f'PASS: {len(rows)} exact server-world read parameter cases; unchanged disk')
