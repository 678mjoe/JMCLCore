from pathlib import Path
import os,json,subprocess,tempfile,hashlib,collections
import argparse,sys
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
parser.add_argument('--output')
options=parser.parse_args()
assert subprocess.check_output(['git','-C',options.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins={'zig':Path(options.zig).resolve(),'rust':Path(options.rust).resolve()}
if not sys.platform.startswith('linux'):
 raw='\ud800' if os.name=='nt' else os.fsdecode(b'\xff')
 with tempfile.TemporaryDirectory(prefix='owned-server-raw-id-platform-') as td:
  for family in ['mods','datapacks']:
   for op in ['list','enable','disable','remove']:
    observed=[]
    for exe in bins.values():
     p=subprocess.run([str(exe),'server',family,op,raw,'--project',raw],capture_output=True,timeout=15,cwd=td)
     observed.append((p.returncode,p.stdout,p.stderr))
    assert observed[0]==observed[1]==(1,b'',b'Server management is currently supported only on Linux\n')
    assert list(Path(td).iterdir())==[]
 print('PASS: eight unchanged non-Linux rejection gates; no path access')
 raise SystemExit(0)
ids=[('ff',os.fsdecode(b'\xff')),('raw_tail',os.fsdecode(b'demo\xff')),('wtf8',os.fsdecode(b'\xed\xa0\x80')),('overlong',os.fsdecode(b'\xc0\xaf')),('unicode','服务器'),('invalid_ascii','../bad'),('empty','')]
rows=[];violations=[]
def snap(t):return {str(p.relative_to(t)):(p.lstat().st_dev,p.lstat().st_ino,p.lstat().st_mode,p.lstat().st_mtime_ns,p.lstat().st_nlink,p.read_bytes().hex() if p.is_file() else None) for p in [t,*t.rglob('*')]}
expected={'code':1,'out':'','err':b'error: InvalidArguments\n'.hex()}
with tempfile.TemporaryDirectory(prefix='owned-server-raw-id-') as td:
 for family in ['mods','datapacks']:
  for op in ['list','enable','disable','remove']:
   for label,ident in ids:
    for case in ['normal','blocked_root','missing_selector','unknown','missing_value','both_selectors']+(['raw_project','raw_unknown'] if label in ['ff','raw_tail','wtf8','overlong'] else []):
     row={'family':family,'op':op,'id_case':label,'tail_case':case,'diagnostics':{}}
     for engine,exe in bins.items():
      t=Path(td)/str(len(rows))/engine;t.mkdir(parents=True);blocked=t/'blocked';blocked.write_bytes(b'preserve');peer=t/'peer';os.link(blocked,peer);peer.chmod(0o444)
      root=t/'must-not-open';existing=t/'existing';(existing/'.jmcl/lifecycle.lock').mkdir(parents=True)
      args=['server',family,op,ident,'--servers-directory',str(blocked if case=='blocked_root' else root)]
      if case!='missing_selector':args+=['--file',family+'/a.jar']
      if case=='unknown':args+=['--unknown','value']
      if case=='missing_value':args+=['--servers-directory']
      if case=='both_selectors':args+=['--project','project']
      if case=='raw_project':args+=['--project',os.fsdecode(b'\xff')]
      if case=='raw_unknown':args+=[os.fsdecode(b'--\xff')]
      before=snap(t);p=subprocess.run([str(exe),*args],capture_output=True,timeout=15,cwd=t)
      result={'code':p.returncode,'out':p.stdout.hex(),'err':p.stderr.hex()};row[engine]=result
      row['diagnostics'][engine]={'argv_hex':[os.fsencode(x).hex() for x in args],'before':before,'after':snap(t)}
      if snap(t)!=before or root.exists() or (existing/'.jmcl/server.pid').exists():violations.append((len(rows),engine,'unexpected path/state effect'))
      if result!=expected:violations.append((len(rows),engine,'expected early InvalidArguments',result))
     rows.append(row)
if options.output:Path(options.output).write_text(json.dumps(rows,indent=2))
diffs=[r for r in rows if r['zig']!=r['rust']]
print(len(rows),'exact CLI pairs;',len(diffs),'differences;',len(violations),'invariant violations')
print(collections.Counter(r['id_case'] for r in diffs))
print('violations',violations[:10]);assert not violations
for row in diffs:print(json.dumps(row))
assert not diffs
