import subprocess,json,pathlib
import argparse
parser=argparse.ArgumentParser()
parser.add_argument('--oracle',type=pathlib.Path,required=True)
parser.add_argument('--output',type=pathlib.Path,required=True)
args=parser.parse_args()
nums=['0','-0','1','1.0','1e0','1.0000000000000001','1.0000000000000000000000000000000001','1.00000000000000000000000000000000001','0.99999999999999999','0.99999999999999999999999999999999999','1e309','1e9999','1e-9999','-1e-9999','0e99999999999999999999999999999999','1e-9999999999999999999999999999999999','255','256','255.0','255.000000000000000000000000000000001','4294967295','4294967296','4294967295.00000001','4294967295.0','18446744073709551615','18446744073709551616','18446744073709551615.0','18446744073709551615.00000000000000001','18446744073709551616.0','9007199254740993.0','9007199254740993.5']
from decimal import Decimal, localcontext
with localcontext() as context:
 context.prec=200
 for scale in [112,113,114,115]:
  for sign in [-1,1]: nums.append(str(Decimal(1)+sign*(Decimal(2)**(-scale))))
strings=['+1','01','1_0','1__0','1_','_1','1e+0','1_0.0','1__0.0','1e','1e+','1.e0','.0','1.',' 1','1 ','0x1','0x1.0','0X1.0p0','0x1.ep1','0x1.0p','0x1.0p+','0x1_0.0','0x1__0.0','0x1.e_p1','NaN','nan','inf','infinity','-1','-0_0','-1x']
raws=[n for n in nums]+[json.dumps(n) for n in nums+strings]+['true','null','{}','[]']
out=[]
for width in [8,32,64]:
 for raw in raws:
  p=subprocess.run([str(args.oracle.resolve()),raw,str(width)],capture_output=True,timeout=5)
  if p.returncode:raise Exception((raw,p.stderr.decode()))
  out.append(dict(width=width,raw=raw,expected=json.loads(p.stdout)))
p=args.output;p.write_text(json.dumps(out,indent=2)+'\n');print(len(out),'fixtures')
