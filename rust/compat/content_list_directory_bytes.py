#!/usr/bin/env python3
"""Client LIST only, owned metadata fixtures; no artifacts or acquisition."""
from pathlib import Path
import tempfile,subprocess,json,os,argparse,errno
parser=argparse.ArgumentParser()
for key in ['zig','rust','oracle-source']:parser.add_argument('--'+key,required=True)
args=parser.parse_args()
assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
bins={'zig':Path(args.zig).resolve(),'rust':Path(args.rust).resolve()}
def native_bytes(p):
 return str(p).encode('utf-8','surrogatepass') if os.name=='nt' else os.fsencode(p)
def snap(root):
 out={}
 for p in [root,*root.rglob('*')]:
  s=p.lstat();out[str(p.relative_to(root))]=(p.read_bytes().hex() if p.is_file() else None,s.st_dev,s.st_ino,s.st_mode,s.st_mtime_ns,s.st_nlink)
 return out
rows=[]
with tempfile.TemporaryDirectory(prefix='owned-content-list-dir-') as td:
 root=Path(td);(root/'sibling').write_bytes(b'owned unchanged');os.link(root/'sibling',root/'peer');(root/'blocked').write_bytes(b'not a directory')
 paths={}
 for label,name in [('ascii','game'),('unicode','目录'),('raw_native','raw-\ud800' if os.name=='nt' else os.fsdecode(b'raw-\xff'))]:
  p=root/name
  try:p.mkdir()
  except OSError as e:
   if label=='raw_native' and e.errno in {errno.EINVAL,errno.EILSEQ,errno.ENOTSUP}:
    print('CAPABILITY',label,'unsupported',e.errno);continue
   raise
  paths[label]=p
  for d in ['mods','resourcepacks','shaderpacks','saves/world/datapacks']:(p/d).mkdir(parents=True)
  (p/'sentinel').write_bytes(b'owned inert')
 def check(method,label,token,world='world'):
  params='{"directory":'+token+(',"world":'+json.dumps(world) if method=='datapacks.list' else '')+'}'
  wire='{"protocol":1,"id":"t","method":'+json.dumps(method)+',"params":'+params+'}\n'
  before=snap(root);row=dict(method=method,label=label,world=world)
  for name,exe in bins.items():
   o=subprocess.run([str(exe),'rpc'],input=wire.encode(),capture_output=True,check=True,timeout=15);assert not o.stderr;row[name]=json.loads(o.stdout);assert snap(root)==before,(name,label,'disk changed')
  rows.append(row)
 for method in ['mods.list','resourcepacks.list','shaderpacks.list','datapacks.list']:
  for label,p in paths.items():
   if label!='raw_native':check(method,label+'_string',json.dumps(str(p)))
   check(method,label+'_array',json.dumps(list(native_bytes(p))))
  for blocked in [False,True]:
   b=list(native_bytes(root/'blocked' if blocked else paths['ascii']));tail=','.join(map(str,b[1:]))
   first=b[0]
   tokens={'ascii':json.dumps(b),'empty':'[]','nul':'[0]',
    'fraction':f'[{first}.5,'+tail+']',
    'rounded':f'[{first}.0000000000000001,'+tail+']',
    'quoted_fraction':f'["{first}.0000000000000001",'+tail+']',
    'integral':f'[{first}.0,'+tail+']',
    'quoted':f'["+{first//10}__{first%10}",'+tail+']',
    'overflow':'[256]','negative':'[-1]','nested':'[[47]]','null_element':'[null]','bool_element':'[true]','null':'null','object':'{}'}
   for label,token in tokens.items():check(method,label+('_blocked' if blocked else ''),token)
  if method=='datapacks.list':
   for world in ['', 'bad/name','missing']:
    for p in [paths['ascii'],root/'blocked']:
     for form in [json.dumps(str(p)),json.dumps(list(native_bytes(p))), '[]']:
      check(method,'world-order-'+p.name,form,world)
for row in rows:assert row['zig']==row['rust'],row
print('PASS:',len(rows),'exact existing-root client list cases; full disk identity snapshots unchanged')

# Separately permit only source-defined empty-root creation in owned parents.
rows=[]
with tempfile.TemporaryDirectory(prefix='owned-list-root-creation-') as td:
 for method in ['mods.list','resourcepacks.list','shaderpacks.list','datapacks.list']:
  for form in ['string','array','empty_array']:
   for world in (['world',''] if method=='datapacks.list' else ['world']):
    row={'method':method,'form':form,'world':world}
    for engine,exe in bins.items():
     t=Path(td)/str(len(rows))/engine;t.mkdir(parents=True);sentinel=t/'sentinel';sentinel.write_bytes(b'owned preserve');os.link(sentinel,t/'peer');ss=sentinel.stat();target=t/'new-root';d=str(target) if form=='string' else list(native_bytes(target)) if form=='array' else []
     p={'directory':d};
     if method=='datapacks.list':p['world']=world
     req=json.dumps({'protocol':1,'id':'t','method':method,'params':p})+'\n';o=subprocess.run([str(exe),'rpc'],input=req.encode(),capture_output=True,check=True,timeout=15);assert not o.stderr
     st=sentinel.stat();assert (ss.st_ino,ss.st_dev,ss.st_mode,ss.st_mtime_ns,ss.st_nlink)==(st.st_ino,st.st_dev,st.st_mode,st.st_mtime_ns,st.st_nlink);assert sentinel.read_bytes()==b'owned preserve'
     created=target.is_dir();assert not created or list(target.iterdir())==[];assert {x.name for x in t.iterdir()}==({'sentinel','peer','new-root'} if created else {'sentinel','peer'})
     row[engine]={'response':json.loads(o.stdout),'created_empty_root':created}
    rows.append(row)
for row in rows:assert row['zig']==row['rust'],row
print('PASS:',len(rows),'exact empty-root creation/range cases; sibling peer preserved')
