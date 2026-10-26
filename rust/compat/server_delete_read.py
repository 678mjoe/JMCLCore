#!/usr/bin/env python3
"""Only owned no-state server deletion; never invokes process/lifecycle commands."""
from pathlib import Path
import hashlib,json,os,subprocess,tempfile
import argparse,sys
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
args=parser.parse_args();zig=Path(args.zig).resolve();rust=Path(args.rust).resolve()
assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
print('binary SHA256:',{str(b):hashlib.sha256(b.read_bytes()).hexdigest() for b in [zig,rust]})
native=sys.platform=='linux'
base={'id':'demo','name':'Demo','version_id':'fake'}
cases=[('ordinary',json.dumps(base),'demo')]
for label,value in [('ascii-array',[68,101,109,111]),('opaque-array',[255]),('control-array',[9,10,0]),('empty-array',[]),('empty-string',''),('invalid-byte',[256]),('name-null',None),('name-number',5)]:
 cases.append((label,json.dumps(dict(base,name=value)),'demo'))
cases += [('missing-name',json.dumps({'id':'demo','version_id':'fake'}),'demo'),('malformed','{','demo'),('duplicate',json.dumps(base)[:-1]+',"name":"duplicate"}','demo'),('id-mismatch',json.dumps(dict(base,id='other')),'demo'),('invalid-stored-id',json.dumps(dict(base,id='../bad')),'demo'),('missing-id',json.dumps({'name':'Demo','version_id':'fake'}),'demo'),('missing-manifest',None,'demo'),('invalid-request-id',json.dumps(base),'../bad'),('missing-target',json.dumps(base),'absent')]
def snap(root):
 return {str(p.relative_to(root)):(p.read_bytes().hex() if p.is_file() else None,p.stat().st_ino,p.stat().st_mode,p.stat().st_mtime_ns,p.stat().st_nlink) for p in [root,*root.rglob('*')]}
