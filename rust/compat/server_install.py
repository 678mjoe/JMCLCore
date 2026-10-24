#!/usr/bin/env python3
"""Pinned server creation/installation oracle. All artifacts/cache/runtime are
synthetic temp fixtures; no Minecraft launch, remote accounts or live servers.
"""
import argparse,hashlib,json,pathlib,subprocess,tempfile,time,os
p=argparse.ArgumentParser();p.add_argument('--zig',required=True);p.add_argument('--rust',required=True);p.add_argument('--oracle-source',required=True);a=p.parse_args()
zig=str(pathlib.Path(a.zig).resolve());rust=str(pathlib.Path(a.rust).resolve());pin='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
assert subprocess.check_output(['git','-C',a.oracle_source,'rev-parse','HEAD'],text=True).strip()==pin
print(json.dumps({'oracle_commit':pin,'zig_sha256':hashlib.sha256(pathlib.Path(zig).read_bytes()).hexdigest(),'rust_sha256':hashlib.sha256(pathlib.Path(rust).read_bytes()).hexdigest()}))
count=0;diff=[]
def check(label,x,y):
 global count
 count+=1
 if x!=y:diff.append((label,x,y))
def rpc(exe,method,params,cwd):
 req=dict(protocol=1,id='fixture',method=method)
 if params!='MISSING':req['params']=params
 p=subprocess.run([exe,'rpc'],input=json.dumps(req)+'\n',text=True,capture_output=True,cwd=cwd,timeout=12)
 assert p.returncode==0,p.stderr
 return [json.loads(s) for s in p.stdout.splitlines()]
def cli(exe,args,cwd):
 p=subprocess.run([exe,'server']+args,text=True,capture_output=True,cwd=cwd,timeout=12)
 out=p.stdout
 if '--json' in args and out.strip():out=json.loads(out)
 return p.returncode,out,p.stderr
sha=lambda b:hashlib.sha1(b).hexdigest()
def setup(root,loader=None,server=True):
 root.mkdir(parents=True,exist_ok=True);cache=root/'.jmclcore/metadata-cache';cache.mkdir(parents=True)
 downloads={'client':{'url':'https://fixture.invalid/client.jar','sha1':sha(b'jar'),'size':3}}
 if server:downloads['server']={'url':'https://fixture.invalid/server.jar','sha1':sha(b'jar'),'size':3}
 metadata=json.dumps({'id':'fixture','type':'release','mainClass':'Main','libraries':[],'downloads':downloads,'javaVersion':{'component':'synthetic','majorVersion':21}})
 manifest=json.dumps({'latest':{'release':'fixture','snapshot':'fixture'},'versions':[{'id':'fixture','type':'release','url':'https://piston-meta.mojang.com/fixture.json','sha1':sha(metadata.encode()),'time':'now','releaseTime':'now'}]})
 entries=[('https://piston-meta.mojang.com/mc/game/version_manifest_v2.json',manifest),('https://piston-meta.mojang.com/fixture.json',metadata)]
 if loader:
  for side,main in [('client','KnotClient'),('server','KnotServer')]:
   profile=json.dumps({'id':'fabric-loader-0.16.10-fixture','inheritsFrom':'fixture','mainClass':'net.fabricmc.loader.impl.launch.knot.'+main,'libraries':[]})
   entries.append((f'https://meta.fabricmc.net/v2/versions/loader/fixture/0.16.10/{"profile/json" if side=="client" else "server/json"}',profile))
 for url,payload in entries:(cache/(sha(url.encode())+'.json')).write_text(json.dumps({'v':1,'url':url,'fetched_at':int(time.time()),'payload':payload}))
 # Benign probe-only executable: emits properties, never launches another process.
 java=root/'fixture-java';java.write_text('#!/bin/sh\nprintf "java.specification.version = 21\\nos.arch = amd64\\n" >&2\n');java.chmod(0o700)
 obj=root/'store/objects/sha1'/sha(b'jar')[:2]/sha(b'jar');obj.parent.mkdir(parents=True);obj.write_bytes(b'jar');obj.chmod(0o444)
 return str(java)
