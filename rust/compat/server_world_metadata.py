#!/usr/bin/env python3
import gzip,hashlib,json,os,stat,struct,subprocess,tempfile
from pathlib import Path
import argparse
p=argparse.ArgumentParser(description="Server read-only worlds: synthetic metadata and unchanged disk")
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
 directory=root/label; instance=directory/'demo'; world=instance/'Alpha';world.mkdir(parents=True)
 (instance/'server.json').write_text(json.dumps({'id':'demo','name':'Synthetic','version_id':'1.20'}))
 (instance/'server.properties').write_bytes(b'level-name=Alpha\n')
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
   args=['server','worlds',command,'demo']+(['Alpha'] if command=='show' else [])+['--servers-directory',str(directory)]+(['--json'] if json_mode else [])
   records=[]
   relation='unknown'
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
 # Only Linux exposes these RPC methods; the non-Linux gate is checked separately.
 for method in ['server.worlds.list','server.worlds.get']:
  params={'directory':str(directory),'id':'demo','minecraft_version':'1.20'}
  if method.endswith('get'):params['world']='Alpha'
  wire=(json.dumps({'protocol':1,'id':'meta','method':method,'params':params})+'\n').encode()
  records=[]
  for binary in bins:
   p=subprocess.run([str(binary),'rpc'],input=wire,capture_output=True,timeout=15)
   assert p.returncode==0 and p.stderr==b'' and snap(root)==before
   obj=json.loads(p.stdout)
   if __import__('sys').platform=='linux':
    info=obj['result']['worlds'][0] if method.endswith('list') else obj['result']['world']
    assert info['name']=='Alpha' and info['level_name']==expected(name) and info['version_name']==expected(ver)
    assert info['version_relation']==('unknown' if not ver else 'same' if ver==b'1.20' else 'different')
    if method.endswith('get'):assert obj['result']['icon_png_base64'] is None
   else:assert obj['error']['code']=='UNSUPPORTED_PLATFORM'
   records.append({'stdout_hex':p.stdout.hex(),'stderr_hex':p.stderr.hex(),'status':p.returncode})
  results.append({'case':label,'method':method,'equal':records[0]==records[1],'records':records})
# Fixed property-derived order and error precedence, with no process state.
for variant in ['ordered','icon-read-error','missing-properties','malformed-properties','skip-notdir','missing-world','invalid-world','malformed-manifest']:
 directory=root/('control-'+variant)
 if variant=='blocked-root':directory.write_bytes(b'not-directory')
 else:
  instance=directory/'demo';instance.mkdir(parents=True)
  (instance/'server.json').write_bytes(b'{' if variant=='malformed-manifest' else b'{"id":"demo","name":"Synthetic","version_id":"1.20"}')
  if variant!='missing-properties':(instance/'server.properties').write_bytes(b'level-name=Zulu\n' if variant!='malformed-properties' else b'\xff')
  selected='world' if variant in ('missing-properties','malformed-properties') else 'Zulu'
  for n in [selected,selected+'_nether',selected+'_the_end']:
   if variant=='skip-notdir' and n.endswith('_nether'):(instance/n).write_bytes(b'inert')
   elif variant!='missing-world':(instance/n).mkdir()
  (directory/'peer').write_bytes(b'inert');os.link(directory/'peer',directory/'sibling');(directory/'peer').chmod(0o444)
  if variant=='icon-read-error':(instance/selected/'icon.png').mkdir()
 requested='..' if variant=='invalid-world' else 'world' if variant in ('missing-properties','malformed-properties') else 'Zulu'
 before=snap(root)
 for command in ['list','show']:
  calls=[(['server','worlds',command,'demo']+([requested] if command=='show' else [])+['--servers-directory',str(directory)]+(['--json'] if j else []),None,j) for j in [False,True]]
  params={'directory':str(directory),'id':'demo'}
  if command=='show':params['world']=requested
  calls.append((['rpc'],(json.dumps({'protocol':1,'id':'control','method':'server.worlds.'+('get' if command=='show' else 'list'),'params':params})+'\n').encode(),True))
  for args,wire,is_json in calls:
   records=[]
   for binary in bins:
    p=subprocess.run([str(binary),*args],input=wire,capture_output=True,timeout=15)
    assert snap(root)==before
    records.append({'status':p.returncode,'stdout_hex':p.stdout.hex(),'stderr_hex':p.stderr.hex()})
    if variant in ('ordered','icon-read-error','missing-properties','malformed-properties','skip-notdir') and is_json:
     parsed=json.loads(p.stdout)
     if wire and __import__('sys').platform!='linux':assert parsed['error']['code']=='UNSUPPORTED_PLATFORM'
     else:
      data=parsed['result'] if wire else parsed
      if command=='list':
       expected_names=[selected,selected+'_nether',selected+'_the_end']
       if variant=='skip-notdir':expected_names.remove(selected+'_nether')
       assert [w['name'] for w in data['worlds']]==expected_names
      else:
       assert data['world']['name']==selected
       if variant=='icon-read-error':
        assert data['world']['has_icon']==True
        if wire:assert data['icon_png_base64'] is None
   results.append({'case':variant,'command':command,'rpc':wire is not None,'json':is_json,'equal':records[0]==records[1],'records':records})
assert not any('.jmcl-copy-' in p.name or '.jmcl-trash-' in p.name or p.name=='server.pid' for p in root.rglob('*'))
(root/'evidence.json').write_text(json.dumps({'fixture':str(root),'cases':results,'final_snapshot':snap(root)},indent=2))
print('cases',len(results),'differences',sum(not x['equal'] for x in results))
assert all(x['equal'] for x in results), 'strict CLI mismatch; see '+str(root)
print('PASS: 138 exact server read-only metadata/order/error commands; platform gates and disk preservation')
