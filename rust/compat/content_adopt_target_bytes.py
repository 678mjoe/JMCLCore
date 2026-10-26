#!/usr/bin/env python3
"""Explicit owned CAS roots; COMPAT-CAS-001 accepted source lock layout difference."""
from pathlib import Path
import os,json,subprocess,tempfile,hashlib
if os.name != "nt":import fcntl
import argparse,contextlib,errno
if os.name=='nt':fcntl=None
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
args=parser.parse_args()
assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins={'zig':Path(args.zig).resolve(),'rust':Path(args.rust).resolve()}
@contextlib.contextmanager
def owned_temp():
 if os.name=='nt':
  td=tempfile.mkdtemp(prefix='owned-adoption-target-')
  try:yield td
  finally:print('RETAINED owned readonly fixtures:',td)
 else:
  with tempfile.TemporaryDirectory(prefix='owned-adoption-target-') as td:yield td
payload=b'owned opaque content only';digest=hashlib.sha1(payload).hexdigest();rows=[];violations=[]
def require(value,detail):
 if not value:violations.append(detail)
def snap(root):
 return {p.relative_to(root).as_posix():(p.lstat().st_dev,p.lstat().st_ino,p.lstat().st_mode,p.lstat().st_mtime_ns,p.lstat().st_nlink,p.read_bytes().hex() if p.is_file() else None) for p in [root,*root.rglob('*')]}
def native(p):return str(p).encode('utf-8','surrogatepass') if os.name=='nt' else os.fsencode(p)
def normalized(root):
 result={}
 for p in root.rglob('*'):
  if p.is_file():
   data=p.read_bytes()
   if p.name=='content.json':data=json.dumps(json.loads(data),sort_keys=True).encode()
   result[p.relative_to(root).as_posix()]={'bytes':data.hex(),'readonly':p.stat().st_mode&0o222==0}
  else:result[p.relative_to(root).as_posix()]={'directory':True}
 return result
