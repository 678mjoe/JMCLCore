#!/usr/bin/env python3
import gzip,hashlib,json,os,stat,struct,subprocess,tempfile
from pathlib import Path
import argparse
p=argparse.ArgumentParser(description="Read-only worlds consumers of persisted manifest names")
for k in ['zig','rust','oracle-source']:p.add_argument('--'+k,required=True)
a=p.parse_args()
assert subprocess.check_output(['git','-C',a.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
root=Path(tempfile.mkdtemp(prefix='owned-world-manifest-name-'))
bins=[Path(a.zig).resolve(),Path(a.rust).resolve()]
print('binary SHA256:',{str(b):hashlib.sha256(b.read_bytes()).hexdigest() for b in bins})
def snap(root):
 return {str(p.relative_to(root)):(p.read_bytes().hex() if p.is_file() else None,s.st_mode,s.st_ino,s.st_nlink,s.st_mtime_ns) for p in [root,*root.rglob('*')] for s in [p.lstat()]}
def nbt_string(value):return struct.pack('>H',len(value))+value
def nbt_tag(tag,name,value):return bytes([tag])+nbt_string(name)+value
level=gzip.compress(nbt_tag(10,b'',nbt_tag(10,b'Data',nbt_tag(10,b'Version',nbt_tag(8,b'Name',nbt_string(b'1.20'))+b'\0')+b'\0')+b'\0'),mtime=0)
cases=[('string','"Demo"',{},True),('ascii','[68,101,109,111]',{},True),('unicode',json.dumps(list('世界'.encode())),{},True),('opaque','[255,0,65]',{},True),('empty','[]',{},False),('long',json.dumps([65]*257),{},False),('wrong-type','42',{},False),('null','null',{},False),('overflow','[256]',{},False),('fraction','[65.0000000000000001]',{},False),('quoted-grammar','["6_5"]',{},True),('invalid-id-empty','[]',{'id':'../bad'},False),('schema-empty','[]',{'schema_version':2},False),('source-empty','[]',{'source':'invalid'},False)]
cases += [('max-name',json.dumps([65]*256),{},True),('invalid-id-valid-name','[65]',{'id':'../bad'},False),('schema-valid-name','[65]',{'schema_version':2},False),('invalid-id-wrong-type','42',{'id':'../bad'},False),('schema-wrong-type','42',{'schema_version':2},False),('invalid-version','[65]',{'version_id':''},False),('name-before-version','[]',{'version_id':''},False)]
records=[]
for label,name,extra,success in cases:
 for kind in ['instance','server']:
  directory=root/(label+'-'+kind);folder=directory/'demo';world=folder/('.minecraft/saves/Alpha' if kind=='instance' else 'Alpha');world.mkdir(parents=True)
  (world/'level.dat').write_bytes(level)
  fields={'schema_version':1,'id':'demo','version_id':'1.20','source':'official',**extra}
  manifest='{'+','.join(json.dumps(k)+':'+json.dumps(v) for k,v in fields.items())+',"name":'+name+'}'
  (folder/(kind+'.json')).write_text(manifest)
  if kind=='server':(folder/'server.properties').write_bytes(b'level-name=Alpha\n')
  (directory/'peer').write_bytes(b'inert');os.link(directory/'peer',directory/'sibling');(directory/'peer').chmod(0o444)
  before=snap(root)
  for command in ['list','show']:
   calls=[([kind,'worlds',command,'demo']+(['Alpha'] if command=='show' else [])+['--'+('instances' if kind=='instance' else 'servers')+'-directory',str(directory)]+(['--json'] if j else []),None,j) for j in [False,True]]
   if kind=='server':
    params={'directory':str(directory),'id':'demo'}
    if command=='show':params['world']='Alpha'
    calls.append((['rpc'],(json.dumps({'protocol':1,'id':'name','method':'server.worlds.'+('list' if command=='list' else 'get'),'params':params})+'\n').encode(),True))
   for args,wire,asjson in calls:
    pair=[]
    for i,binary in enumerate(bins):
     p=subprocess.run([str(binary),*args],input=wire,capture_output=True,timeout=15);assert snap(root)==before
     if i==0:
      if wire and __import__('sys').platform!='linux':assert json.loads(p.stdout)['error']['code']=='UNSUPPORTED_PLATFORM'
      elif success:
       assert p.returncode==0 and not p.stderr
       if asjson:
        value=json.loads(p.stdout);value=value['result'] if wire else value
        info=value['worlds'][0] if command=='list' else value['world']
        assert info['name']=='Alpha' and info['version_name']=='1.20'
        assert info['version_relation']==('same' if kind=='instance' else 'unknown')
       else:assert b'Alpha' in p.stdout
      elif wire:assert 'error' in json.loads(p.stdout)
      else:assert p.returncode==1 and not p.stdout and p.stderr
     pair.append({'status':p.returncode,'stdout_hex':p.stdout.hex(),'stderr_hex':p.stderr.hex()})
    records.append({'case':label,'kind':kind,'command':command,'rpc':wire is not None,'json':asjson,'equal':pair[0]==pair[1],'pair':pair})
assert not any(p.name=='server.pid' or '.jmcl-copy-' in p.name or '.jmcl-trash-' in p.name for p in root.rglob('*'))
(root/'evidence.json').write_text(json.dumps({'fixture':str(root),'records':records,'snapshot':snap(root)},indent=2))
print(len(records),'comparisons;',sum(not r['equal'] for r in records),'differences')
for r in records:
 if not r['equal']:print(r['case'],r['kind'],r['command'],r['rpc'],r['json'],[(p['status'],bytes.fromhex(p['stderr_hex'])) for p in r['pair']])

assert all(r['equal'] for r in records), 'strict manifest consumer mismatch; see '+str(root)
print('PASS: 210 exact read-only world manifest-name commands; real version and complete disk preservation')