rows=[]
for label,raw,request_id in cases:
 for blocked in (([False,True] if label in ['ordinary','opaque-array','malformed','missing-manifest'] else [False]) + ([None] if label in ['ordinary','opaque-array','malformed','missing-manifest','empty-string','id-mismatch'] else [])):
  for mode in ['rpc','cli']:
   outputs=[]
   for binary in [zig,rust]:
    with tempfile.TemporaryDirectory(prefix='owned-no-state-delete-') as td:
     root=Path(td);home=root/'demo';home.mkdir()
     if blocked is not None:
      (home/'.jmcl').mkdir()
      lock=home/'.jmcl/lifecycle.lock'
      if blocked:lock.mkdir()
      else:lock.write_bytes(b'')
     if raw is not None:(home/'server.json').write_text(raw)
     (root/'peer').write_bytes(b'inert shared fixture');os.link(root/'peer',home/'opaque.bin')
     sibling=root/'sibling';sibling.mkdir();(sibling/'keep').write_bytes(b'untouched sibling')
     before=snap(root);peer=(root/'peer').stat();sibling_before=snap(sibling)
     assert not list(root.rglob('server.pid')) and not list(root.rglob('*.exe'))
     assert not any(p.is_file() and (p.stat().st_mode&0o111) for p in root.rglob('*'))
     if mode=='rpc':
      cmd=['rpc'];data=json.dumps({'protocol':1,'id':'delete','method':'server.delete','params':{'directory':'.','id':request_id}}).encode()+b'\n'
     else:cmd=['server','delete',request_id,'--servers-directory','.'];data=None
     p=subprocess.run([str(binary),*cmd],cwd=root,input=data,capture_output=True,timeout=15)
     assert snap(sibling)==sibling_before
     now=(root/'peer').stat();assert (root/'peer').read_bytes()==b'inert shared fixture'
     assert (now.st_ino,now.st_mode,now.st_mtime_ns)==(peer.st_ino,peer.st_mode,peer.st_mtime_ns)
     removed=not home.exists();assert now.st_nlink==(1 if removed else 2)
     expected_removed=native and blocked is not True and label in ['ordinary','ascii-array','opaque-array','control-array']
     assert removed==expected_removed,(label,blocked,mode,binary,'deletion outcome',removed,expected_removed)
     if not native:
      assert not removed and snap(root)==before
      if mode=='rpc':
       assert p.returncode==0 and not p.stderr
       assert json.loads(p.stdout)['error']=={'code':'UNSUPPORTED_PLATFORM','message':'Server management is currently supported only on Linux'}
      else:assert (p.returncode,p.stdout,p.stderr)==(1,b'',b'Server management is currently supported only on Linux\n')
     elif expected_removed:
      if mode=='rpc':
       assert p.returncode==0 and not p.stderr
       assert json.loads(p.stdout)['result']=={'stage':'server.delete','id':'demo','deleted':True}
      else:assert (p.returncode,p.stdout,p.stderr)==(0,b'deleted\tdemo\n',b'')
     elif mode=='rpc':
      if blocked is True:code,message='SERVER_STATUS_FAILED','Failed to validate the server lifecycle status'
      elif label in ['invalid-stored-id','missing-manifest','invalid-request-id','missing-target']:code,message='SERVER_NOT_FOUND','No server matches the given id'
      else:code,message='SERVER_DELETE_FAILED','Failed to delete the server'
      assert p.returncode==0 and not p.stderr and json.loads(p.stdout)['error']=={'code':code,'message':message},(label,p.stdout)
     else:
      if blocked is True:status,message=1,'failed to validate the server lifecycle status: ServerStatusFailed'
      elif label=='invalid-request-id':status,message=2,'usage: jmcl-core server delete ID [--servers-directory PATH]'
      elif label=='missing-target':status,message=1,'failed to open server directory: FileNotFound'
      else:
       error={'empty-array':'InvalidServerName','empty-string':'InvalidServerName','invalid-byte':'Overflow','name-null':'UnexpectedToken','name-number':'UnexpectedToken','missing-name':'MissingField','malformed':'UnexpectedEndOfInput','duplicate':'DuplicateField','id-mismatch':'ServerIdMismatch','invalid-stored-id':'InvalidServerId','missing-id':'MissingField','missing-manifest':'FileNotFound'}[label]
       status,message=1,'failed to delete server: '+error
      assert (p.returncode,p.stdout,p.stderr)==(status,b'',(message+'\n').encode()),(label,p)

     if not removed:
      after=snap(root)
      if blocked is not None:assert after==before,(label,blocked,mode,binary,'unexpected disk mutation')
      else:
       created=set(after)-set(before);assert created==({'demo/.jmcl','demo/.jmcl/lifecycle.lock'} if native else set()),(label,created)
       for name in before:
        if name!='demo':assert after[name]==before[name],(label,name,'unrelated change')
       if created:
        assert (home/'.jmcl').is_dir() and (home/'.jmcl/lifecycle.lock').read_bytes()==b''
        assert (home/'.jmcl/lifecycle.lock').stat().st_mode&0o777==0o600
        assert after['demo'][1:3]==before['demo'][1:3]
       else:assert after['demo']==before['demo']
     assert not list(root.rglob('server.pid'))
     files={str(q.relative_to(root)):q.read_bytes().hex() for q in root.rglob('*') if q.is_file()}
     outputs.append({'status':p.returncode,'stdout':p.stdout.decode(),'stderr':p.stderr.decode(),'deleted':removed,'files':files})
   rows.append({'case':label,'blocked_guard':blocked,'mode':mode,'equal':outputs[0]==outputs[1],'zig':outputs[0],'rust':outputs[1]})
for row in rows:
 assert row['equal'],row
print(f"PASS: {len(rows)} exact no-state server.delete RPC/CLI pairs; phase errors, guard artifacts and sibling/peer invariants checked")
