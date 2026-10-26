#!/usr/bin/env python3
"""Existing-root lists and absent selectors only; no registered-file mutations."""
from pathlib import Path
import json,subprocess,tempfile,hashlib
import argparse
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
args=parser.parse_args()
assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins={'zig':Path(args.zig).resolve(),'rust':Path(args.rust).resolve()}
def snap(root):
 return {str(p.relative_to(root)):(p.lstat().st_ino,p.lstat().st_mode,p.lstat().st_mtime_ns,p.lstat().st_nlink,p.read_bytes().hex() if p.is_file() else None) for p in [root,*root.rglob('*')]}
rows=[]
with tempfile.TemporaryDirectory(prefix='owned-field-parser-read-') as td:
 root=Path(td);(root/'saves/world/datapacks').mkdir(parents=True);(root/'sibling').write_bytes(b'unchanged');before=snap(root)
 tokens=['1','1e400','-1e400','1e-400','{"nested":1e400}','[1e400]','null','true','"world"','"\\ud800"','{"x":"\\ud800"}','{"same":1,"same":2}','01']
 for method in [f'{family}.{op}' for family in ['mods','resourcepacks','shaderpacks','datapacks'] for op in ['list','enable','disable','remove']]:
  for field in ['future','project','file','world','directory']:
   for token in tokens:
    parts=['"directory":'+json.dumps(td),'"world":"world"']
    if not method.endswith('.list') and field!='file':parts.append('"file":'+json.dumps(method.split('.')[0]+'/absent.jar'))
    if field=='directory':parts=parts[1:]
    if field=='world':parts=[p for p in parts if not p.startswith('"world":')]
    parts.append(json.dumps(field)+':'+token)
    wire='{"protocol":1,"id":"t","method":'+json.dumps(method)+',"params":{'+','.join(parts)+'}}\n'
    row={'method':method,'field':field,'token':token}
    for engine,exe in bins.items():
     p=subprocess.run([str(exe),'rpc'],input=wire.encode(),capture_output=True,check=True,timeout=15);assert not p.stderr
     row[engine]=json.loads(p.stdout);assert snap(root)==before,(row,engine,'mutated')
    rows.append(row)
 # Outer duplicate rejection is a protocol gate, not local last-key decoding.
 for method in ['mods.list','datapacks.list','mods.disable','datapacks.remove']:
  for params in ['{"directory":'+json.dumps(td)+',"directory":'+json.dumps(td)+'}',
                 '{"directory":'+json.dumps(td)+',"future":1,"future":2}',
                 '{"directory":'+json.dumps(td)+',"future":{"x":1,"x":2}}',
                 '[]','null','{}']:
   wire='{"protocol":1,"id":"t","method":'+json.dumps(method)+',"params":'+params+'}\n'
   row={'method':method,'boundary_params':params.replace(td,'<OWNED_ROOT>')}
   for engine,exe in bins.items():
    p=subprocess.run([str(exe),'rpc'],input=wire.encode(),capture_output=True,check=True,timeout=15);assert not p.stderr
    row[engine]=json.loads(p.stdout);assert snap(root)==before,(row,engine,'mutated')
   if '"future":1,"future":2' in params or '"x":1,"x":2' in params or params.count('"directory"')==2:
    assert row['zig']['error']['code']=='INVALID_REQUEST'
   rows.append(row)
diffs=[r for r in rows if r['zig']!=r['rust']]
for r in diffs:print('MISMATCH:',json.dumps(r,sort_keys=True))
print(len(rows),'exact local parser RPC cases; disk snapshots unchanged;',len(diffs),'mismatches')
assert not diffs
