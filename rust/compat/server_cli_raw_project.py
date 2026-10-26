from pathlib import Path
import os,subprocess,tempfile,json,hashlib,collections,argparse,sys
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
parser.add_argument('--output')
options=parser.parse_args()
assert subprocess.check_output(['git','-C',options.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins={'zig':Path(options.zig).resolve(),'rust':Path(options.rust).resolve()}
if not sys.platform.startswith('linux'):
 raw='\ud800' if os.name=='nt' else os.fsdecode(b'\xff')
 with tempfile.TemporaryDirectory(prefix='owned-server-project-platform-') as td:
  for family in ['mods','datapacks']:
   for op in ['list','enable','disable','remove']:
    for exe in bins.values():
     p=subprocess.run([str(exe),'server',family,op,'demo','--project',raw],cwd=td,capture_output=True,timeout=15)
     assert (p.returncode,p.stdout,p.stderr)==(1,b'',b'Server management is currently supported only on Linux\n')
    assert list(Path(td).iterdir())==[]
 print('PASS: eight non-Linux platform gates; no filesystem access')
 raise SystemExit(0)
raw=os.fsdecode(b'\xff'); rows=[]; violations=[]
def expected(family,op,case):
 if op=='list' or case in ['invalid_id_raw','unicode_id_raw','empty_id_raw','invalid_id_text','unicode_id_text','empty_id_text','raw_id_raw','unknown_before','unknown_after','missing_tail','raw_then_empty','consumed_option']:
  return 1,b'error: InvalidArguments\n'
 if case=='both':return 2,f'server {family} {op} requires exactly one of --project or --file\n'.encode()
 if case in ['blocked_root','missing_id']:return 1,b'no server matches the given id\n'
 if 'guard' in case:return 1,b'failed to validate the server lifecycle status\n'
 if family=='datapacks' and case=='bad_properties':return 1,b'server.properties is missing or has no usable level-name (install the server first)\n'
 if case in ['ascii_valid','raw_then_valid']:return 1,f'no registered {"mod" if family=="mods" else "datapack"} matches the given selector\n'.encode()
 prefix='server content operation failed' if family=='mods' else f'server datapacks {op} failed'
 return 1,f'{prefix}: {"MissingField" if case=="bad_manifest" else "InvalidModrinthProject"}\n'.encode()
def snap(t):
 return {str(p.relative_to(t)):[p.lstat().st_dev,p.lstat().st_ino,p.lstat().st_mode,p.lstat().st_mtime_ns,p.lstat().st_nlink,p.read_bytes().hex() if p.is_file() else None] for p in [t,*t.rglob('*')]}
with tempfile.TemporaryDirectory(prefix='owned-server-project-') as temp:
 for family in ['mods','datapacks']:
  for op in ['list','enable','disable','remove']:
   for case in ['raw','unicode','ascii_bad','ascii_valid','invalid_id_raw','unicode_id_raw','empty_id_raw','invalid_id_text','unicode_id_text','empty_id_text','raw_id_raw','blocked_root','missing_id','bad_manifest','guard','bad_properties','guard_bad_properties','both','unknown_before','unknown_after','missing_tail','raw_then_valid','valid_then_raw','raw_then_empty','consumed_option']:
    row={'family':family,'op':op,'case':case,'diagnostics':{}}
    for engine,exe in bins.items():
     t=Path(temp)/str(len(rows))/engine;t.mkdir(parents=True);root=t/'servers';server=root/'demo';server.mkdir(parents=True);(server/'.jmcl').mkdir();guard=server/'.jmcl/lifecycle.lock'
     if 'guard' in case:guard.mkdir()
     else:guard.write_bytes(b'')
     manifest={'id':'demo','name':'Demo','version_id':'1.20.1'}
     if case=='bad_manifest':manifest.pop('version_id')
     (server/'server.json').write_text(json.dumps(manifest));(server/'server.properties').write_text('level-name='+('../bad' if 'bad_properties' in case else 'world')+'\n')
     home=server/'world' if family=='datapacks' else server;(home/family).mkdir(parents=True);(home/'.jmcl').mkdir(exist_ok=True);(home/'.jmcl/content.json').write_text('{"schema_version":1,"entries":[]}')
     inert=home/family/'inert.jar';inert.write_bytes(b'opaque unchanged bytes');peer=t/'peer';os.link(inert,peer);peer.chmod(0o444);blocked=t/'blocked';blocked.write_bytes(b'preserve')
     ident={'invalid_id_raw':'../bad','unicode_id_raw':'服务器','empty_id_raw':'','invalid_id_text':'../bad','unicode_id_text':'服务器','empty_id_text':'','raw_id_raw':raw,'missing_id':'absent'}.get(case,'demo')
     project={'unicode':'项目','ascii_bad':'bad/ref','ascii_valid':'nomatch','invalid_id_text':'nomatch','unicode_id_text':'nomatch','empty_id_text':'nomatch'}.get(case,raw)
     argv=['server',family,op,ident,'--servers-directory',str(blocked if case=='blocked_root' else root)]
     if case=='unknown_before':argv+=['--unknown','x']
     argv+=['--project',project]
     if case=='both':argv+=['--file',family+'/inert.jar']
     if case=='unknown_after':argv+=['--unknown','x']
     if case=='missing_tail':argv+=['--file']
     if case=='raw_then_valid':argv+=['--project','nomatch']
     if case=='valid_then_raw':argv[-1]='nomatch';argv+=['--project',raw]
     if case=='raw_then_empty':argv+=['--project','']
     if case=='consumed_option':argv=['server',family,op,ident,'--servers-directory',str(root),'--file','--project',raw]
     before=snap(t);p=subprocess.run([str(exe),*argv],cwd=t,capture_output=True,timeout=15);after=snap(t)
     row[engine]={'code':p.returncode,'out':p.stdout.hex(),'err':p.stderr.hex()};row['diagnostics'][engine]={'argv_hex':[os.fsencode(a).hex() for a in argv],'before':before,'after':after}
     if before!=after or (server/'.jmcl/server.pid').exists():violations.append([len(rows),engine,'changed filesystem'])
     code,err=expected(family,op,case)
     if (p.returncode,p.stdout,p.stderr)!=(code,b'',err):violations.append([len(rows),engine,'exact expected error',row[engine]])
    rows.append(row)
if options.output:Path(options.output).write_text(json.dumps(rows,indent=2))
diffs=[r for r in rows if r['zig']!=r['rust']];print(len(rows),'pairs',len(diffs),'differences',len(violations),'invariant failures');print(collections.Counter(r['case'] for r in diffs));print(violations)
for r in rows:
 if r['family']=='datapacks' and r['op']=='disable':print(r['case'],r['zig']['code'],bytes.fromhex(r['zig']['err']).decode().strip(),'rust',r['rust']['code'])
for row in diffs:print(json.dumps(row))
assert not violations
assert not diffs