basecases=['unicode_directory_array','unicode_store_array','directory_integral','directory_fraction','store_quoted_integral','store_quoted_fraction','string','directory_array','store_array','both_arrays','raw_directory','raw_store','empty_directory','blocked_directory','blocked_store','both_blocked','invalid_directory','invalid_store','fresh_blocked_directory','fresh_empty_directory']
dpcases=['unicode_world_array','world_array','all_arrays','raw_world','empty_world','missing_world','bad_world','invalid_world','locked','locked_world_array','locked_bad_world','fresh_missing_world','fresh_locked','fresh_empty_world']
positives={'unicode_directory_array','unicode_store_array','unicode_world_array','directory_integral','store_quoted_integral','string','directory_array','store_array','both_arrays','raw_directory','raw_store','world_array','all_arrays','raw_world'}
with owned_temp() as td:
 raw_name='raw-\ud800' if os.name=='nt' else os.fsdecode(b'raw-\xff')
 raw_capable=True;probe=Path(td)/raw_name
 try:probe.mkdir();probe.rmdir()
 except OSError as e:
  if os.name!='nt' and e.errno==errno.EILSEQ or os.name=='nt' and getattr(e,'winerror',None)==123:
   raw_capable=False;print('CAPABILITY:',repr(e))
  else:raise
 for family,kind in [('mods','mod'),('resourcepacks','resourcepack'),('shaderpacks','shaderpack'),('datapacks','datapack')]:
  for case in basecases+(dpcases if family=='datapacks' else []):
   if 'raw_' in case and not raw_capable:continue
   if 'locked' in case and os.name=='nt':continue
   row={'method':family+'.adopt','case':case,'diagnostics':{}}
   engines=list(bins.items())
   has_twin=('array' in case or case in ['directory_integral','store_quoted_integral'] or case.endswith('empty_directory') or case.endswith('empty_world') or 'missing_world' in case or 'bad_world' in case) and not case.startswith('raw_') and not case.startswith('invalid_')
   if has_twin:engines.append(('rust_string',bins['rust']))
   for engine,exe in engines:
    t=Path(td)/str(len(rows))/engine;t.mkdir(parents=True)
    game=t/(raw_name if case=='raw_directory' else '游戏' if case=='unicode_directory_array' else 'game');game.mkdir()
    world=raw_name if case=='raw_world' else '世界' if case=='unicode_world_array' else 'World'
    home=game/'saves'/world if family=='datapacks' else game
    content=home/family;content.mkdir(parents=True)
    file=content/('a.jar' if family=='mods' else 'a.zip');file.write_bytes(payload);peer=t/'peer';os.link(file,peer);peer.chmod(0o444)
    sibling=t/'sibling';sibling.write_bytes(b'preserve')
    store=t/(raw_name if case=='raw_store' else '缓存' if case=='unicode_store_array' else 'store')
    fresh=case.startswith('fresh_')
    if not fresh:
     (store/'objects/sha1').mkdir(parents=True);(store/'locks/sha1').mkdir(parents=True)
    (t/'blocked').write_bytes(b'not directory')
    directory=t/'blocked' if 'blocked_directory' in case or case=='both_blocked' else game
    storeparam=t/'blocked' if case in ['blocked_store','both_blocked'] else store
    d=list(native(directory)) if case in ['directory_array','unicode_directory_array','directory_integral','directory_fraction','both_arrays','all_arrays','raw_directory'] else str(directory)
    st=list(native(storeparam)) if case in ['store_array','unicode_store_array','store_quoted_integral','store_quoted_fraction','both_arrays','all_arrays','raw_store'] else str(storeparam)
    w=list(native(world)) if case in ['world_array','unicode_world_array','all_arrays','raw_world','locked_world_array'] else world
    if case.endswith('empty_directory'):d=[]
    if case.endswith('empty_world'):w=[]
    if case=='invalid_directory':d=[256]
    if case=='invalid_store':st=[256]
    if case=='invalid_world':w=[256]
    if 'missing_world' in case:w=list(b'Missing')
    if 'bad_world' in case:w=list(b'../bad')
    if engine=='rust_string':
     d=bytes(d).decode('utf-8') if isinstance(d,list) else d
     st=bytes(st).decode('utf-8') if isinstance(st,list) else st
     w=bytes(w).decode('utf-8') if isinstance(w,list) else w
    params={'directory':d,'store_directory':st}
    if family=='datapacks':params['world']=w
    lock=None
    if 'locked' in case:
     lock=open(home/'session.lock','wb');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    before=snap(t);game_before=snap(game);peer_before=peer.stat();responses=[];states=[];row['diagnostics'][engine]=[]
    for step in range(2):
     req=json.dumps({'protocol':1,'id':'t','method':row['method'],'params':params})+'\n'
     if engine!='rust_string' and case in ['directory_integral','directory_fraction','store_quoted_integral','store_quoted_fraction']:
      key='directory' if case.startswith('directory_') else 'store_directory'
      first=params[key][0];value=str(first)+('.5' if case.endswith('fraction') else '.0')
      if case.startswith('store_'):value=json.dumps(value)
      req=req.replace(json.dumps(key)+': ['+str(first)+',',json.dumps(key)+': ['+value+',',1)
     pre=snap(t);o=subprocess.run([str(exe),'rpc'],input=req.encode(),capture_output=True,check=True,timeout=20);assert not o.stderr
     response=json.loads(o.stdout);responses.append(response)
     ps=peer.stat();object_path=store/f'objects/sha1/{digest[:2]}/{digest}'
     row['diagnostics'][engine].append({'step':step,'peer_before':[peer_before.st_dev,peer_before.st_ino,peer_before.st_mode,peer_before.st_mtime_ns,peer_before.st_nlink],'peer_after':[ps.st_dev,ps.st_ino,ps.st_mode,ps.st_mtime_ns,ps.st_nlink],'bytes':peer.read_bytes().hex()})
     require((ps.st_dev,ps.st_ino,ps.st_mode,ps.st_mtime_ns)==(peer_before.st_dev,peer_before.st_ino,peer_before.st_mode,peer_before.st_mtime_ns) and peer.read_bytes()==payload,(row,engine,step,'immutable peer'))
     if object_path.exists():
      obj=object_path.stat();src=file.stat()
      row['diagnostics'][engine][-1]['object']={'identity':[obj.st_dev,obj.st_ino],'mode':obj.st_mode,'links':obj.st_nlink,'bytes':object_path.read_bytes().hex()}
      row['diagnostics'][engine][-1]['source']={'identity':[src.st_dev,src.st_ino],'mode':src.st_mode,'links':src.st_nlink,'bytes':file.read_bytes().hex()}
     require(sibling.read_bytes()==b'preserve',(row,engine,step,'sibling'))
     if case in positives:require(response['event']=='result',(row,engine,step,'expected positive adoption'))
     if response['event']=='result':
      adopted=response['result']['adopted'];require(len(adopted)==(1 if step==0 else 0),(row,engine,step,'repeat adoption'))
      reg=json.loads((home/'.jmcl/content.json').read_text());require(len(reg['entries'])==1,(row,engine,step,'registry'))
      expected_entry={'kind':kind,'file':file.relative_to(home).as_posix(),'sha1':digest,'size':len(payload),'source':{'provider':'manual','project_id':None,'version_id':None,'slug':None},'enabled':True,'as_dependency':False}
      require(reg=={'schema_version':1,'entries':[expected_entry]},(row,engine,step,'exact registry'))
      require(object_path.is_file() and not object_path.is_symlink(),(row,engine,step,'regular CAS object'))
      if object_path.exists():
       os_=object_path.stat();fs=file.stat();require((fs.st_dev,fs.st_ino)==(os_.st_dev,os_.st_ino)==(ps.st_dev,ps.st_ino) and object_path.read_bytes()==payload and os_.st_mode&0o222==0 and ps.st_nlink==3,(row,engine,step,'CAS inode/hash/readonly'))
     elif not fresh:require(snap(t)==pre,(row,engine,step,'failed preinitialized snapshot'))
     else:
      expected=family=='datapacks' and case in ['fresh_blocked_directory','fresh_missing_world','fresh_locked']
      require(store.is_dir()==expected,(row,engine,step,'fresh store order'))
      require(snap(game)==game_before,(row,engine,step,'fresh-store failure preserves game subtree'))
      require(snap(t)['sibling']==before['sibling'],(row,engine,step,'fresh-store sibling identity'))
      require({p.name for p in t.iterdir()}=={game.name,'peer','sibling','blocked'}|({store.name} if store.exists() else set()),(row,engine,step,'only configured fresh store may appear'))
      require(not (home/'.jmcl').exists() and not object_path.exists(),(row,engine,step,'failure has no registry/object'))
      if store.exists():require({p.relative_to(store).as_posix() for p in store.rglob('*')}=={'objects','objects/sha1','locks','locks/sha1'},(row,engine,step,'only CAS scaffold'))
     leaf=store/f'locks/sha1/{digest}.lock'
     if response['event']=='result':
      expected_store={'objects':{'directory':True},'objects/sha1':{'directory':True},'locks':{'directory':True},'locks/sha1':{'directory':True},f'objects/sha1/{digest[:2]}':{'directory':True},f'objects/sha1/{digest[:2]}/{digest}':{'bytes':payload.hex(),'readonly':True}}
      if engine!='zig':expected_store[f'locks/sha1/{digest}.lock']={'bytes':'','readonly':False}
      require(normalized(store)==expected_store,(row,engine,step,'exact per-engine CAS layout; COMPAT-CAS-001 accepted lock difference'))
      if leaf.exists():
       require(leaf.is_file() and not leaf.is_symlink(),(row,engine,step,'regular retained digest lock'))
       leaf_id=[leaf.stat().st_dev,leaf.stat().st_ino]
       if step:require(leaf_id==row['diagnostics'][engine][0].get('lock_identity'),(row,engine,step,'stable retained lock identity'))
       row['diagnostics'][engine][-1]['lock_identity']=leaf_id
     if engine=='zig':require(not leaf.exists(),(row,engine,step,'source digest lock absent'))
     row['diagnostics'][engine][-1]['post_snapshot']=snap(t)
     registry_path=home/'.jmcl/content.json'
     row['diagnostics'][engine][-1]['registry_bytes_hex']=registry_path.read_bytes().hex() if registry_path.exists() else None
     states.append(normalized(t))
    row[engine]={'responses':responses,'states':states}
    if lock:lock.close()
   require([d['registry_bytes_hex'] for d in row['diagnostics']['zig']]==[d['registry_bytes_hex'] for d in row['diagnostics']['rust']],(row,'exact source/Rust raw registry bytes'))
   if has_twin:require(row['rust']==row['rust_string'],(row,'byte-vs-String full Rust effects'))
   rows.append(row)
wire_diffs=[r for r in rows if r['zig']['responses']!=r['rust']['responses']]
layout_diffs=[r for r in rows if r['zig']['states']!=r['rust']['states']]
print(len(rows),'paired adoption sequences;',sum(len(r[k]['responses']) for r in rows for k in ['zig','rust','rust_string'] if k in r),'total calls;',len(wire_diffs),'wire mismatches;',len(layout_diffs),'canonical layout differences (COMPAT-CAS-001 accepted, NOT whole-CAS parity; concurrency unverified); raw snapshots retained')
for r in layout_diffs:print('RAW_LAYOUT_DIFFERENCE:',json.dumps(r,sort_keys=True))
for r in wire_diffs:print('WIRE_MISMATCH:',json.dumps(r,sort_keys=True))
if violations:print('CONTRACT_FAILURES:',json.dumps(violations,indent=2))
assert not violations
assert not wire_diffs
