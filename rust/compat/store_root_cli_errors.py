#!/usr/bin/env python3
"""Eight previously failing CLI diagnostic sequences; owned explicit CAS only."""
import argparse,json,subprocess,tempfile
from pathlib import Path
p=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:p.add_argument('--'+key,required=True)
a=p.parse_args()
assert subprocess.check_output(['git','-C',a.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
def snap(t):
 return {str(x.relative_to(t)):(x.stat().st_dev,x.stat().st_ino,x.stat().st_mode,x.stat().st_mtime_ns,x.stat().st_nlink,x.read_bytes().hex() if x.is_file() else None) for x in [t,*t.rglob('*')]}
import sys
if not sys.platform.startswith('linux'):
 print('NOT EXERCISED: server CLI diagnostic surface is Linux-only; direct root API tests remain native')
 raise SystemExit(0)
rows=[]
for family in ['mods','datapacks']:
 for name in ['Name',[78,97,109,101]]:
  for blocked_root in [False,True]:
   row={'family':family,'name':name,'blocked_server_root':blocked_root}
   for engine,exe in [('zig',a.zig),('rust',a.rust)]:
    with tempfile.TemporaryDirectory(prefix='owned-store-root-error-') as td:
     t=Path(td);root=t/'servers';server=root/'demo';server.mkdir(parents=True)
     (server/'server.json').write_text(json.dumps({'id':'demo','name':name,'version_id':'1.20.1'}))
     blocked=t/'blocked';blocked.write_bytes(b'owned sentinel');before=snap(t);results=[]
     for _ in range(2):
      o=subprocess.run([str(Path(exe).resolve()),'server',family,'adopt','demo','--servers-directory',str(blocked if blocked_root else root),'--store-directory',str(blocked)],capture_output=True,timeout=15)
      results.append({'code':o.returncode,'out':o.stdout.decode(),'err':o.stderr.decode()})
      assert snap(t)==before,(row,engine,'unexpected disk change')
      assert not (server/'.jmcl/server.pid').exists()
     row[engine]=results;row[engine+'_snapshot']=snap(t)
   rows.append(row)
expected={'code':1,'out':'','err':'failed to open artifact store: NotDir\n'}
failed=[r for r in rows if r['zig']!=[expected]*2 or r['rust']!=[expected]*2]
for row in failed:print(json.dumps(row))
print(len(rows),'diagnostic sequences /',len(rows)*2,'CLI pairs;',len(failed),'mismatches; exact snapshots unchanged')
assert not failed
