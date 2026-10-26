#!/usr/bin/env python3
import os,sys,json,stat,hashlib,subprocess,tempfile,gzip,struct,argparse,errno
from pathlib import Path
p=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:p.add_argument('--'+key,required=True)
a=p.parse_args()
assert subprocess.check_output(['git','-C',a.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
base=Path(tempfile.mkdtemp(prefix='owned-world-cli-native-'))
bins=[Path(a.zig).resolve(),Path(a.rust).resolve()]
print('binary SHA256:', {str(b):hashlib.sha256(b.read_bytes()).hexdigest() for b in bins})
raw='raw-\ud800' if os.name=='nt' else os.fsdecode(b'raw-\xff')
def snap():
 return {str(p.relative_to(base)):(p.read_bytes().hex() if stat.S_ISREG(s.st_mode) else None,os.readlink(p) if stat.S_ISLNK(s.st_mode) else None,s.st_mode,s.st_ino,s.st_nlink,s.st_mtime_ns) for p in [base,*base.rglob('*')] for s in [p.lstat()]}
def text(b):return struct.pack('>H',len(b))+b
def tag(t,n,v):return bytes([t])+text(n)+v
level=gzip.compress(tag(10,b'',tag(10,b'Data',tag(8,b'LevelName',text(b'Synthetic'))+tag(10,b'Version',tag(8,b'Name',text(b'1.20'))+b'\0')+b'\0')+b'\0'),mtime=0)
records=[]
for domain in ['instance','server']:
 for shape in ['plain','unicode','raw-root','raw-world','raw-both']:
  directory=base/(domain+'-'+shape)/(raw if shape in ('raw-root','raw-both') else '目录' if shape=='unicode' else 'roots')
  target=directory/'demo';worldname=raw if shape in ('raw-world','raw-both') else '世界' if shape=='unicode' else 'Alpha'
  saves=target/'.minecraft/saves' if domain=='instance' else target
  world=saves/worldname
  try:world.mkdir(parents=True)
  except OSError as e:
   if e.errno==getattr(errno,'EILSEQ',None) or getattr(e,'winerror',None)==123:
    print('CAPABILITY: physical raw-name success unavailable;',domain,shape,repr(e),'comparing exact outcomes; Unicode controls still required')
    if directory.exists():
     (target/('instance.json' if domain=='instance' else 'server.json')).write_text(json.dumps({'schema_version':1,'id':'demo','name':'Synthetic','version_id':'1.20','source':'official'}))
     if domain=='server':(target/'server.properties').write_bytes(b'level-name=Alpha\n')
    option='--instances-directory' if domain=='instance' else '--servers-directory'
    for cmd in ['list','show']:
     reports=[];before=snap()
     args=[domain,'worlds',cmd,'demo']+([worldname] if cmd=='show' else [])+[option,str(directory),'--json']
     for i,b in enumerate(bins):
      p=subprocess.run([str(b),*args],capture_output=True,timeout=15)
      assert snap()==before,(domain,shape,cmd,'unsupported-name disk change')
      if i==0:assert p.returncode==0 if cmd=='list' and directory.exists() else p.returncode==1
      reports.append({'status':p.returncode,'stdout_hex':p.stdout.hex(),'stderr_hex':p.stderr.hex()})
     records.append({'domain':domain,'shape':shape,'case':cmd+'-unsupported-name','equal':reports[0]==reports[1],'reports':reports})
    continue
   raise
  (world/'level.dat').write_bytes(level)
  (target/('instance.json' if domain=='instance' else 'server.json')).write_text(json.dumps({'schema_version':1,'id':'demo','name':'Synthetic','version_id':'1.20','source':'official'}))
  if domain=='server':(target/'server.properties').write_bytes(('level-name='+('Alpha' if shape in ('raw-world','raw-both') else worldname)+'\n').encode())
  (directory/'blocked').write_bytes(b'inert');(directory/'peer').write_bytes(b'inert');os.link(directory/'peer',directory/'sibling');(directory/'peer').chmod(0o444)
  option='--instances-directory' if domain=='instance' else '--servers-directory'
  cases=[]
  for cmd in ['list','show']:
   pos=[worldname] if cmd=='show' else []
   for mode in [[],['--json']]:cases.append((cmd+('-json' if mode else '-text'),[domain,'worlds',cmd,'demo',*pos,option,str(directory),*mode],True))
  cases += [('repeated-directory',[domain,'worlds','show','demo',worldname,option,str(directory/'blocked'),option,str(directory),'--json'],True),('id-before-root',[domain,'worlds','show','bad/id',worldname,option,str(directory/'blocked')],False),('root-before-name',[domain,'worlds','show','demo','..',option,str(directory/'blocked')],False),('empty-world',[domain,'worlds','show','demo','',option,str(directory)],False),('raw-id',[domain,'worlds','show',raw,worldname,option,str(directory)],False),('extra-position',[domain,'worlds','show','demo',worldname,'extra',option,str(directory)],domain=='server')]
  cases += [
   ('ignored-raw-label',[domain,'worlds','show','demo',worldname,option,str(directory),'--label',raw,'--json'],True),
   ('ignored-replace',[domain,'worlds','show','demo',worldname,option,str(directory),'--replace','--label',raw],True),
   ('consumed-json-as-root',[domain,'worlds','show','demo',worldname,option,str(directory),option,'--json','--label',raw],None),
   ('missing-root-value',[domain,'worlds','show','demo',worldname,option,str(directory),'--label',raw,option],domain=='server'),
   ('third-positional',[domain,'worlds','show','demo',worldname,'two','three',option,str(directory),'--label',raw],False),
   ('empty-root',[domain,'worlds','show','demo',worldname,option,'','--label',raw],False),
   ('list-extra-world',[domain,'worlds','list','demo',worldname,option,str(directory),'--label',raw],False),
   ('invalid-name',[domain,'worlds','show','demo','..',option,str(directory),'--label',raw],False),
  ]
  # The consumed root option is an existing owned directory, never an ambient path.
  (directory/'--json').mkdir()
  for label,args,positive in cases:
   reports=[];before=snap()
   for i,b in enumerate(bins):
    p=subprocess.run([str(b),*args],capture_output=True,timeout=15,cwd=directory);assert snap()==before,(domain,shape,label,'disk changed')
    if i==0:
     assert positive is None or (p.returncode==0)==positive,(domain,shape,label,p.returncode,p.stderr)
     if positive and '--json' in args:
      obj=json.loads(p.stdout)
      if 'world' in obj: assert obj['world']['name']==(list(os.fsencode(worldname)) if shape in ('raw-world','raw-both') else worldname)
      elif domain=='instance':assert len(obj['worlds'])==1
      worlds=[obj['world']] if 'world' in obj else obj['worlds']
      for w in worlds:
       assert w['level_name']=='Synthetic' and w['version_name']=='1.20' and w['version_relation']==('same' if domain=='instance' else 'unknown')
    reports.append({'status':p.returncode,'stdout_hex':p.stdout.hex(),'stderr_hex':p.stderr.hex()})
   records.append({'domain':domain,'shape':shape,'case':label,'equal':reports[0]==reports[1],'reports':reports})
(base/'evidence.json').write_text(json.dumps({'root':str(base),'binary_sha256':[hashlib.sha256(b.read_bytes()).hexdigest() for b in bins],'records':records,'snapshot':snap()},indent=2))
print(len(records),'cases',sum(not r['equal'] for r in records),'differences')
for r in records:
 if not r['equal']:print(r['domain'],r['shape'],r['case'],[p['status'] for p in r['reports']])

assert all(r['equal'] for r in records), 'strict native world CLI mismatch'
print('PASS:',len(records),'world CLI read commands; all disk/peer identities preserved')
