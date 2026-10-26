"""Owned metadata reads only: no lifecycle, process state, mutation or acquisition."""
import argparse,json,os,subprocess,sys,tempfile
from pathlib import Path
p=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:p.add_argument('--'+key,required=True)
p.add_argument('--output');options=p.parse_args()
assert subprocess.check_output(['git','-C',options.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins={k:Path(getattr(options,k)).resolve() for k in ['zig','rust']}
def snap(t):return {str(x.relative_to(t)):[x.lstat().st_dev,x.lstat().st_ino,x.lstat().st_mode,x.lstat().st_mtime_ns,x.lstat().st_nlink,x.read_bytes().hex() if x.is_file() else None] for x in [t,*t.rglob('*')]}
base=b'{"id":"demo","name":"Demo","version_id":"fake"}'
cases=[('below',65535,False),('exact',65536,False),('over',65537,False),('large',131072,False),('early_bad_over',65537,True),('early_bad_small',32,True),('invalid_id_over',65537,False),('missing',0,False)]
if not sys.platform.startswith('linux'):
 with tempfile.TemporaryDirectory(prefix='owned-server-limit-gate-') as td:
  for name,_,_ in cases:
   for mode in ['text','json','get','list']:
    values=[]
    for exe in bins.values():
     ident='../bad' if name=='invalid_id_over' else 'demo'
     if mode in ['text','json']:
      argv=['server','show',ident,'--servers-directory',str(Path(td)/'not-opened')]+(['--json'] if mode=='json' else []);data=None
     else:
      argv=['rpc'];data=(json.dumps({'protocol':1,'id':'limit','method':'server.'+mode,'params':{'directory':str(Path(td)/'not-opened'),'id':ident}})+'\n').encode()
     r=subprocess.run([str(exe),*argv],input=data,cwd=td,capture_output=True,timeout=20)
     if mode in ['text','json']:assert (r.returncode,r.stdout,r.stderr)==(1,b'',b'Server management is currently supported only on Linux\n')
     else:assert r.returncode==0 and not r.stderr and json.loads(r.stdout)['error']['code']=='UNSUPPORTED_PLATFORM'
     values.append((r.returncode,r.stdout,r.stderr));assert list(Path(td).iterdir())==[]
    assert values[0]==values[1]
 print('32 unchanged non-Linux platform gates; no metadata path access')
 raise SystemExit(0)
rows=[];errors=[]
with tempfile.TemporaryDirectory(prefix='owned-server-manifest-size-') as temp:
 for name,size,bad in cases:
  for mode in ['text','json','get','list']:
   row={'case':name,'mode':mode,'diagnostics':{}}
   for engine,exe in bins.items():
    t=Path(temp)/str(len(rows))/engine;root=t/'servers';server=root/'demo';server.mkdir(parents=True)
    body=(b'{"source":"bad",' if bad else base).ljust(size,b' ')
    if name!='missing':
     path=server/'server.json';path.write_bytes(body);os.link(path,t/'peer');(t/'peer').chmod(0o444)
    (t/'sentinel').write_bytes(b'preserve');ident='../bad' if name=='invalid_id_over' else 'demo'
    if mode in ['text','json']:
     args=['server','show',ident,'--servers-directory',str(root)]+(['--json'] if mode=='json' else []);data=None
    else:
     args=['rpc'];data=(json.dumps({'protocol':1,'id':'limit','method':'server.'+mode,'params':{'directory':str(root),'id':ident}})+'\n').encode()
    before=snap(t);r=subprocess.run([str(exe),*args],input=data,cwd=t,capture_output=True,timeout=20);after=snap(t)
    row[engine]={'code':r.returncode,'out':r.stdout.hex(),'err':r.stderr.hex()};row['diagnostics'][engine]={'before':before,'after':after}
    if before!=after or (server/'.jmcl/server.pid').exists():errors.append([name,mode,engine,'changed metadata'])
    native=sys.platform.startswith('linux')
    if native and mode in ['text','json']:
     good=name in ['below','exact']
     if good:
      if r.returncode or r.stderr:errors.append([name,mode,engine,'expected success'])
      elif mode=='json':
       value=json.loads(r.stdout)['server']
       if (value['id'],value['name'],value['version_id'])!=('demo','Demo','fake'):errors.append([name,mode,engine,'positive fields'])
     else:
      expected=b'usage: jmcl-core server show ID [--servers-directory PATH] [--json]\n' if name=='invalid_id_over' else ('failed to load server: '+('FileNotFound' if name=='missing' else 'InvalidEnumTag' if name=='early_bad_small' else 'ServerManifestTooLarge')+'\n').encode()
      if (r.returncode,r.stdout,r.stderr)!=(2 if name=='invalid_id_over' else 1,b'',expected):errors.append([name,mode,engine,'expected diagnostic'])
    if mode in ['get','list']:
     if r.returncode or r.stderr:errors.append([name,mode,engine,'RPC process'])
     else:
      result=json.loads(r.stdout)
      if native and mode=='list' and len(result['result']['servers'])!=(1 if name in ['below','exact'] else 0):errors.append([name,mode,engine,'list positive size'])
      if native and mode=='get' and name in ['below','exact'] and result['result']['id']!='demo':errors.append([name,mode,engine,'get positive ID'])
   rows.append(row)
if options.output:Path(options.output).write_text(json.dumps(rows,indent=2))
diffs=[r for r in rows if r['zig']!=r['rust']]
print(len(rows),'read-only pairs',len(diffs),'differences',len(errors),'contract failures')
for r in diffs:print(json.dumps(r))
print(errors);assert not errors;assert not diffs
