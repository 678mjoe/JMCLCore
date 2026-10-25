#!/usr/bin/env python3
"""Instance install CLI parity with pre-existing verified fake bytes; no HTTP downloads."""
import argparse,pathlib,tempfile,subprocess,json,hashlib,time
p=argparse.ArgumentParser();p.add_argument('--zig',required=True);p.add_argument('--rust',required=True);a=p.parse_args();bins=[str(pathlib.Path(x).resolve())for x in[a.zig,a.rust]];count=0
with tempfile.TemporaryDirectory() as tmp:
 root=pathlib.Path(tmp);data=b'benign fixture artifact';digest=hashlib.sha1(data).hexdigest();cache=root/'.jmclcore/metadata-cache';cache.mkdir(parents=True)
 metadata={'id':'demo','type':'release','mainClass':'Fake.Main','libraries':[],'downloads':{'client':{'sha1':digest,'size':len(data),'url':'https://example.invalid/client'}}};body=json.dumps(metadata);manifest={'latest':{'release':'demo','snapshot':'demo'},'versions':[{'id':'demo','type':'release','url':'https://piston-meta.mojang.com/demo','time':'2026-01-01','releaseTime':'2026-01-01','sha1':hashlib.sha1(body.encode()).hexdigest()}]}
 for host in ['https://piston-meta.mojang.com','https://bmclapi2.bangbang93.com']:
  for url,payload in[(host+'/mc/game/version_manifest_v2.json',json.dumps(manifest)),(host+'/demo',body)]:
   (cache/(hashlib.sha1(url.encode()).hexdigest()+'.json')).write_text(json.dumps({'v':1,'url':url,'fetched_at':int(time.time()),'payload':payload}))
 def compare(args,success=False):
  global count
  results=[];receipts=[]
  for n,exe in enumerate(bins):
   base=root/f'case{count}-{n}';instance=base/'instances/fixture';game=instance/'.minecraft';(game/'versions/demo').mkdir(parents=True);(game/'versions/demo/demo.jar').write_bytes(data);doc={'schema_version':1,'id':'fixture','name':'Fixture','version_id':'demo','source':'official'};(instance/'instance.json').write_text(json.dumps(doc));before=(instance/'instance.json').read_bytes()
   cmd=[*args,'--instances-directory',str(base/'instances'),'--store-directory',str(base/'store')] if args[:3]==['instance','install','fixture'] else args
   out=subprocess.run([exe,*cmd],cwd=root,capture_output=True,timeout=10);results.append((out.returncode,out.stdout,out.stderr))
   assert (instance/'instance.json').read_bytes()==before
   receipt=game/'.jmcl/install.json'
   if success:assert out.returncode==0 and receipt.exists(),results[-1];receipts.append(receipt.read_bytes());assert (game/'versions/demo/demo.jar').read_bytes()==data
  assert results[0]==results[1],(args,results)
  if success:assert receipts[0]==receipts[1]
  count+=1
 for args in [['instance','install'],['instance','install','../x'],['instance','install','missing'],['instance','install','fixture','--workers','0'],['instance','install','fixture','--workers','33'],['instance','install','fixture','--retries','6'],['instance','install','fixture','--source','bad'],['instance','install','fixture','--java-policy','bad'],['instance','install','fixture','--arch','bad'],['instance','install','fixture','--java-override','x'],['instance','install','fixture','--os','macos']]:compare(args)
 for flags in [[],['--source','bmclapi'],['--workers','+1','--retries','-0'],['--workers','0__2'],['--arch','amd64'],['--java','literal path','--java-policy','local']]:compare(['instance','install','fixture',*flags],True)
print(f'{count} exact instance install CLI/receipt checks passed')
