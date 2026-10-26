#!/usr/bin/env python3
"""Owned worlds.list/get metadata only; no staging names or archive operations."""
import argparse,gzip,hashlib,json,os,stat,struct,subprocess,tempfile
from pathlib import Path
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
args=parser.parse_args();zig=Path(args.zig).resolve();rust=Path(args.rust).resolve()
assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
print('binary SHA256:',{str(b):hashlib.sha256(b.read_bytes()).hexdigest() for b in [zig,rust]})
def text(s):b=s.encode();return struct.pack('>H',len(b))+b
def tag(t,n,v):return bytes([t])+text(n)+v
nbt=tag(10,'',tag(10,'Data',tag(8,'LevelName',text('Alpha'))+tag(3,'GameType',struct.pack('>i',1))+tag(10,'Version',tag(8,'Name',text('1.21.1'))+tag(3,'Id',struct.pack('>i',3955))+b'\0')+b'\0')+b'\0')
def snap(root):
 return {str(p.relative_to(root)):(p.read_bytes().hex() if p.is_file() else None,p.lstat().st_ino,p.lstat().st_mode,p.lstat().st_mtime_ns,p.lstat().st_nlink) for p in [root,*root.rglob('*')]}
def raw(obj):return '{'+','.join(json.dumps(k)+':'+v for k,v in obj.items())+'}'
rows=[]
# Windows readonly peer fixtures are retained; never chmod a shared inode.
with __import__('contextlib').nullcontext(tempfile.mkdtemp(prefix='owned-world-read-')) as td:
 root=Path(td);(root/'game/saves/Alpha').mkdir(parents=True);(root/'empty/saves').mkdir(parents=True);(root/'nosaves').mkdir();(root/'blocked').write_bytes(b'not-dir')
 (root/'game/saves/Alpha/level.dat').write_bytes(gzip.compress(nbt,mtime=0))
 (root/'game/saves/Alpha/icon.png').write_bytes(b'inert-icon')
 (root/'peer').write_bytes(b'inert-data');os.chmod(root/'peer',0o444);os.link(root/'peer',root/'sibling')
 cases=[]
 for m in ['worlds.list','worlds.get']:
  base={'directory':'"game"','minecraft_version':'"1.21.1"'}
  if m.endswith('get'):base['world']='"Alpha"'
  cases.append((m,'string-positive',dict(base)))
  for k in base:
   v=json.loads(base[k]);d=dict(base);d[k]=json.dumps(list(v.encode()));cases.append((m,k+'-array',d))
  d={k:json.dumps(list(json.loads(v).encode())) for k,v in base.items()};cases.append((m,'all-arrays',d))
  for token,label in [('[]','empty'),('[0]','nul'),('[256]','overflow'),('[1.5]','fraction'),('[true]','bool'),('[null]','null-element'),('[[65]]','nested'),('null','null')]:
   for field in ['directory','minecraft_version']+(['world'] if m.endswith('get') else []):
    d=dict(base);d[field]=token;cases.append((m,field+'-'+label,d))
  for key,value in [('unknown','1e400'),('unused','[1e400]')]:
   d=dict(base);d[key]=value;cases.append((m,key+'-huge',d))
  for directory in ['empty','nosaves','blocked']:
   for array in [False,True]:
    d=dict(base);d['directory']=json.dumps(list(directory.encode()) if array else directory);cases.append((m,directory+('-array' if array else '-string'),d))
  for field in base:
   d=dict(base);del d[field];cases.append((m,field+'-missing',d))
 for token,label in [('[65.0000000000000001,108,112,104,97]','raw-round'),('["65.0000000000000001",108,112,104,97]','quoted-exact'),('["6_5",108.0,112,104,97]','legacy-mixed'),('[46,46]','dotdot'),('[255]','opaque'),('[0]','nul')]:
  for directory in ['game','nosaves','blocked']:
   cases.append(('worlds.get',label+'-'+directory,{'directory':json.dumps(directory),'world':token}))
 # Methods still traverse their existing cleanup logic, but these fixtures
 # contain no matching names and the full snapshot must remain identical.
 assert not any('.jmcl-copy-' in str(p.name) or '.jmcl-trash-' in str(p.name) for p in root.rglob('*'))
 for field in ['world','minecraft_version']:
  for token,label in [('""','empty-string'),('["65.0000000000000001"]','quoted-precision'),('[65.0000000000000001]','raw-rounded'),('["+6__5"]','quoted-underscores'),('["0x1.04p6"]','quoted-hex'),('[-1]','negative'),('[-0]','negative-zero'),('[1e400]','huge'),('[1e-400]','underflow')]:
   cases.append(('worlds.get',field+'-boundary-'+label,{'directory':'"game"','world':'"Alpha"',field:token}))
 for size in [255,256]:
  cases.append(('worlds.get','world-length-'+str(size),{'directory':'"game"','world':json.dumps([65]*size)}))
 for token,label in [('"1.21.1"','same'),('"other"','different'),('[]','empty'),('[255]','opaque'),('null','null')]:
  cases.append(('worlds.get','version-'+label,{'directory':'"game"','world':'"Alpha"','minecraft_version':token}))
 for method in ['worlds.list','worlds.get']:
  for token,label in [('["game",null]','list-positional'),('["game","Alpha",null]','get-positional'),('[]','empty-array'),('null','null'),('true','bool'),('123','number'),('"game"','string'),('[["game"],null]','nested')]:
   cases.append((method,'shape-'+label,token))
 if os.name=='nt':
  for method in ['worlds.list','worlds.get']:
   for token,label in [('[255]','invalid-lead'),('[192,175]','overlong'),('[226,130]','truncated')]:
    value={'directory':token}
    if method=='worlds.get':value['world']='"Alpha"'
    cases.append((method,'windows-directory-'+label,value))
 for method in ['worlds.list','worlds.get']:
  world_field=',"world":"Alpha"' if method.endswith('get') else ''
  for body,label in [
   ('{"directory":"game","directory":[103,97,109,101]'+world_field+'}', 'duplicate-directory'),
   ('{"directory":"game"'+world_field+',"extra":"\\ud800"}', 'unknown-lone-surrogate'),
   ('{"directory":"game"'+world_field+',"extra":"\\ud83d\\ude00"}', 'unknown-paired-surrogate'),
   ('{"directory":"game"'+world_field+',"extra":"\\\\ud800"}', 'unknown-literal-backslash'),
   ('{"directory":"game"'+world_field+',"new_name":false}', 'irrelevant-mutation-field'),
  ]:
   cases.append((method,label,body))
 cases.append(('worlds.list','irrelevant-world',{'directory':'"game"','world':'false'}))
 before=snap(root)
 for m,label,params in cases:
  wire='{"protocol":1,"id":"read","method":'+json.dumps(m)+',"params":'+(params if isinstance(params,str) else raw(params))+'}\n'
  pair=[]
  for b in [zig,rust]:
   r=subprocess.run([str(b),'rpc'],cwd=root,input=wire.encode(),capture_output=True,timeout=10)
   assert snap(root)==before,(m,label,b,'disk mutation')
   assert r.returncode==0 and r.stderr==b'',(m,label,b,r)
   pair.append({'stdout_hex':r.stdout.hex(),'stderr_hex':r.stderr.hex(),'status':r.returncode,'value':json.loads(r.stdout)})
  rows.append({'method':m,'case':label,'params':params,'equal':pair[0]['stdout_hex']==pair[1]['stdout_hex'],'engines':pair})
  if label in ['string-positive','all-arrays','directory-array','world-array','minecraft_version-array']:
   v=pair[0]['value']['result'];world=v['world'] if m.endswith('get') else v['worlds'][0]
   assert world['name']=='Alpha' and world['level_name']=='Alpha' and world['game_mode']=='creative' and world['version_relation']=='same'
  if label.startswith('shape-'):
   expected='missing parameters' if label=='shape-null' else ('worlds list parameters are invalid' if m=='worlds.list' else 'world lookup parameters are invalid')
   assert pair[0]['value']['error']=={'code':'INVALID_PARAMS','message':expected},(m,label,pair[0])
  if label in ['unknown-huge','unused-huge','unknown-paired-surrogate','unknown-literal-backslash','irrelevant-mutation-field','irrelevant-world']:
   assert 'result' in pair[0]['value'],(m,label,pair[0])
  if label in ['duplicate-directory','unknown-lone-surrogate']:
   assert pair[0]['value']['error']['code']=='INVALID_REQUEST',(m,label,pair[0])
  if label.startswith('windows-directory-'):
   assert pair[0]['value']['error']['code']==('WORLD_LIST_FAILED' if m=='worlds.list' else 'WORLD_GET_FAILED'),(m,label,pair[0])
  if label in ['raw-round-game','legacy-mixed-game']:
   assert pair[0]['value']['result']['world']['name']=='Alpha'
  if label.startswith('version-'):
   relation={'version-same':'same','version-different':'different','version-empty':'unknown','version-opaque':'different','version-null':'unknown'}[label]
   assert pair[0]['value']['result']['world']['version_relation']==relation
  if m=='worlds.list' and label in ['empty-array','empty-string']:
   assert pair[0]['value']['result']['worlds']==[]
 # Separate targeted native lookup: never run a populated list containing an
 # opaque physical world, so enumeration remains outside this request slice.
 def native_bytes(text):
  return text.encode('utf-8','surrogatepass') if os.name=='nt' else os.fsencode(text)
 opaque='\ud800' if os.name=='nt' else os.fsdecode(b'\xff')
 raw_directory='RawRoot'+opaque
 raw_world='World'+opaque
 supported=True
 try:
  (root/raw_directory/'saves').mkdir(parents=True)
 except OSError as e:
  import errno
  if not (getattr(e,'winerror',None)==123 if os.name=='nt' else e.errno==errno.EILSEQ):raise
  supported=False
  print('physical raw-name success unavailable; comparing actual request rejection, not claiming success')
 if supported:
  (root/'game/saves'/raw_world).mkdir()
  (root/'game/saves'/raw_world/'level.dat').write_bytes(gzip.compress(nbt,mtime=0))
 native_cases=[('worlds.list','native-directory',{'directory':json.dumps(list(native_bytes(raw_directory)))}),
               ('worlds.get','native-world',{'directory':'"game"','world':json.dumps(list(native_bytes(raw_world))),'minecraft_version':'"1.21.1"'})]
 if os.name=='posix':
  (root/'GameAlias').symlink_to('game',target_is_directory=True)
  (root/'game/saves/Alias').symlink_to('Alpha',target_is_directory=True)
  native_cases += [('worlds.get','root-alias',{'directory':json.dumps(list(b'GameAlias')),'world':'"Alpha"'}),
                   ('worlds.get','world-alias',{'directory':'"game"','world':json.dumps(list(b'Alias'))})]
 for method,label,params in native_cases:
  before=snap(root);pair=[]
  wire='{"protocol":1,"id":"read","method":'+json.dumps(method)+',"params":'+raw(params)+'}\n'
  for binary in [zig,rust]:
   result=subprocess.run([str(binary),'rpc'],cwd=root,input=wire.encode(),capture_output=True,timeout=10)
   assert snap(root)==before,(label,binary,'disk mutation')
   assert result.returncode==0 and result.stderr==b''
   pair.append({'stdout_hex':result.stdout.hex(),'stderr_hex':result.stderr.hex(),'status':result.returncode,'value':json.loads(result.stdout)})
  if label=='native-directory' and supported:
   assert pair[0]['value']['result']['worlds']==[]
  elif label=='native-world' and supported:
   world=pair[0]['value']['result']['world']
   assert world['name']==list(native_bytes(raw_world)) and world['level_name']=='Alpha' and world['version_relation']=='same'
  elif label.endswith('alias'):
   world=pair[0]['value']['result']['world'];assert world['name']==('Alias' if label=='world-alias' else 'Alpha') and world['level_name']=='Alpha'
  else:
   assert 'error' in pair[0]['value'],(label,'unsupported raw names did not fail')
  rows.append({'method':method,'case':label,'params':params,'equal':pair[0]['stdout_hex']==pair[1]['stdout_hex'],'engines':pair})
for r in rows:
 assert r['equal'],r
print(f"PASS: {len(rows)} exact worlds.list/get parameter comparisons; full disk unchanged")
