#!/usr/bin/env python3
import gzip,struct,os,stat,json,hashlib,subprocess,tempfile,argparse
from pathlib import Path
p=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:p.add_argument('--'+key,required=True)
a=p.parse_args()
assert subprocess.check_output(['git','-C',a.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
root=Path(tempfile.mkdtemp(prefix='owned-world-cli-alias-'))
bins=[Path(a.zig).resolve(),Path(a.rust).resolve()]
print('binary SHA256:',{str(b):hashlib.sha256(b.read_bytes()).hexdigest() for b in bins})
raw='ignored-\ud800' if os.name=='nt' else os.fsdecode(b'ignored-\xff')
def text(b):return struct.pack('>H',len(b))+b
def tag(t,n,v):return bytes([t])+text(n)+v
level=gzip.compress(tag(10,b'',tag(10,b'Data',tag(8,b'LevelName',text(b'Synthetic'))+tag(10,b'Version',tag(8,b'Name',text(b'1.20'))+b'\0')+b'\0')+b'\0'),mtime=0)
def snap():
 return {str(p.relative_to(root)):(p.read_bytes().hex() if stat.S_ISREG(s.st_mode) else None,os.readlink(p) if stat.S_ISLNK(s.st_mode) else None,s.st_mode,s.st_ino,s.st_nlink,s.st_mtime_ns) for p in [root,*root.rglob('*')] for s in [p.lstat()]}
records=[]
alias_supported=True
if os.name=='nt':
 (root/'alias-capability-target').mkdir()
 try:(root/'alias-capability-probe').symlink_to(root/'alias-capability-target',target_is_directory=True)
 except OSError as e:
  if getattr(e,'winerror',None)!=1314:raise
  alias_supported=False
  print('CAPABILITY: Windows symlink privilege unavailable; physical alias cases excluded, ordinary metadata controls required')
for domain in ['instance','server']:
 for case in ['regular','world-alias','level-alias','both-alias','missing-level','broken-level','directory-level','malformed-level','wrong-data','empty-data','all-null','broken-world','file-world']:
  if not alias_supported and case in ['world-alias','level-alias','both-alias','broken-level','broken-world']:continue
  base=root/domain/case;directory=base/'roots';target=directory/'demo';saves=target/'.minecraft/saves' if domain=='instance' else target;saves.mkdir(parents=True);outside=base/'outside';outside.mkdir()
  (target/('instance.json' if domain=='instance' else 'server.json')).write_text(json.dumps({'schema_version':1,'id':'demo','name':'Synthetic','version_id':'1.20','source':'official'}))
  if domain=='server':(target/'server.properties').write_bytes(b'level-name=Alpha\n')
  (outside/'peer').write_bytes(b'inert');os.link(outside/'peer',outside/'sibling');(outside/'peer').chmod(0o444)
  world=saves/'Alpha'
  if case=='broken-world':world.symlink_to(outside/'absent',target_is_directory=True)
  elif case=='file-world':world.write_bytes(b'inert')
  elif case in ['world-alias','both-alias']:
   physical=outside/'world';physical.mkdir();world.symlink_to(physical,target_is_directory=True);world=physical
  else:world.mkdir()
  if case in ['level-alias','both-alias']:
   (outside/'level').write_bytes(level);(outside/'level').chmod(0o444);os.link(outside/'level',outside/'level-peer');(world/'level.dat').symlink_to(outside/'level')
  elif case=='broken-level':(world/'level.dat').symlink_to(outside/'absent')
  elif case=='directory-level':(world/'level.dat').mkdir()
  elif case=='malformed-level':(world/'level.dat').write_bytes(b'not-level-data')
  elif case=='empty-data':(world/'level.dat').write_bytes(gzip.compress(tag(10,b'',tag(10,b'Data',b'\0')+b'\0'),mtime=0))
  elif case=='all-null':(world/'level.dat').write_bytes(gzip.compress(tag(10,b'',tag(10,b'Data',tag(3,b'LevelName',struct.pack('>i',42))+tag(3,b'Version',struct.pack('>i',42))+b'\0')+b'\0'),mtime=0))
  elif case=='wrong-data':(world/'level.dat').write_bytes(gzip.compress(tag(10,b'',tag(8,b'Data',text(b'wrong'))+b'\0'),mtime=0))
  elif case not in ['missing-level','broken-world','file-world']:(world/'level.dat').write_bytes(level)
  for mode in ['text','json']:
   for hook in [False,True]:
    args=[domain,'worlds','show','demo','Alpha','--instances-directory' if domain=='instance' else '--servers-directory',str(directory)]+(['--json'] if mode=='json' else [])+(['--label',raw] if hook else [])
    before=snap();reports=[]
    for i,b in enumerate(bins):
     p=subprocess.run([str(b),*args],capture_output=True,timeout=15);assert snap()==before
     if i==0:
      if case in ['broken-world','file-world']:assert p.returncode==1 and not p.stdout
      else:
       assert p.returncode==0 and not p.stderr
       named=case in ['regular','world-alias','level-alias','both-alias'];meta=named or case in ['empty-data','all-null']
       if mode=='json':assert json.loads(p.stdout)['world']['level_name']==('Synthetic' if named else None)
       else:assert ((b'level_name: Synthetic\n' if named else b'level_name: -\n') in p.stdout)==meta and (b'level.dat: unavailable\n' in p.stdout)==(not meta)
     reports.append({'status':p.returncode,'stdout_hex':p.stdout.hex(),'stderr_hex':p.stderr.hex()})
    records.append({'domain':domain,'case':case,'mode':mode,'hook':hook,'equal':reports[0]==reports[1],'reports':reports})
(root/'evidence.json').write_text(json.dumps({'root':str(root),'records':records,'snapshot':snap()},indent=2))
print(len(records),'cases',sum(not r['equal'] for r in records),'differences')
for r in records:
 if not r['equal']:print(r['domain'],r['case'],r['mode'],r['hook'],[bytes.fromhex(p['stdout_hex']).decode() for p in r['reports']])

assert all(r['equal'] for r in records), 'strict world show alias metadata mismatch'
print('PASS:',len(records),'world show metadata-presence commands; unchanged owned aliases and peers')
