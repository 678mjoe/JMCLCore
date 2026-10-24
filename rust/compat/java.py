#!/usr/bin/env python3
"""Offline Java detection/receipt differential corpus. No JVM/network install."""
import argparse,json,os,pathlib,subprocess,tempfile,hashlib,time
p=argparse.ArgumentParser();p.add_argument('--zig',required=True);p.add_argument('--rust',required=True);a=p.parse_args();zig=str(pathlib.Path(a.zig).resolve());rust=str(pathlib.Path(a.rust).resolve())
count=0
with tempfile.TemporaryDirectory() as temp:
 root=pathlib.Path(temp); env=dict(os.environ);env.update({'JAVA_TOOL_OPTIONS':'must-be-cleared','_JAVA_OPTIONS':'must-be-cleared','JDK_JAVA_OPTIONS':'must-be-cleared'})
 def run(binary,args,input=None):return subprocess.run([binary,*args],input=input,capture_output=True,cwd=root,env=env,timeout=30)
 def compare(args,input=None):
  global count
  z=run(zig,args,input);r=run(rust,args,input)
  assert (z.returncode,z.stdout,z.stderr)==(r.returncode,r.stdout,r.stderr),(args,input,z.returncode,z.stdout,z.stderr,r.returncode,r.stdout,r.stderr)
  count+=1
 def rpc(method,params=None):
  req={'protocol':1,'id':str(count),'method':method}
  if params is not None:req['params']=params
  compare(['rpc'],json.dumps(req).encode()+b'\n')
 for params in [None,{},[],{'paths':None},{'paths':[1]},{'paths':[]},{'paths':[str(root/'missing')]}]:rpc('java.detect',params)
 if os.name!='nt':
  fake=root/'java';fake.write_text('#!/bin/sh\n[ -z "$JAVA_TOOL_OPTIONS$_JAVA_OPTIONS$JDK_JAVA_OPTIONS" ] || exit 5\nprintf "java.specification.version = 17\\nos.arch = amd64\\nsun.arch.data.model = 64\\njava.home = /fixture/java\\njava.vendor = Fixture\\n" >&2\n');fake.chmod(0o755)
  rpc('java.detect',{'paths':[str(fake),str(fake)]});compare(['java','detect','--path',str(fake),'--json']);compare(['java','detect','--path',str(fake)])
  for name,content,mode in [('invalid','not-an-executable',0o755),('denied','',0o644)]:
   f=root/name;f.write_text(content);f.chmod(mode);rpc('java.detect',{'paths':[str(f)]})
 # Java select reads only pre-seeded metadata cache; no service downloads.
 cache=root/'.jmclcore'/'metadata-cache';cache.mkdir(parents=True,exist_ok=True)
 metadata={'id':'demo','type':'release','mainClass':'Fixture.Main','libraries':[],'javaVersion':{'component':'runtime','majorVersion':17}}
 payload=json.dumps(metadata,separators=(',',':'))
 manifest={'latest':{'release':'demo','snapshot':'demo'},'versions':[{'id':'demo','type':'release','url':'https://piston-meta.mojang.com/demo','time':'2026-01-01','releaseTime':'2026-01-01','sha1':hashlib.sha1(payload.encode()).hexdigest()}]}
 for host in ['https://piston-meta.mojang.com','https://bmclapi2.bangbang93.com']:
  for url,body in [(host+'/mc/game/version_manifest_v2.json',json.dumps(manifest)),(host+'/demo',payload)]:
   (cache/(hashlib.sha1(url.encode()).hexdigest()+'.json')).write_text(json.dumps({'v':1,'url':url,'fetched_at':int(time.time()),'payload':body}))
 for params in [None,{},[],{'id':''},{'id':'missing'},{'id':'demo','arch':'amd64'},{'id':'demo','source':None},{'id':'demo','paths':None}]:rpc('java.select',params)
 for args in [['java','select'],['java','select','demo','--arch','bad'],['java','select','missing']]:compare(args)
 if os.name!='nt':
  for source in ['official','bmclapi']:
   for arch in ['x86_64','arm64']:
    rpc('java.select',{'id':'demo','source':source,'arch':arch,'paths':[str(fake)]})
    for json_args in [[],['--json']]:compare(['java','select','demo','--source',source,'--arch',arch,'--path',str(fake),*json_args])
 for args in [['java'],['java','detect','--path'],['java','detect','--bad'],['java','runtime'],['java','runtime','list','--bad'],['java','runtime','remove'],['java','runtime','remove','a','--json']]:compare(args)
 for params in [None,{},[],{'store_directory':''},{'store_directory':12}]:rpc('java.runtime.list',params)
 store=root/'store';runtimes=store/'runtimes';runtimes.mkdir(parents=True)
 fixture={'provider':'mojang','id':'java-runtime-gamma','platform':'linux','major_version':17,'java_home':'','executable':'bin/java','source_url':'https://example.invalid/runtime','installed_at_ms':123}
 for name,data in [('mojang-gamma-linux',fixture),('invalid',{'provider':'oops'}),('zulu-z-linux',dict(fixture,provider='zulu',major_version='8',files_total='1',bytes_total=23))]:
  d=runtimes/name;d.mkdir();(d/'.jmcl-runtime.json').write_text(json.dumps(data))
 rpc('java.runtime.list',{'store_directory':str(store)});compare(['java','runtime','list','--store',str(store),'--json']);compare(['java','runtime','list','--store',str(store)])
 for params in [None,{},[],{'runtime':''},{'runtime':'../x'},{'runtime':'x.staging-abc'},{'runtime':'missing','store_directory':str(store)}]:rpc('java.runtime.remove',params)
 # Use distinct synthetic twins for mutation, compare outputs then absence.
 for binary in [zig,rust]:
  d=runtimes/'remove-me';d.mkdir();(d/'.jmcl-runtime.json').write_text(json.dumps(fixture))
  result=run(binary,['rpc'],json.dumps({'protocol':1,'id':'remove','method':'java.runtime.remove','params':{'runtime':'remove-me','store_directory':str(store)}}).encode()+b'\n')
  assert result.returncode==0 and json.loads(result.stdout)['result']=={'removed':True,'runtime':'remove-me'} and not d.exists()
 count+=2
print(f'{count} Java RPC/CLI and direct-disk fixture checks passed')
