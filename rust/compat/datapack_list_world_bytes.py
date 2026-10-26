#!/usr/bin/env python3
"""Owned existing-world read fixtures only; no creation, locks or mutations."""
from pathlib import Path
import os,json,subprocess,tempfile,hashlib,errno
import argparse
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
args=parser.parse_args()
assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins={'zig':Path(args.zig).resolve(),'rust':Path(args.rust).resolve()}
def snap(root):
 return {str(p.relative_to(root)):(p.lstat().st_dev,p.lstat().st_ino,p.lstat().st_mode,p.lstat().st_mtime_ns,p.lstat().st_nlink,p.read_bytes().hex() if p.is_file() else None) for p in [root,*root.rglob('*')]}
def native_bytes(p):return str(p).encode('utf-8','surrogatepass') if os.name=='nt' else os.fsencode(p)
rows=[]
with tempfile.TemporaryDirectory(prefix='owned-datapack-world-read-') as td:
 t=Path(td);game=t/'game';(game/'saves').mkdir(parents=True)
 (t/'no-saves').mkdir();(t/'file-saves').mkdir();(t/'file-saves/saves').write_bytes(b'not directory');(t/'blocked').write_bytes(b'not root')
 (t/'sibling').write_bytes(b'unchanged');os.link(t/'sibling',t/'peer')
 names=['World','世界',os.fsdecode(b'raw-\xff') if os.name!='nt' else 'raw-\ud800']
 for name in names[:]:
  p=game/'saves'/name
  try:p.mkdir()
  except OSError as e:
   if (os.name!='nt' and e.errno==errno.EILSEQ) or (os.name=='nt' and getattr(e,'winerror',None)==123):
    print('CAPABILITY:',repr(e));names.remove(name);continue
   raise
  (p/'datapacks').mkdir();(p/'datapacks/inert.zip').write_bytes(b'opaque bytes, not archive')
 (game/'saves/file-world').write_bytes(b'not world')
 tokens={};positive={}
 for name in names:
  if not any(0xd800<=ord(c)<=0xdfff for c in name):tokens[name+'_string']=json.dumps(name)
  tokens[name+'_array']=json.dumps(list(native_bytes(name)))
  try:expected=native_bytes(name).decode('utf-8')
  except UnicodeDecodeError:expected=list(native_bytes(name))
  positive[name+'_array']=expected
  if name+'_string' in tokens:positive[name+'_string']=name
 for name in ['', '.', '..','../sibling','a/b','a\\b','.hidden','trailing.','trailing ','control\x01','missing','file-world','a'*255,'a'*256]:
  tokens[name+'_string']=json.dumps(name);tokens[name+'_array']=json.dumps(list(name.encode()))
 tokens.update({'overflow':'[256]','negative':'[-1]','fraction':'[87.5]','integral':'[87.0,111,114,108,100]','quoted_integral':'["87",111,114,108,100]','rounded':'[87.0000000000000001,111,114,108,100]','quoted_fraction':'["87.0000000000000001",111,114,108,100]','bool':'true','null':'null','object':'{}','array_object':'[{}]','nul':'[0]','nonUTF8_missing':'[254]'})
 positive.update({'integral':'World','quoted_integral':'World','rounded':'World'})
 before=snap(t)
 for root in [game,t/'no-saves',t/'file-saves',t/'blocked']:
  for form in ['string','array']:
   directory=json.dumps(str(root) if form=='string' else list(native_bytes(root)))
   for label,token in tokens.items():
    req='{"protocol":1,"id":"t","method":"datapacks.list","params":{"directory":'+directory+',"world":'+token+'}}\n';row={'root':root.name,'directory_form':form,'world':label}
    for engine,exe in bins.items():
     p=subprocess.run([str(exe),'rpc'],input=req.encode(),capture_output=True,timeout=15,check=True);assert not p.stderr
     row[engine]=json.loads(p.stdout);assert snap(t)==before,(row,engine,'changed')
    if root==game and label in positive:
     assert row['zig']['result']=={'stage':'datapacks.list','world':positive[label],'entries':[],'unmanaged':['datapacks/inert.zip']}
    rows.append(row)
diffs=[r for r in rows if r['zig']!=r['rust']]
print(len(rows),'paired world LIST cases;',len(diffs),'mismatches; complete snapshots unchanged')
for r in diffs:print(json.dumps(r))
assert not diffs
