from pathlib import Path
import os,json,subprocess,tempfile,hashlib,collections
import argparse,sys
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
parser.add_argument('--output')
args=parser.parse_args()
assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins={'zig':Path(args.zig).resolve(),'rust':Path(args.rust).resolve()}
if not sys.platform.startswith('linux'):
 for family in ['mods','datapacks']:
  for op in ['list','enable','disable','remove']:
   observed=[]
   for exe in bins.values():
    p=subprocess.run([str(exe),'server',family,op,'demo'],capture_output=True,timeout=15)
    observed.append((p.returncode,p.stdout,p.stderr))
   assert observed[0]==observed[1]
   assert observed[1]==(1,b'',b'Server management is currently supported only on Linux\n')
 print('PASS: eight non-Linux local CLI platform gates; no fixture access')
 raise SystemExit(0)
payload=b'owned opaque server content';digest=hashlib.sha1(payload).hexdigest();rows=[];violations=[]
def require(v,d):
 if not v:violations.append(d)
def snap(t):
 return {p.relative_to(t).as_posix():(p.lstat().st_dev,p.lstat().st_ino,p.lstat().st_mode,p.lstat().st_mtime_ns,p.lstat().st_nlink,p.read_bytes().hex() if p.is_file() else None) for p in [t,*t.rglob('*')]}
names=[('string','Name'),('ascii',[78,97,109,101]),('unicode',[195,169]),('opaque',[255]),('control',[0,10,255]),('empty',[]),('long',[65]*257),('overflow',[256]),('null',None)]
metadata=['normal','missing_version','invalid_id','id_mismatch','missing_version_bad_id','schema','source']
with tempfile.TemporaryDirectory(prefix='owned-server-cli-name-') as td:
 for family,kind in [('mods','mod'),('datapacks','datapack')]:
  for action in ['list-text','list-json','enable','disable','remove']:
   op=action.split('-')[0]
   cases=[(label,name,state) for label,name in names for state in metadata]+[(label,name,state) for label,name in names[:2] for state in ['guard','bad_properties','guard_bad_properties','bad_selector','guard_bad_selector','both_selectors','missing_selector']]
   for label,name,state in cases:
    row={'family':family,'action':action,'name':label,'state':state,'diagnostics':{}}
    for engine,exe in bins.items():
     t=Path(td)/str(len(rows))/engine;t.mkdir(parents=True);directory=t/'servers';server=directory/'demo';server.mkdir(parents=True)
     (server/'.jmcl').mkdir();guard=server/'.jmcl/lifecycle.lock'
     if 'guard' in state:guard.mkdir()
     else:guard.write_bytes(b'')
     manifest={'id':'demo','name':name,'version_id':'1.20.1'}
     if state in ['missing_version','missing_version_bad_id']:manifest.pop('version_id')
     if state in ['invalid_id','missing_version_bad_id']:manifest['id']='bad id'
     if state=='id_mismatch':manifest['id']='other'
     if state=='schema':manifest['schema_version']=2
     if state=='source':manifest['source']='bad'
     (server/'server.json').write_text(json.dumps(manifest));(server/'server.properties').write_text('level-name='+('../bad' if 'bad_properties' in state else 'world')+'\n')
     home=server/'world' if family=='datapacks' else server;(home/family).mkdir(parents=True);(home/'.jmcl').mkdir(exist_ok=True)
     relative=family+('/a.jar' if family=='mods' else '/a.zip');enabled=op!='enable';file=home/(relative+('' if enabled else '.disabled'));file.write_bytes(payload);peer=t/'peer';os.link(file,peer);peer.chmod(0o444)
     sibling=t/'sibling';sibling.write_bytes(b'preserve')
     entry={'kind':kind,'file':relative,'sha1':digest,'size':len(payload),'source':{'provider':'manual','project_id':None,'version_id':None,'slug':None},'enabled':enabled,'as_dependency':False}
     registry=home/'.jmcl/content.json';registry.write_text(json.dumps({'schema_version':1,'entries':[entry]}))
     argv=['server',family,op,'demo','--servers-directory',str(directory)]
     if action=='list-json':argv+=['--json']
     if op!='list':
      if state!='missing_selector':argv+=['--file','../bad' if 'bad_selector' in state else relative]
      if state=='both_selectors':argv+=['--project','project']
     pid=server/'.jmcl/server.pid';assert not pid.exists();initial=peer.stat();observed=[];states=[];row['diagnostics'][engine]=[]
     for step in range(1 if op=='list' else 2):
      pre=snap(t);o=subprocess.run([str(exe),*argv],capture_output=True,timeout=15);observed.append({'code':o.returncode,'out':o.stdout.hex(),'err':o.stderr.hex()});assert not pid.exists();ps=peer.stat()
      row['diagnostics'][engine].append({'step':step,'raw_snapshot':snap(t),'peer_identity':[ps.st_dev,ps.st_ino,ps.st_mode,ps.st_mtime_ns,ps.st_nlink]})
      require((ps.st_dev,ps.st_ino,ps.st_mode,ps.st_mtime_ns)==(initial.st_dev,initial.st_ino,initial.st_mode,initial.st_mtime_ns) and peer.read_bytes()==payload,(len(rows),engine,step,'immutable peer'))
      require(sibling.read_bytes()==b'preserve',(len(rows),engine,step,'sibling'))
      if label in ['string','ascii','unicode','opaque','control'] and state=='normal':require(o.returncode==(1 if op=='remove' and step==1 else 0),(len(rows),engine,step,'positive source expected'))
      reg=json.loads(registry.read_bytes());normal=(home/relative).exists();disabled=(home/(relative+'.disabled')).exists()
      if o.returncode or op=='list':require(snap(t)==pre,(len(rows),engine,step,'unchanged failure/list'))
      if not o.returncode:
       require(not o.stderr,(len(rows),engine,step,'success stderr'))
       if op=='list':
        if action=='list-json':require(json.loads(o.stdout)=={'entries':[entry],'unmanaged':[]},(len(rows),engine,step,'list object'))
        else:require(relative.encode() in o.stdout and digest.encode() in o.stdout,(len(rows),engine,step,'list text'))
       elif op=='remove':require(reg['entries']==[] and not normal and not disabled and ps.st_nlink==1,(len(rows),engine,step,'remove state'))
       else:
        require(reg['entries']==[dict(entry,enabled=op=='enable')],(len(rows),engine,step,'toggle registry'))
        require(normal==(op=='enable') and disabled==(op=='disable'),(len(rows),engine,step,'toggle paths'))
        fs=(home/(relative+('' if op=='enable' else '.disabled'))).stat();require((fs.st_dev,fs.st_ino,fs.st_mode)==(ps.st_dev,ps.st_ino,ps.st_mode) and ps.st_nlink==2,(len(rows),engine,step,'shared inode'))
      states.append({'registry_bytes':registry.read_bytes().hex(),'normal':normal,'disabled':disabled,'peer_links':ps.st_nlink})
     row[engine]={'responses':observed,'states':states}
    rows.append(row)
