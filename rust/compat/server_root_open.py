#!/usr/bin/env python3
"""Owned server show/delete roots; absent state and no executable fixtures."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

p=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:p.add_argument('--'+key,required=True)
a=p.parse_args();binaries=[Path(a.zig).resolve(),Path(a.rust).resolve()]
assert subprocess.check_output(['git','-C',a.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
print('binary SHA256:',{str(b):hashlib.sha256(b.read_bytes()).hexdigest() for b in binaries})
native=sys.platform=='linux'

def snapshot(root):
 out={}
 for path in [root,*root.rglob('*')]:
  st=path.lstat();data=os.readlink(path) if path.is_symlink() else path.read_bytes() if path.is_file() else None
  out[str(path.relative_to(root))]=(data,st.st_ino,st.st_mode,st.st_mtime_ns,st.st_nlink)
 return out

cases=['normal','missing-root','root-file','ancestor-file','target-file','empty-root']
if os.name!='nt':cases+=['root-alias','ancestor-alias','dangling-root-alias','target-alias']
else:print('Symlink fixtures not exercised on Windows; server platform gate remains required.')
count=0
for case in cases:
 for command in ['show','delete']:
  for mode in ['rpc','cli']:
   outputs=[]
   for binary in binaries:
    with tempfile.TemporaryDirectory(prefix='owned-server-roots-') as td:
     root=Path(td);(root/'sibling').mkdir();(root/'sibling/keep').write_bytes(b'sibling')
     peer=root/'peer';peer.write_bytes(b'owned inert bytes')
     directory='servers';physical=root/'servers';physical.mkdir()
     if case=='missing-root':directory='missing/nested';physical=root/directory
     elif case=='root-file':(root/'blocked').write_bytes(b'file');directory='blocked';physical=None
     elif case=='ancestor-file':(root/'blocked').write_bytes(b'file');directory='blocked/nested';physical=None
     elif case=='empty-root':directory='';physical=None
     elif case=='root-alias':(root/'alias').symlink_to('servers',target_is_directory=True);directory='alias'
     elif case=='ancestor-alias':
      (root/'alias').symlink_to('servers',target_is_directory=True);directory='alias/nested';physical=root/'servers/nested';physical.mkdir()
     elif case=='dangling-root-alias':(root/'alias').symlink_to('absent',target_is_directory=True);directory='alias';physical=None
     home=None
     if physical is not None and case!='missing-root':
      home=physical/'demo'
      if case=='target-file':home.write_bytes(b'target file')
      elif case=='target-alias':home.symlink_to('../sibling',target_is_directory=True)
      else:
       home.mkdir();(home/'server.json').write_text('{"id":"demo","name":"Demo","version_id":"fake"}')
       os.link(peer,home/'opaque.bin')
     before=snapshot(root);sibling_before=snapshot(root/'sibling');st=peer.stat()
     assert not list(root.rglob('server.pid'))
     assert not any(q.is_file() and q.stat().st_mode&0o111 for q in root.rglob('*'))
     method='server.get' if command=='show' else 'server.delete'
     if mode=='rpc':
      args=['rpc'];data=json.dumps({'protocol':1,'id':'root','method':method,'params':{'directory':directory,'id':'demo'}}).encode()+b'\n'
     else:args=['server',command,'demo','--servers-directory',directory];data=None
     result=subprocess.run([str(binary),*args],input=data,cwd=root,capture_output=True,timeout=15)
     after=snapshot(root);expected_delete=native and command=='delete' and case in ['normal','ancestor-alias']
     if expected_delete:assert not home.exists()
     if native and case=='missing-root':
      assert set(after)-set(before)=={'missing','missing/nested'}
      assert not list((root/'missing/nested').iterdir())
      for name in before:
       if name!='.':assert after[name]==before[name]
     elif not expected_delete:assert after==before,(case,mode,command,'unexpected disk change')
     assert snapshot(root/'sibling')==sibling_before
     now=peer.stat();assert peer.read_bytes()==b'owned inert bytes'
     assert (now.st_ino,now.st_mode,now.st_mtime_ns)==(st.st_ino,st.st_mode,st.st_mtime_ns)
     assert now.st_nlink==st.st_nlink-int(expected_delete)
     assert not list(root.rglob('server.pid'))
     if native and case in ['normal','ancestor-alias']:
      assert result.returncode==0 and not result.stderr
      if mode=='rpc':
       value=json.loads(result.stdout)['result'];assert value['id']=='demo'
       if command=='delete':assert value['deleted'] is True
       else:assert value['name']=='Demo' and value['installed'] is False
      elif command=='delete':assert result.stdout==b'deleted\tdemo\n'
     elif not native:
      if mode=='rpc':assert json.loads(result.stdout)['error']['code']=='UNSUPPORTED_PLATFORM'
      else:assert (result.returncode,result.stdout,result.stderr)==(1,b'',b'Server management is currently supported only on Linux\n')
     else:
      if mode=='rpc':assert 'error' in json.loads(result.stdout)
      else:assert result.returncode!=0 and not result.stdout
     logical={name:(value[0].hex() if isinstance(value[0],bytes) else value[0],value[2]) for name,value in after.items()}
     outputs.append((result.returncode,result.stdout.hex(),result.stderr.hex(),logical))
   assert outputs[0]==outputs[1],(case,command,mode,outputs)
   count+=1
print(f'PASS: {count} exact server root show/delete RPC/CLI cases; precise creation/preservation and no-state controls')
