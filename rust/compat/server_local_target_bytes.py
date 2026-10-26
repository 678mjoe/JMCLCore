#!/usr/bin/env python3
"""Six owned no-state local mutation methods; no PIDs/processes/signals/CAS."""
from pathlib import Path
import os,json,subprocess,tempfile,hashlib
import argparse,sys,errno
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
args=parser.parse_args()
assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins={'zig':Path(args.zig).resolve(),'rust':Path(args.rust).resolve()}
if not sys.platform.startswith('linux'):
 for family in ['mods','datapacks']:
  for op in ['enable','disable','remove']:
   req=json.dumps({'protocol':1,'id':'t','method':f'server.{family}.{op}','params':{'directory':[256],'id':[]}})+'\n'
   observed=[]
   for exe in bins.values():
    p=subprocess.run([str(exe),'rpc'],input=req.encode(),capture_output=True,check=True,timeout=15);assert not p.stderr
    v=json.loads(p.stdout);assert v['error']['code']=='UNSUPPORTED_PLATFORM';observed.append(v)
   assert observed[0]==observed[1]
 print('PASS: six exact non-Linux platform gates; no fixtures or lifecycle access')
 raise SystemExit(0)
payload=b'owned opaque server content';digest=hashlib.sha1(payload).hexdigest();rows=[];violations=[]
def require(value,detail):
 if not value:violations.append(detail)
def snap(root):
 return {p.relative_to(root).as_posix():(p.lstat().st_dev,p.lstat().st_ino,p.lstat().st_mode,p.lstat().st_mtime_ns,p.lstat().st_nlink,p.read_bytes().hex() if p.is_file() else None) for p in [root,*root.rglob('*')]}
