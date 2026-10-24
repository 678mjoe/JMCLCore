#!/usr/bin/env python3
"""Offline exact launch.plan JSONL comparisons. Synthetic auth and native fake Java only."""
import argparse, pathlib, subprocess, tempfile, json, hashlib, time, os
p=argparse.ArgumentParser();p.add_argument('--zig',required=True);p.add_argument('--rust',required=True);a=p.parse_args();zig=str(pathlib.Path(a.zig).resolve());rust=str(pathlib.Path(a.rust).resolve());count=0
with tempfile.TemporaryDirectory() as tmp:
 root=pathlib.Path(tmp);fake=root/('java.exe' if os.name=='nt' else 'java');src=pathlib.Path(__file__).resolve().parent.parent/'tests/fixtures/java_probe.rs';subprocess.run(['rustc',str(src),'-o',str(fake)],check=True)
 cache=root/'.jmclcore/metadata-cache';cache.mkdir(parents=True)
 def seed(md):
  body=json.dumps(md,separators=(',',':'));manifest={'latest':{'release':'demo','snapshot':'demo'},'versions':[{'id':'demo','type':'release','url':'https://piston-meta.mojang.com/demo','time':'2026-01-01','releaseTime':'2026-01-01','sha1':hashlib.sha1(body.encode()).hexdigest()}]}
  for host in ['https://piston-meta.mojang.com','https://bmclapi2.bangbang93.com']:
   for url,payload in [(host+'/mc/game/version_manifest_v2.json',json.dumps(manifest)),(host+'/demo',body)]:
    (cache/(hashlib.sha1(url.encode()).hexdigest()+'.json')).write_text(json.dumps({'v':1,'url':url,'fetched_at':int(time.time()),'payload':payload}))
 def compare(params):
  global count
  req={'protocol':1,'id':str(count),'method':'launch.plan'}
  if params is not None:req['params']=params
  data=json.dumps(req).encode()+b'\n';results=[subprocess.run([exe,'rpc'],cwd=root,input=data,capture_output=True,timeout=15)for exe in [zig,rust]]
  z,r=results;assert(z.returncode,z.stdout,z.stderr)==(r.returncode,r.stdout,r.stderr),(req,z.stdout,z.stderr,r.stdout,r.stderr)
  assert b'fake-token' not in r.stdout and b'fake-session' not in r.stdout
  count+=1
 md={'id':'demo','type':'release','mainClass':'Fake.Main','libraries':[],'downloads':{'client':{'sha1':'1'*40,'size':0,'url':'https://example.invalid/client'}},'javaVersion':{'component':'fixture','majorVersion':17},'arguments':{'jvm':['-cp','${classpath}'],'game':['${auth_access_token}','${auth_session}','${auth_player_name}','${resolution_width}']}}
 seed(md)
 base={'id':'demo','directory':'literal root','natives_directory':'literal natives','auth':{'player_name':'Fixture','uuid':'fake-uuid','access_token':'fake-token','session':'fake-session'},'java_override':str(fake)}
 for v in [None,{},[],dict(base,id=''),dict(base,directory=''),dict(base,natives_directory=''),dict(base,java_policy='bad'),dict(base,java_paths=[str(fake)]),dict(base,auth=None),dict(base,options=None),dict(base,source=None),dict(base,arch='amd64'),dict(base,id='missing'),dict(base,fabric_loader='x',forge_version='y')]:compare(v)
 for source in ['official','bmclapi']:
  for policy in ['auto','local','managed']:
   compare(dict(base,source=source,java_policy=policy))
 for options in [{'resolution_width':'1280','resolution_height':720.0},{'extra_jvm_args':['a b','${literal}','fake-token']},{'extra_game_args':['fake-session',';not shell']},{'demo':None},{'demo':1},{'resolution_width':-1},{'extra_game_args':None},{'include_default_user_jvm':False}]:compare(dict(base,options=options))
 for v in ["1_280","+1280","-0","1e3","4294967295","4294967296","1.5"," 10 "]:
  compare(dict(base,options={"resolution_width":v}))
 for auth in [dict(base['auth'],access_token=''),dict(base['auth'],session=None),dict(base['auth'],access_token=17),dict(base['auth'],player_name='')]:compare(dict(base,auth=auth))
 for token in ['${unknown}','${unfinished','${}','${auth_access_token}']:
  md['arguments']['game']=[token];seed(md);compare(base)
print(f'{count} exact offline launch.plan RPC checks passed')
