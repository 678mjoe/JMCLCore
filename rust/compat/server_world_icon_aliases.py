#!/usr/bin/env python3
import base64,hashlib,json,os,stat,subprocess,tempfile
from pathlib import Path
import argparse,sys
p=argparse.ArgumentParser()
for key in ["zig","rust","oracle-source"]:p.add_argument("--"+key,required=True)
a=p.parse_args()
assert subprocess.check_output(["git","-C",a.oracle_source,"rev-parse","HEAD"],text=True).strip()=="5a4310debbcee26b3d2391bab2339f79cd6ec52e"
base=Path(tempfile.mkdtemp(prefix='owned-server-world-alias-'))
bins=[Path(a.zig).resolve(),Path(a.rust).resolve()]
print('binary SHA256:',{str(b):hashlib.sha256(b.read_bytes()).hexdigest() for b in bins})
if sys.platform!='linux':
 request=b'{"protocol":1,"id":"alias","method":"server.worlds.get","params":{"directory":"unused-owned-root","id":"demo","world":"Alpha"}}\n'
 reports=[]
 for binary in bins:
  p=subprocess.run([str(binary),'rpc'],input=request,capture_output=True,timeout=15)
  assert p.returncode==0 and not p.stderr
  assert json.loads(p.stdout)['error']['code']=='UNSUPPORTED_PLATFORM'
  reports.append(p.stdout)
 assert reports[0]==reports[1]
 print('PASS: platform rejection; named aliases not exercised off Linux')
 sys.exit(0)
def snap():
 result={}
 for p in [base,*base.rglob('*')]:
  s=p.lstat();result[str(p.relative_to(base))]=(p.read_bytes().hex() if stat.S_ISREG(s.st_mode) else None,os.readlink(p) if stat.S_ISLNK(s.st_mode) else None,s.st_mode,s.st_ino,s.st_nlink,s.st_mtime_ns)
 return result
records=[]
for case in ['regular','empty','missing','maximum','oversize','icon-alias','broken-icon','icon-directory','world-alias','world-alias-missing','world-and-icon-alias','broken-world','world-file']:
 fixture=base/case;server=fixture/'servers/demo';server.mkdir(parents=True);outside=fixture/'outside';outside.mkdir()
 (server/'server.json').write_bytes(b'{"id":"demo","name":"Synthetic","version_id":"1.20"}');(server/'server.properties').write_bytes(b'level-name=Alpha\n')
 (outside/'peer').write_bytes(b'inert');os.link(outside/'peer',outside/'sibling');(outside/'peer').chmod(0o444)
 world=server/'Alpha'
 if case=='broken-world':world.symlink_to(outside/'missing',target_is_directory=True)
 elif case=='world-file':world.write_bytes(b'inert')
 elif case.startswith('world-'):
  target=outside/'world';target.mkdir();world.symlink_to(target,target_is_directory=True);world=target
 else:world.mkdir()
 expected=b'opaque\x00icon'
 if case in ('missing','world-alias-missing','broken-icon','icon-directory','oversize','broken-world','world-file'):expected=None
 elif case=='empty':expected=b''
 elif case=='maximum':expected=b'x'*65536
 if case in ('icon-alias','world-and-icon-alias'):
  (outside/'icon').write_bytes(expected);(outside/'icon').chmod(0o444);os.link(outside/'icon',outside/'icon-peer');(world/'icon.png').symlink_to(outside/'icon')
 elif case=='broken-icon':(world/'icon.png').symlink_to(outside/'absent')
 elif case=='icon-directory':(world/'icon.png').mkdir()
 elif case=='oversize':(world/'icon.png').write_bytes(b'x'*65537)
 elif expected is not None:(world/'icon.png').write_bytes(expected)
 before=snap();reports=[]
 request=(json.dumps({'protocol':1,'id':'alias','method':'server.worlds.get','params':{'directory':str(fixture/'servers'),'id':'demo','world':'Alpha'}})+'\n').encode()
 for i,b in enumerate(bins):
  p=subprocess.run([str(b),'rpc'],input=request,capture_output=True,timeout=15);assert p.returncode==0 and not p.stderr and snap()==before
  result=json.loads(p.stdout)
  if i==0:
   if case in ('broken-world','world-file'):
    assert result['error']['code']==('WORLD_NOT_FOUND' if case=='broken-world' else 'WORLD_GET_FAILED')
   else:
    info=result['result']['world'];assert info['name']=='Alpha' and info['locked']==False and info['level_name'] is None
    assert info['has_icon']==(case not in ('missing','world-alias-missing','broken-icon'))
    assert result['result']['icon_png_base64']==(base64.b64encode(expected).decode() if expected is not None else None)
  reports.append({'status':p.returncode,'stdout_hex':p.stdout.hex()})
 records.append({'case':case,'equal':reports[0]==reports[1],'reports':reports})

print(len(records),'cases;',sum(not r['equal'] for r in records),'differences')
for r in records:
 if not r['equal']:print(r['case'],[bytes.fromhex(p['stdout_hex']).decode() for p in r['reports']])

assert all(r['equal'] for r in records), 'strict alias/icon response mismatch (see records above)'
print('PASS: 13 server get alias/icon cases; all owned targets and peers unchanged')
