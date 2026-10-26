#!/usr/bin/env python3
"""Local project selectors: empty owned registries, no matching mutation."""
from pathlib import Path
import hashlib,json,os,subprocess,tempfile,sys
import argparse
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
args=parser.parse_args()
zig=Path(args.zig).resolve();rust=Path(args.rust).resolve()
assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
print('binary SHA256:', {str(b):hashlib.sha256(b.read_bytes()).hexdigest() for b in [zig,rust]})
CASES=[('string-valid','"abc"'),('array-valid','[97,98,99]'),('array-mixed','["97",98.0,99]'),('raw-rounding','[97.0000000000000001]'),('quoted-precise','["97.0000000000000001"]'),('opaque','[255]'),('empty','[]'),('string-empty','""'),('null','null'),('overflow','[256]'),('fraction','[1.5]'),('negative','[-1]'),('negative-zero','[-0]'),('boolean','[true]'),('nested-array','[[97]]'),('nested-object','[{}]'),('null-element','[null]'),('numeric-string-underscore','["9_7"]'),('numeric-string-hex','["0x1.84p6"]'),('raw-underflow','[1e-400]'),('quoted-underflow','["1e-400"]'),('raw-huge','[1e400]'),('string-invalid','"bad/ref"'),('array-invalid','[98,97,100,47,114,101,102]'),('array-control','[0]'),('string-unicode','"中文"')]
for label,token in list(CASES[:8]):
 CASES.append((label+'-conflict',token))
rows=[]
def snap(root):
 return {str(p.relative_to(root)):(p.read_bytes().hex() if p.is_file() else None,p.stat().st_ino,p.stat().st_mode,p.stat().st_mtime_ns,p.stat().st_nlink) for p in [root,*root.rglob('*')]}
with tempfile.TemporaryDirectory(prefix='owned-project-selector-') as td:
 root=Path(td);home=root/'demo';home.mkdir();game=home/'.minecraft'
 (home/'server.json').write_text('{"id":"demo","name":"Demo","version_id":"demo"}')
 (home/'server.properties').write_text('level-name=World\n')
 for path in [game,game/'saves/World',home,home/'World']:
  (path/'.jmcl').mkdir(parents=True,exist_ok=True)
  (path/'.jmcl/content.json').write_text('{"schema_version":1,"entries":[]}')
 (home/'.jmcl/lifecycle.lock').write_bytes(b'')
 (root/'blocked').write_bytes(b'not a directory')
 (root/'sibling').write_bytes(b'untouched');os.link(root/'sibling',root/'sibling-link')
 before=snap(root)
 for server,families in [(False,['mods','resourcepacks','shaderpacks','datapacks']),(True,['mods','datapacks'])]:
  if server and sys.platform!='linux':continue
  for family in families:
   for op in ['enable','disable','remove']:
    for root_case in ['valid','blocked'] + (['bad-guard','bad-id'] if server else []) + (['bad-world'] if family=='datapacks' else []):
     if root_case=='bad-guard':
      (home/'.jmcl/lifecycle.lock').unlink();(home/'.jmcl/lifecycle.lock').mkdir()
     if root_case=='bad-world' and server:(home/'server.properties').write_text('level-name=../bad\n')
     before=snap(root)
     for label,token in CASES:
      params={'directory':('blocked' if root_case=='blocked' else '.' if server else 'demo/.minecraft')}
      if server:params['id']='../bad' if root_case=='bad-id' else 'demo'
      elif family=='datapacks':params['world']='Absent' if root_case=='bad-world' else 'World'
      if label.endswith('-conflict'):params['file']='missing.jar'
      raw=json.dumps(params)[:-1]+',"project":'+token+'}'
      method=('server.' if server else '')+family+'.'+op
      wire='{"protocol":1,"id":"selector","method":'+json.dumps(method)+',"params":'+raw+'}\n'
      outputs=[];wire_outputs=[]
      for binary in [zig,rust]:
       p=subprocess.run([str(binary),'rpc'],cwd=root,input=wire.encode(),capture_output=True,timeout=15)
       assert p.returncode==0 and not p.stderr,(method,label,p)
       assert snap(root)==before,(method,label,root_case,binary,'disk changed')
       value=json.loads(p.stdout);assert 'error' in value,(method,label,'unexpected matching mutation')
       outputs.append(value['error']);wire_outputs.append({'status':p.returncode,'stdout_hex':p.stdout.hex(),'stderr_hex':p.stderr.hex()})
      rows.append({'method':method,'root':root_case,'case':label,'project':token,'equal':wire_outputs[0]==wire_outputs[1],'zig':outputs[0],'rust':outputs[1],'wire':wire_outputs})
     if root_case=='bad-guard':
      (home/'.jmcl/lifecycle.lock').rmdir();(home/'.jmcl/lifecycle.lock').write_bytes(b'')
     if root_case=='bad-world' and server:(home/'server.properties').write_text('level-name=World\n')
 # Existing CLI String project behavior is unchanged; no-match/invalid only.
 (home/'instance.json').write_text('{"id":"demo","name":"Demo","version_id":"demo"}')
 cli_count=0
 for server,families in [(False,['mods','resourcepacks','shaderpacks','datapacks']),(True,['mods','datapacks'])]:
  if server and sys.platform!='linux':continue
  for family in families:
   for op in ['enable','disable','remove']:
    for project in ['abc','bad/ref']:
     cmd=['server' if server else 'instance',family,op,'demo','--project',project,'--servers-directory' if server else '--instances-directory','.']
     if family=='datapacks' and not server:cmd+=['--world','World']
     before=snap(root);outputs=[]
     for binary in [zig,rust]:
      p=subprocess.run([str(binary),*cmd],cwd=root,capture_output=True,timeout=15)
      assert p.returncode==1,(cmd,p)
      assert snap(root)==before,(cmd,'CLI changed disk')
      outputs.append((p.returncode,p.stdout,p.stderr))
     assert outputs[0]==outputs[1],(cmd,outputs)
     cli_count+=1
 print(f'PASS: {cli_count} exact unchanged String-project CLI controls')
