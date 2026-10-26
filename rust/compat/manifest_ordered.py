from pathlib import Path
import json,subprocess,tempfile,hashlib,collections,argparse,sys
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
parser.add_argument('--output')
parser.add_argument('--oversized-server-only',action='store_true',help='Separate known CLI read-limit diagnostic debt; strict equality, never normalized')
options=parser.parse_args()
assert subprocess.check_output(['git','-C',options.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins={'zig':Path(options.zig).resolve(),'rust':Path(options.rust).resolve()}
cases={
'enum_eof':b'{"source":"bad",','enum_illegal':b'{"source":"bad",!',
'enum_bad_unknown':br'{"source":"bad","future":"\ud800"}',
'bad_unknown_enum':br'{"future":"\ud800","source":"bad"}',
'bad_unknown_valid':br'{"future":"\ud800","source":"official"}',
'enum_bad_known':br'{"source":"bad","name":"\ud800"}',
'bad_known_enum':br'{"name":"\ud800","source":"bad"}',
'enum_unknown_nested':br'{"source":"bad","future":{"a":"\ud800"}}',
'unknown_nested_enum':br'{"future":{"a":"\ud800"},"source":"bad"}',
'enum_truncated_string':b'{"source":"bad","future":"oops',
'enum_missing_colon':b'{"source":"bad","future" 1}',
'enum_bad_number':b'{"source":"bad","future":01}',
'bad_number_enum':b'{"future":01,"source":"bad"}',
'overflow_eof':b'{"schema_version":4294967296,',
'wrong_type_eof':b'{"id":true,',
'duplicate_eof':b'{"name":"A","name":"B",',
'valid_enum_eof':b'{"source":"official",',
'eof_enum_token':b'{"source":"bad',
'root_array':b'[]','root_null':b'null',
'enum_trailing':b'{"source":"bad"}!',
'missing_trailing':b'{}!',
'semantic_trailing':b'{"id":"demo","name":"","version_id":"fake"}!',
'valid_ignored_huge':b'{"id":"demo","name":"Demo","version_id":"fake","future":1e400}',
'valid_ignored_nested_huge':b'{"id":"demo","name":"Demo","version_id":"fake","future":{"a":[1e400]}}',
'enum_overflow':b'{"source":"bad","schema_version":4294967296}',
'overflow_enum':b'{"schema_version":4294967296,"source":"bad"}',
'valid':b'{"id":"demo","name":"Demo","version_id":"fake"}',
'byte_name':b'{"id":"demo","name":[255],"version_id":"fake"}',
}
# The source caps the entire manifest and scanner value at 64 KiB. Test both
# boundaries rather than importing a provider cap or inventing a nesting policy.
valid=cases['valid']
cases['exact_64k']=valid+b' '*(65536-len(valid))
cases['over_64k']=valid+b' '*(65537-len(valid))
cases['long_ignored_number']=valid[:-1]+b',"future":'+b'9'*65000+b'}'
for depth in [128,129,256]:
 cases['nested_'+str(depth)]=valid[:-1]+b',"future":'+b'['*depth+b'0'+b']'*depth+b'}'
cases['escaped_source_key']=br'{"s\u006furce":"bad",'
cases['duplicate_escaped_name']=br'{"name":"A","na\u006de":"B",'
expected_errors={
 'enum_eof':'InvalidEnumTag','enum_illegal':'InvalidEnumTag','enum_bad_unknown':'InvalidEnumTag',
 'bad_unknown_enum':'SyntaxError','bad_unknown_valid':'SyntaxError','enum_bad_known':'InvalidEnumTag',
 'bad_known_enum':'SyntaxError','enum_unknown_nested':'InvalidEnumTag','unknown_nested_enum':'SyntaxError',
 'enum_truncated_string':'InvalidEnumTag','enum_missing_colon':'InvalidEnumTag','enum_bad_number':'InvalidEnumTag',
 'bad_number_enum':'SyntaxError','overflow_eof':'Overflow','wrong_type_eof':'UnexpectedToken',
 'duplicate_eof':'DuplicateField','valid_enum_eof':'UnexpectedEndOfInput','eof_enum_token':'UnexpectedEndOfInput',
 'root_array':'UnexpectedToken','root_null':'UnexpectedToken','enum_trailing':'InvalidEnumTag',
 'missing_trailing':'MissingField','semantic_trailing':'SyntaxError','enum_overflow':'InvalidEnumTag',
 'overflow_enum':'Overflow','escaped_source_key':'InvalidEnumTag','duplicate_escaped_name':'DuplicateField',
}
def snap(t):return {str(p.relative_to(t)):[p.lstat().st_dev,p.lstat().st_ino,p.lstat().st_mode,p.lstat().st_mtime_ns,p.lstat().st_nlink,p.read_bytes().hex() if p.is_file() else None] for p in [t,*t.rglob('*')]}
rows=[];violations=[]
with tempfile.TemporaryDirectory(prefix='owned-manifest-order-') as temp:
 for kind in ['instance','server']:
  for label,body in cases.items():
   oversized=kind=='server' and label=='over_64k'
   if oversized!=options.oversized_server_only:continue
   row={'kind':kind,'case':label,'body_hex':body.hex(),'diagnostics':{}}
   for engine,exe in bins.items():
    t=Path(temp)/str(len(rows))/engine;d=t/(kind+'s')/'demo';d.mkdir(parents=True);(d/(kind+'.json')).write_bytes(body);(t/'sentinel').write_bytes(b'unchanged')
    before=snap(t);p=subprocess.run([str(exe),kind,'show','demo','--json'],cwd=t,capture_output=True,timeout=15);after=snap(t)
    row[engine]={'code':p.returncode,'out':p.stdout.hex(),'err':p.stderr.hex()};row['diagnostics'][engine]={'before':before,'after':after}
    if before!=after:violations.append([kind,label,engine])
    if kind=='server' and not sys.platform.startswith('linux'):
     if (p.returncode,p.stdout,p.stderr)!=(1,b'',b'Server management is currently supported only on Linux\n'):violations.append([kind,label,engine,'platform gate'])
    elif label in expected_errors or label=='over_64k':
     err=expected_errors.get(label,'InstanceManifestTooLarge' if kind=='instance' else 'ServerManifestTooLarge')
     if (p.returncode,p.stdout,p.stderr)!=(1,b'',f'failed to load {kind}: {err}\n'.encode()):violations.append([kind,label,engine,'exact expected error'])
    else:
     if p.returncode!=0 or p.stderr:violations.append([kind,label,engine,'expected success'])
     else:
      value=json.loads(p.stdout)[kind]
      if value['id']!='demo' or value['name']!=([255] if label=='byte_name' else 'Demo') or value['version_id']!='fake':violations.append([kind,label,engine,'positive fields'])

   rows.append(row)
if options.output:Path(options.output).write_text(json.dumps(rows,indent=2))
print('Scope:', 'separate oversized-server CLI diagnostic' if options.oversized_server_only else 'ordered manifest schema; excludes exactly server over_64k CLI debt')
print(len(rows),'show-only pairs',sum(r['zig']!=r['rust'] for r in rows),'differences',len(violations),'snapshot violations')
for r in rows:
 if r['kind']=='instance':print(r['case'],*[engine+': '+str(r[engine]['code'])+' '+bytes.fromhex(r[engine]['err']).decode().strip() for engine in bins])
for r in rows:
 if r["zig"]!=r["rust"]:print(json.dumps(r))
assert not violations
assert all(r["zig"]==r["rust"] for r in rows)