def normalize(v,root):
 if isinstance(v,str):return v.replace(str(root),'ROOT')
 if isinstance(v,list):return [normalize(x,root) for x in v]
 if isinstance(v,tuple):return tuple(normalize(x,root) for x in v)
 if isinstance(v,dict):return {k:normalize(x,root) for k,x in v.items()}
 return v
with tempfile.TemporaryDirectory() as temp:
 root=pathlib.Path(temp)
 for method in ['server.create','server.install']:
  base={'directory':'servers','id':'demo','version_id':'fixture'}
  cases=['MISSING',None,[],{},dict(base,id=''),dict(base,directory=''),dict(base,id='../x'),dict(base,source='bad'),dict(base,fabric_loader='x',forge_version='y'),dict(base,workers=0),dict(base,retries=6)]
  if method=='server.create':cases=cases[:9]
  else:cases += [dict(base,accept_eula=True,id='absent'),dict(base,accept_eula=True,workers='2e1'),dict(base,accept_eula=True,id='../x')]
  for i,value in enumerate(cases):
   # No metadata access: creation cases all fail before resolution.
   check((method,i),rpc(zig,method,value,root),rpc(rust,method,value,root))
 for command in ['create','install']:
  for tail in [[],['bad/id'],['demo'],['demo','--unknown'],['demo','--workers','0'],['demo','--servers-directory',''],['demo','--accept-eula','--servers-directory','servers'],['demo','--version','fixture','--source','bad']]:
   check((command,tail),cli(zig,[command]+tail,root),cli(rust,[command]+tail,root))
 for method in ['server.create','server.install']:
  for field in (['directory','id','version_id','name','source','java_path','accept_eula'] if method=='server.create' else ['directory','id','store_directory','workers','retries','java_paths','accept_eula']):
   for value in [None,False,0,[],{},[100],['x'],'1.0']:
    # Invalid scalar cases resolve only against seeded metadata, never network.
    results=[]
    for name,exe in [('zig',zig),('rust',rust)]:
     case=root/f'schema-{count}-{name}';setup(case)
     params={'directory':'servers','id':'demo','version_id':'fixture'}
     params[field]=value
     results.append(normalize(rpc(exe,method,params,case),case))
    check((method,field,value),*results)
 for mode in ['rpc','cli']:
  for loader in [None,'fabric']:
   for scenario in ['create','install','missing-download','properties-dir','existing-properties']:
    results=[]
    for name,exe in [('zig',zig),('rust',rust)]:
     case=root/f'{mode}-{loader}-{scenario}-{name}';java=setup(case,loader,scenario!='missing-download')
     params={'directory':'servers','id':'demo','version_id':'fixture','java_path':java}
     if loader:params['fabric_loader']='0.16.10'
     create_args=['create','demo','--version','fixture','--java-path',java,'--json']+(['--fabric-loader','0.16.10'] if loader else [])
     created=rpc(exe,'server.create',params,case) if mode=='rpc' else cli(exe,create_args,case)
     if scenario=='create':result=created
     else:
      server_dir=case/'servers/demo'
      assert server_dir.exists(),created
      if scenario=='properties-dir':(server_dir/'server.properties').mkdir()
      elif scenario=='existing-properties':(server_dir/'server.properties').write_text('custom=preserve\n')
      params={'directory':'servers','id':'demo','accept_eula':True,'store_directory':'store','workers':1,'retries':0}
      result=rpc(exe,'server.install',params,case) if mode=='rpc' else cli(exe,['install','demo','--accept-eula','--store-directory','store','--workers','1','--retries','0','--json'],case)
     files={str(f.relative_to(case/'servers/demo')):f.read_text() for f in (case/'servers/demo').rglob('*') if f.is_file() and f.name not in ['lifecycle.lock']}
     # JSON object ordering is not a wire contract; compare disk documents semantically.
     for path in files:
      if path.endswith('.json'):files[path]=json.loads(files[path])
     results.append(normalize((result,files),case))
    check((mode,loader,scenario),*results)
print(json.dumps({'cases':count,'differences':len(diff)}))
for item in diff:print(json.dumps(item,ensure_ascii=False))
raise SystemExit(bool(diff))
