from itertools import product
from pathlib import Path
import subprocess,json,hashlib,random,argparse
parser=argparse.ArgumentParser()
parser.add_argument("binary_directory",type=Path)
root=parser.parse_args().binary_directory.resolve()
alphabet=b'a=:\\\n\r\t #!\x00\xff'
cases=[bytes(x) for n in range(5) for x in product(alphabet,repeat=n)]
tokens=[b'',b'key',b'\\n',b'\\r',b'\\u0000',b'\\uD800',b'\\uDC00',b'\\uD800\\uDC00',b'\\uZZZZ',b'\\u00ff',b'\xff',b'\\',b'\\\\',b'\\ key',b'\xc3\xa9',b'k'*256,b'k'*257]
cases += [a+b'='+b+b'\n' for a in tokens for b in tokens]
rng=random.Random(20261105)
cases += [bytes(rng.choice(alphabet+b'utf01289ABCDEF') for _ in range(rng.randrange(1,81))) for _ in range(10000)]
outputs=[]
for name in ['oracle','candidate']:
 lines=[]
 for i in range(0,len(cases),1000):
  p=subprocess.run([str(root/name)]+[x.hex() for x in cases[i:i+1000]],check=True,capture_output=True,text=True,timeout=30)
  assert not p.stderr,p.stderr
  lines.extend(p.stdout.splitlines())
 assert len(lines)==len(cases),(name,len(lines),len(cases))
 outputs.append(lines)
diffs=[(v.hex(),a,b) for v,a,b in zip(cases,*outputs) if a!=b]
result={'cases':len(cases),'differences':len(diffs),'examples':diffs[:30],'scope':'in-memory properties parser only; Rust source adapter removes only serde derive; Zig imports pinned original module','sha256':{name:hashlib.sha256((root/name).read_bytes()).hexdigest() for name in ['oracle','candidate','properties.rs']}}
(root/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result,indent=2))

assert not diffs, diffs[:20]