if args.output:Path(args.output).write_text(json.dumps(rows,indent=2))
diffs=[r for r in rows if r['zig']!=r['rust']]
print(len(rows),'sequences;',sum(len(r['zig']['responses']) for r in rows),'CLI pairs;',len(diffs),'differences;',len(violations),'invariant violations')
print(collections.Counter((r['name'],r['state']) for r in diffs))
for r in diffs:print(json.dumps(r))
print('violations',violations[:20]);assert not violations

assert not diffs

# Existing server_content.rs CLI tail matrix, restricted to four local operations.
argument_pairs=0
with tempfile.TemporaryDirectory(prefix='owned-server-cli-args-') as td:
 for family in ['mods','datapacks']:
  for op in ['list','enable','disable','remove']:
   for tail in [[],['demo'],['../invalid'],['demo','--unknown','value'],['demo','--workers','0'],['demo','--project','bad/id'],['demo','--file','../bad'],['demo','--project','x','--file','mods/a.jar'],['demo','--project','x','--provider','curseforge'],['demo','--project','x','--api-key','synthetic-key']]:
    observations=[]
    for engine,exe in bins.items():
     t=Path(td)/str(argument_pairs)/engine;server=t/'servers/demo';server.mkdir(parents=True)
     (server/'.jmcl').mkdir();(server/'.jmcl/lifecycle.lock').write_bytes(b'');(server/'World').mkdir()
     (server/'server.json').write_text(json.dumps({'id':'demo','name':'Test','version_id':'1.20.1'}))
     (server/'server.properties').write_text('level-name=World\n')
     argv=['server',family,op,*tail]
     if tail:argv+=['--servers-directory',str(t/'servers')]
     before=snap(t);p=subprocess.run([str(exe),*argv],capture_output=True,timeout=15)
     observations.append((p.returncode,p.stdout,p.stderr))
     assert not (server/'.jmcl/server.pid').exists()
     assert snap(t)==before,(family,op,tail,engine,'unexpected local argument fixture mutation')
    assert observations[0]==observations[1],(family,op,tail,observations)
    argument_pairs+=1
print(argument_pairs,'prior local CLI argument/target pairs exact; no excluded operations')