cases=['string','directory_array','id_array','both_arrays','raw_directory','unicode_directory_array','empty_directory','empty_id','invalid_directory_element','invalid_id_element','opaque_id','unsafe_id','missing_id','blocked_root','blocked_root_array','missing_version','missing_version_array','invalid_stored_id','id_mismatch','guard_directory','guard_directory_array','bad_selector','bad_selector_array','both_selectors','both_selectors_array','no_selector','no_selector_array','guard_bad_selector','guard_bad_selector_array','guard_both_selectors','guard_both_selectors_array','missing_file','missing_file_array','id_integral','id_fraction']
positive={'string','directory_array','id_array','both_arrays','raw_directory','unicode_directory_array','missing_file','missing_file_array','id_integral'}
with tempfile.TemporaryDirectory(prefix='owned-server-local-target-') as td:
 probe=Path(td)/os.fsdecode(b'probe-\xff')
 try:probe.mkdir();probe.rmdir()
 except OSError as e:
  if e.errno==errno.EILSEQ:cases.remove('raw_directory');print('CAPABILITY raw directory:',repr(e))
  else:raise
 for family,kind in [('mods','mod'),('datapacks','datapack')]:
  variants=cases+(['bad_properties','bad_properties_array','guard_bad_properties','guard_bad_properties_array','bad_properties_bad_selector','bad_properties_bad_selector_array'] if family=='datapacks' else [])
  for op in ['enable','disable','remove']:
   for case in variants:
    row={'method':f'server.{family}.{op}','case':case,'diagnostics':{}}
    for engine,exe in bins.items():
     t=Path(td)/str(len(rows))/engine;t.mkdir(parents=True)
     directory=t/(os.fsdecode(b'servers-\xff') if case=='raw_directory' else '服务器' if case=='unicode_directory_array' else 'servers');server=directory/'demo';server.mkdir(parents=True)
     (server/'.jmcl').mkdir();guard=server/'.jmcl/lifecycle.lock'
     if case.startswith('guard_'):guard.mkdir()
     else:guard.write_bytes(b'')
     manifest={'id':'demo','name':'Demo','version_id':'1.20.1'}
     if case.startswith('missing_version'):manifest.pop('version_id')
     if case=='invalid_stored_id':manifest['id']='../bad'
     if case=='id_mismatch':manifest['id']='other'
     (server/'server.json').write_text(json.dumps(manifest));(server/'server.properties').write_text('level-name='+('../bad' if 'bad_properties' in case else 'world')+'\n')
     home=server/'world' if family=='datapacks' else server;(home/family).mkdir(parents=True)
     (home/'.jmcl').mkdir(exist_ok=True)
     relative=family+('/a.jar' if family=='mods' else '/a.zip');enabled=op!='enable';file=home/(relative+('' if enabled else '.disabled'))
     file.write_bytes(payload);peer=t/'peer';os.link(file,peer)
     if case.startswith('missing_file'):file.unlink()
     peer.chmod(0o444)
     sibling=t/'sibling';sibling.write_bytes(b'preserve');(t/'blocked').write_bytes(b'not directory')
     entry={'kind':kind,'file':relative,'sha1':digest,'size':len(payload),'source':{'provider':'manual','project_id':None,'version_id':None,'slug':None},'enabled':enabled,'as_dependency':False}
     registry=home/'.jmcl/content.json';registry.write_text(json.dumps({'schema_version':1,'entries':[entry]}))
     root=t/'blocked' if case.startswith('blocked_root') else directory
     d=list(os.fsencode(root)) if case in ['directory_array','both_arrays','raw_directory','unicode_directory_array'] or case.endswith('_array') else str(root)
     ident=list(b'demo') if case in ['id_array','both_arrays','id_integral','id_fraction'] else 'demo'
     if case=='empty_directory':d=[]
     if case=='empty_id':ident=[]
     if case=='invalid_directory_element':d=[256]
     if case=='invalid_id_element':ident=[256]
     if case=='opaque_id':ident=[255]
     if case=='unsafe_id':ident=list(b'../bad')
     if case=='missing_id':ident=list(b'missing')
     params={'directory':d,'id':ident,'file':'../bad' if 'bad_selector' in case else relative}
     if 'both_selectors' in case:params['project']='project'
     if case.startswith('no_selector'):params.pop('file')
     pid=server/'.jmcl/server.pid';assert not pid.exists()
     before=snap(t);ps0=peer.stat();responses=[];states=[];row['diagnostics'][engine]=[]
     for step in range(2):
      req=json.dumps({'protocol':1,'id':'t','method':row['method'],'params':params})+'\n'
      if case in ['id_integral','id_fraction']:req=req.replace('"id": [100,','"id": ['+('100.0' if case=='id_integral' else '100.5')+',',1)
      pre=snap(t);o=subprocess.run([str(exe),'rpc'],input=req.encode(),capture_output=True,check=True,timeout=15);assert not o.stderr
      response=json.loads(o.stdout);responses.append(response);ps=peer.stat();assert not pid.exists()
      diag={'step':step,'raw_snapshot':snap(t),'peer_identity':[ps.st_dev,ps.st_ino,ps.st_mode,ps.st_mtime_ns,ps.st_nlink],'peer_bytes':peer.read_bytes().hex()};row['diagnostics'][engine].append(diag)
      require((ps.st_dev,ps.st_ino,ps.st_mode,ps.st_mtime_ns)==(ps0.st_dev,ps0.st_ino,ps0.st_mode,ps0.st_mtime_ns) and peer.read_bytes()==payload,(row,engine,step,'immutable peer'))
      require(sibling.read_bytes()==b'preserve',(row,engine,step,'sibling'))
      if case in positive:
       require(response['event']==('error' if op=='remove' and step==1 else 'result'),(row,engine,step,'positive/repeat source contract'))
       if op=='remove' and step==1:require(response.get('error',{}).get('code')=='CONTENT_NOT_FOUND',(row,engine,step,'repeat removal'))
      reg=json.loads(registry.read_bytes());normal=(home/relative).exists();disabled=(home/(relative+'.disabled')).exists()
      if response['event']=='error':require(snap(t)==pre,(row,engine,step,'failed call full snapshot'))
      else:
       require(response['result']['id']=='demo',(row,engine,step,'result id'))
       if op=='remove':require(reg['entries']==[] and not normal and not disabled and ps.st_nlink==1,(row,engine,step,'remove state'))
       else:
        require(reg['entries']==[dict(entry,enabled=op=='enable')],(row,engine,step,'toggle registry'))
        require(response['result']['changed']==(step==0),(row,engine,step,'idempotence'))
        missing=case.startswith('missing_file');require(normal==(op=='enable' and not missing) and disabled==(op=='disable' and not missing),(row,engine,step,'toggle paths'))
        if not missing:
         fs=(home/(relative+('' if op=='enable' else '.disabled'))).stat();require((fs.st_dev,fs.st_ino,fs.st_mode)==(ps.st_dev,ps.st_ino,ps.st_mode) and ps.st_nlink==2,(row,engine,step,'shared inode'))
      states.append({'registry_bytes':registry.read_bytes().hex(),'normal':normal,'disabled':disabled,'peer_links':ps.st_nlink,'tree':{k:{'mode':v[2],'links':v[4],'bytes':v[5]} for k,v in snap(t).items()}})
     row[engine]={'responses':responses,'states':states}
    rows.append(row)
diffs=[r for r in rows if r['zig']!=r['rust']]
print(len(rows),'paired no-state sequences;',len(rows)*2,'RPC pairs;',len(diffs),'mismatches')
for row in diffs:print('MISMATCH:',json.dumps(row,sort_keys=True))
if violations:print('CONTRACT_FAILURES:',json.dumps(violations,indent=2))
assert not violations
assert not diffs
