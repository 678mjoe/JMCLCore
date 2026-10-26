#!/usr/bin/env python3
"""Only portable server world list/show root-opening contracts."""
import argparse,hashlib,json,os,stat,subprocess,tempfile
from pathlib import Path
p=argparse.ArgumentParser()
for k in ['zig','rust','oracle-source']:p.add_argument('--'+k,required=True)
a=p.parse_args()
assert subprocess.check_output(['git','-C',a.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins=[Path(a.zig).resolve(),Path(a.rust).resolve()]
print('binary SHA256:',{str(b):hashlib.sha256(b.read_bytes()).hexdigest() for b in bins})
def snapshot(base):
 r={}
 for f in [base,*base.rglob('*')]:
  s=f.lstat();r[str(f.relative_to(base))]={'mode':s.st_mode,'ino':s.st_ino,'nlink':s.st_nlink,'mtime':s.st_mtime_ns,'bytes':f.read_bytes().hex() if stat.S_ISREG(s.st_mode) else None,'link':os.readlink(f) if f.is_symlink() else None}
 return r
base=Path(tempfile.mkdtemp(prefix='owned-server-world-root-'))
cases=['existing','missing','regular-file','ancestor-file','empty']
if os.name=='posix':cases+=['final-alias','ancestor-alias']
results=[]
for case in cases:
 for command in ['list','show']:
  for json_mode in [False,True]:
   reports=[]
   for index,binary in enumerate(bins):
    fixture=base/f'{case}-{command}-{json_mode}-{index}';fixture.mkdir()
    ordinary=fixture/'ordinary';(ordinary/'demo').mkdir(parents=True)
    (ordinary/'demo/server.json').write_bytes(b'{"id":"demo","name":"Synthetic","version_id":"1.20"}')
    (fixture/'file').write_bytes(b'inert');os.link(fixture/'file',fixture/'peer');(fixture/'file').chmod(0o444)
    root=ordinary
    if case=='missing':root=fixture/'missing'
    elif case=='regular-file':root=fixture/'file'
    elif case=='ancestor-file':root=fixture/'file'/'child'
    elif case=='final-alias':root=fixture/'alias';root.symlink_to(ordinary,target_is_directory=True)
    elif case=='ancestor-alias':
     (fixture/'alias').symlink_to(fixture,target_is_directory=True);root=fixture/'alias'/'ordinary'
    before=snapshot(fixture)
    args=['server','worlds',command,'demo']+(['Alpha'] if command=='show' else [])+['--servers-directory','' if case=='empty' else str(root)]+(['--json'] if json_mode else [])
    out=subprocess.run([str(binary),*args],capture_output=True,timeout=15);after=snapshot(fixture)
    if case=='missing':
     assert set(after)==set(before)|{'missing'} and (fixture/'missing').is_dir()
     # Only parent directory mtime may change when its missing child is created.
     for name,item in before.items():
      actual=after[name].copy();expected=item.copy()
      if name=='.':
       actual.pop('mtime');expected.pop('mtime')
       if os.name=='posix':assert actual['nlink']==expected['nlink']+1
       actual.pop('nlink');expected.pop('nlink')
      assert actual==expected
    else:assert after==before
    if case in ('existing','ancestor-alias') and command=='list':
     assert out.returncode==0 and out.stderr==b''
     assert out.stdout==(b'{"worlds":[]}\n' if json_mode else b'')
    else:assert out.returncode==1 and out.stdout==b'' and out.stderr
    if case=='regular-file':assert out.stderr==b'failed to open servers directory: NotDir\n'
    reports.append({'status':out.returncode,'stdout':out.stdout.hex(),'stderr':out.stderr.hex()})
   assert reports[0]==reports[1],(case,command,json_mode,reports)
   results.append({'case':case,'command':command,'json':json_mode,'reports':reports})
(base/'evidence.json').write_text(json.dumps(results,indent=2))
print(f'PASS: {len(results)} exact server world list/show root commands; no lifecycle and all disk effects checked')