for row in rows:
 assert row['equal'], row
# Positive grammar controls must reach the expected no-match result, not equal
# parse errors; malformed quoted precision must still fail parameter parsing.
for row in rows:
 if row['root']=='valid' and row['case'] in ['string-valid','array-valid','array-mixed','raw-rounding','numeric-string-underscore','numeric-string-hex']:
  assert row['zig']=={'code':'CONTENT_NOT_FOUND','message':'No registered content matches the given selector'}, row
 if row['case']=='quoted-precise':
  assert row['zig']['code']=='INVALID_PARAMS' and row['zig']['message'].endswith('parameters are invalid'), row
print(f"PASS: {len(rows)} exact project-selector RPC comparisons; no matching entries or disk changes")
# Positive controls below are separately authorized owned local mutations. They
# prove the decoded project selects exactly its entry, not merely equal errors.
positive = 0
for server, families in [(False,['mods']), (True,['mods'])]:
 if server and sys.platform!='linux':continue
 for family in families:
  for selector in ['"abc"','[97,98,99]']:
   histories=[]
   for binary in [zig,rust]:
    with tempfile.TemporaryDirectory(prefix='owned-project-match-') as td:
     root=Path(td);home=root/'demo';home.mkdir();game=home if server else home/'.minecraft'
     if server:
      (home/'server.json').write_text('{"id":"demo","name":"Demo","version_id":"demo"}')
      (home/'server.properties').write_text('level-name=World\n')
      (home/'.jmcl').mkdir();(home/'.jmcl/lifecycle.lock').write_bytes(b'')
     content=game/('World' if server else 'saves/World') if family=='datapacks' else game
     (content/'.jmcl').mkdir(parents=True,exist_ok=True);(content/family).mkdir()
     peer=root/'peer';peer.write_bytes(b'opaque local fixture')
     target=content/family/'target.jar';sibling=content/family/'sibling.jar'
     os.link(peer,target);os.link(peer,sibling)
     st=peer.stat();identity=(st.st_ino,st.st_mode,st.st_mtime_ns)
     def entry(name,provider,project):
      return {'kind':{'mods':'mod','resourcepacks':'resourcepack','shaderpacks':'shaderpack','datapacks':'datapack'}[family], 'file':family+'/'+name,'sha1':hashlib.sha1(peer.read_bytes()).hexdigest(),'size':len(peer.read_bytes()),'source':{'provider':provider,'project_id':project,'version_id':'V1' if project else None,'slug':'alias' if project else None},'enabled':True,'as_dependency':False}
     registry=content/'.jmcl/content.json';registry.write_text(json.dumps({'schema_version':1,'entries':[entry('target.jar','modrinth','abc'),entry('sibling.jar','manual',None)]}))
     other_before=json.loads(registry.read_bytes())['entries'][1]
     history=[]
     for op in ['disable','enable','remove']:
      params={'directory':'.' if server else 'demo/.minecraft'}
      if server:params['id']='demo'
      elif family=='datapacks':params['world']='World'
      raw=json.dumps(params)[:-1]+',"project":'+selector+'}'
      method=('server.' if server else '')+family+'.'+op
      wire='{"protocol":1,"id":"matched","method":'+json.dumps(method)+',"params":'+raw+'}\n'
      result=subprocess.run([str(binary),'rpc'],input=wire.encode(),cwd=root,capture_output=True,timeout=15)
      assert result.returncode==0 and not result.stderr,(method,result)
      out=json.loads(result.stdout);assert 'result' in out and 'error' not in out,(method,out)
      doc=json.loads(registry.read_bytes());untouched=[e for e in doc['entries'] if e['file'].endswith('sibling.jar')]
      assert untouched==[other_before],(method,doc)
      remaining=[e for e in doc['entries'] if e['file'].endswith('target.jar')]
      disabled=target.with_name('target.jar.disabled')
      if op=='remove':
       assert not remaining and not target.exists() and not disabled.exists()
       assert out['result']['removed']['source']['project_id']=='abc'
      else:
       assert len(remaining)==1 and remaining[0]['enabled']==(op=='enable')
       assert target.exists()==(op=='enable') and disabled.exists()==(op=='disable')
       assert out['result']['changed'] is True and out['result']['entry']['source']['project_id']=='abc'
      for p in [peer,sibling]:
       now=p.stat();assert p.read_bytes()==b'opaque local fixture'
       assert (now.st_ino,now.st_mode,now.st_mtime_ns)==identity
       assert now.st_nlink==(2 if op=='remove' else 3)
      disk={str(p.relative_to(root)):p.read_bytes().hex() for p in root.rglob('*') if p.is_file()}
      history.append((result.returncode,result.stdout.hex(),result.stderr.hex(),disk))
     histories.append(history)
   assert histories[0]==histories[1],(server,family,selector,histories)
   positive+=3
print(f'PASS: {positive} exact positive String/byte-project mutation comparisons; sibling/peer bytes and identity preserved')
