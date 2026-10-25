import json,subprocess,pathlib,hashlib
import argparse
parser=argparse.ArgumentParser(description="Bounded offline raw protocol edge comparison; no normalization")
parser.add_argument("--zig",type=pathlib.Path,required=True)
parser.add_argument("--rust",type=pathlib.Path,required=True)
args=parser.parse_args()
zig=args.zig.resolve();rust=args.rust.resolve()
cases=[]
def req(p='1',params='',method='ping',extra='',id='"edge"'):
 return ('{"protocol":'+p+',"id":'+id+',"method":"'+method+'"'+params+extra+'}').encode()
nums=['1','1.0','1e0','1.0000000000000001','1.0000000000000000000000000000000001','0.99999999999999999','4294967295','4294967296','4294967295.00000001','18446744073709551615','18446744073709551616','1e309','1e9999','1e-9999','-0','-0.0','+1','01','1_0','1__0','1_','_1','1e+0','1_0.0','0x1',' 1','1 ','NaN','inf']
for n in nums:
 for quoted in [False,True]:cases.append(('protocol '+repr(n)+(' string' if quoted else ''),req(json.dumps(n) if quoted else n)))
for raw in ['null','[]','1','"x"','{}','{"x":1,"x":2}','{"x":1,"\\u0078":2}','{"x":"\\ud800"}','{"x":"\\udc00"}','{"x":"\\ud800\\udc00"}','{"x":1e9999}']:
 cases.append(('params '+raw,req(params=',"params":'+raw)))
for extra in [',"unknown":"\\ud800"',',"unknown":1e9999',',"unknown":1,"unknown":2',',"unknown":{"x":1,"x":2}',',"\\u0070rotocol":1']:
 cases.append(('extra '+extra,req(extra=extra)))
for method in ['instance.list','account.microsoft.list','store.gc','install.plan','launch.plan','worlds.list','java.detect','version.get','server.list']:
 for param in ['',',"params":null',',"params":[]',',"params":{}',',"params":{"unused":1e9999}']:
  cases.append((method+param,req(method=method,params=param)))
for p,id in [('2','""'),('1.5','""'),('2','null')]:cases.append(('precedence '+p+id,req(p,id=id)))
for number in ['1.0000000000000001','"1.0000000000000001"','"1_0"','"1__0"','"1_0.0"','18446744073709551615','"18446744073709551615.0"','18446744073709551616','1e9999','1e-9999']:
 cases.append(('dynamic store.gc '+number,req(method='store.gc',params=',"params":{"store_directory":"","min_age_seconds":'+number+'}')))
for method,field in [('store.gc','store_directory'),('instance.list','directory'),('account.microsoft.list','directory')]:
 for value in ['1e9999','{"x":1,"x":2}','null']:
  cases.append(('unknown field '+method+' '+value,req(method=method,params=',"params":{"'+field+'":"","unused":'+value+'}')))
# Read-only loading of isolated synthetic disk manifests; both binaries see the
# same fixture paths. No instance/server is launched or modified by either core.
import tempfile
fixtures = tempfile.TemporaryDirectory(prefix="jmcl-numeric-manifests-")
for kind in ['instance','server']:
 for i, number in enumerate(['1.0000000000000001','"1.0000000000000001"','"1_0"','1e9999','1','"1.00000000000000000000000000000000001"']):
  directory = pathlib.Path(fixtures.name)/kind/str(i)
  leaf = directory/'demo'
  leaf.mkdir(parents=True)
  (leaf/(kind+'.json')).write_text('{"schema_version":'+number+',"id":"demo","name":"Demo","version_id":"1.0","unused":1e9999}')
  params = json.dumps({'directory':str(directory),'id':'demo'},separators=(',',':'))
  cases.append(('disk '+kind+' '+number,req(method=kind+'.get',params=',"params":'+params)))
results=[]
for name,data in cases:
 def run(bin):
  p=subprocess.run([str(bin),'rpc'],input=data+b'\n',capture_output=True,timeout=5,cwd=None)
  return [p.returncode,p.stdout.decode(errors='backslashreplace'),p.stderr.decode(errors='backslashreplace')]
 a,b=run(zig),run(rust); results.append(dict(name=name,request=data.decode(),zig=a,rust=b,equal=a==b))
print('TOTAL',len(results),'DIFF',sum(not x['equal'] for x in results))
for x in results:
 if not x['equal']:print(json.dumps(x))

print('Rust sha256',hashlib.sha256(rust.read_bytes()).hexdigest())

raise SystemExit(0 if all(x["equal"] for x in results) else 1)
