#!/usr/bin/env python3
"""Offline local/installed-runtime launch RPC+CLI differential corpus; no runtime downloads."""
import argparse,pathlib,tempfile,subprocess,json,hashlib,time,re,os,shutil
p=argparse.ArgumentParser();p.add_argument('--zig',required=True);p.add_argument('--rust',required=True);a=p.parse_args();bins=[str(pathlib.Path(x).resolve()) for x in [a.zig,a.rust]];count=0
with tempfile.TemporaryDirectory() as tmp:
 root=pathlib.Path(tmp);fake=root/('java.exe' if os.name=='nt' else 'java');src=pathlib.Path(__file__).resolve().parent.parent/'tests/fixtures/launch/child.rs';subprocess.run(['rustc',str(src),'-o',str(fake)],check=True)
 probe=subprocess.run([str(fake),'-XshowSettings:properties','-version'],capture_output=True);assert probe.returncode==0 and b'java.specification.version = 17' in probe.stderr
 cache=root/'.jmclcore/metadata-cache';cache.mkdir(parents=True)
 md={'id':'demo','type':'release','mainClass':'args','libraries':[],'downloads':{'client':{'sha1':'1'*40,'size':0,'url':'https://example.invalid/client'}},'javaVersion':{'component':'fixture','majorVersion':17},'arguments':{'jvm':[],'game':['${auth_access_token}','${auth_player_name}','${auth_uuid}','literal argument',';not shell']}}
 def seed():
  body=json.dumps(md,separators=(',',':'));manifest={'latest':{'release':'demo','snapshot':'demo'},'versions':[{'id':'demo','type':'release','url':'https://piston-meta.mojang.com/demo','time':'2026-01-01','releaseTime':'2026-01-01','sha1':hashlib.sha1(body.encode()).hexdigest()}]}
  for host in ['https://piston-meta.mojang.com','https://bmclapi2.bangbang93.com']:
   for url,payload in [(host+'/mc/game/version_manifest_v2.json',json.dumps(manifest)),(host+'/demo',body)]:
    (cache/(hashlib.sha1(url.encode()).hexdigest()+'.json')).write_text(json.dumps({'v':1,'url':url,'fetched_at':int(time.time()),'payload':payload}))
 seed()
 def run(exe,args,data=None):return subprocess.run([exe,*args],cwd=root,input=data,capture_output=True,timeout=15)
 def normalized(rows):
  for row in rows:
   if 'started' in row:assert row['started']['pid']>0;row['started']['pid']=0
   if 'result' in row and 'process' in row['result']:
    r=row['result']['process'];assert r['pid']>0 and r['duration_ms']>=0 and r['max_rss'] is not None
    for k in ['pid','duration_ms','max_rss']:r[k]=0
  return rows
 def rpc(params,success=False):
  global count
  req={'protocol':1,'id':str(count),'method':'launch.execute'}
  if params is not None:req['params']=params
  z,r=[run(exe,['rpc'],json.dumps(req).encode()+b'\n')for exe in bins]
  assert(z.returncode,z.stderr)==(r.returncode,r.stderr),(req,z.stderr,r.stderr)
  assert normalized([json.loads(x)for x in z.stdout.splitlines()])==normalized([json.loads(x)for x in r.stdout.splitlines()]),(req,z.stdout,r.stdout)
  assert b'fake-secret' not in r.stdout
  if success:assert json.loads(r.stdout.splitlines()[-1])['event']=='result' and b'"started"' in r.stdout
  natives=root/'game/.jmcl/runtime/natives'
  if natives.exists():assert not list(natives.iterdir())
  count+=1
 base={'id':'demo','directory':'game','auth':{'player_name':'Fixture','uuid':'fake-uuid','access_token':'fake-secret'},'java_override':str(fake)}
 for params in [None,{},[],dict(base,id=''),dict(base,directory=''),dict(base,auth=None),dict(base,auth=dict(base['auth'],access_token='')),dict(base,auth=dict(base['auth'],player_name='')),dict(base,java_policy='bad'),dict(base,java_paths=[str(fake)]),dict(base,id='missing'),dict(base,java_override=str(root/'missing')),dict(base,java_override='relative'),dict(base,fabric_loader='x',forge_version='y')]:rpc(params)
 for source in ['official','bmclapi']:
  rpc(dict(base,source=source),True)
  local=dict(base,source=source,java_policy='local',java_paths=[str(fake)]);del local['java_override'];rpc(local,True)
 # Managed policy with installed receipt: no download branch, no archive experiment.
 platform='windows-x64' if os.name=='nt' else ('mac-os' if __import__('sys').platform=='darwin' else 'linux')
 rt=root/'store/runtimes/mojang-fixture';(rt/'bin').mkdir(parents=True);rel='bin/java.exe' if os.name=='nt' else 'bin/java';shutil.copy2(fake,rt/rel)
 (rt/'.jmcl-runtime.json').write_text(json.dumps({'provider':'mojang','id':'fixture','platform':platform,'major_version':17,'java_home':'','executable':rel,'source_url':'https://example.invalid/runtime','installed_at_ms':123,'component':'fixture'}))
 installed=dict(base,java_policy='managed',java_paths=[],store_directory=str(root/'store'));del installed['java_override']
 # Plan preflight cannot download. Abort fixture before execute if receipt/probe is invalid.
 for exe in bins:
  pre=run(exe,['rpc'],json.dumps({'protocol':1,'id':'preflight','method':'launch.plan','params':dict(installed,natives_directory='unused')}).encode()+b'\n')
  assert json.loads(pre.stdout)['result']['java_source']=='managed',pre.stdout
 rpc(installed,True)
 def cli(args,success=False):
  global count
  z,r=[run(exe,args)for exe in bins]
  norm=lambda b:re.sub(rb'game started: pid [0-9]+',b'game started: pid PID',b)
  assert(z.returncode,z.stdout,norm(z.stderr))==(r.returncode,r.stdout,norm(r.stderr)),(args,z.returncode,z.stdout,z.stderr,r.returncode,r.stdout,r.stderr)
  if success:assert b'game started: pid ' in r.stderr
  count+=1
 for args in [['launch'],['launch','plan'],['launch','execute'],['launch','execute','demo'],['launch','execute','demo','--offline-name',''],['launch','execute','demo','--offline-name','Fixture','--bad','x'],['instance','launch'],['instance','launch','../bad','--offline-name','Fixture'],['instance','launch','missing','--offline-name','Fixture']]:cli(args)
 cli(['launch','execute','demo','--offline-name','Fixture','--directory','game','--java-override',str(fake)],True)
 inst=root/'instances/fixture';inst.mkdir(parents=True);(inst/'instance.json').write_text(json.dumps({'schema_version':1,'id':'fixture','name':'Fixture','version_id':'demo','source':'official'}))
 cli(['instance','launch','fixture','--offline-name','Fixture','--java-override',str(fake)],True)
 for name in ['玩家','x'*128]:cli(['launch','execute','demo','--offline-name',name,'--directory','game','--java-override',str(fake),'--arch','amd64'],True)
 md['mainClass']='emit';md['arguments']['game']=[];seed();rpc(base,True);cli(['launch','execute','demo','--offline-name','Fixture','--directory','game','--java-override',str(fake)],True)
print(f'{count} local/installed launch.execute + CLI differential checks passed (only PID/duration/RSS normalized)')
