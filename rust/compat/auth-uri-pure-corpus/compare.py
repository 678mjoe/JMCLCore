import itertools,random,subprocess,json
from pathlib import Path
p=Path(__file__).resolve().parent
hosts=['example.com','localhost','a.','a..b','-a','a-','a_b','a b','%61.com','%61.','%2e','%41.com','%+1','%4_','[::1]','127.0.0.1','user@example.com','user:pass@example.com','', '.'.join(['a'*63]*4), '.'.join(['a'*63]*4)+'.','%61'+'.'.join(['a'*63]*4)[1:],'%61'+'.'.join(['a'*63]*4)[1:]+'.']
ports=['',':443',':4_43',':-0',':+443',':65535',':65536',':_1',':1_',':','::1']
schemes=['https://','http://','HTTPS://','https:','https:///']
suffixes=['','/link','?q=a','#x','/a@b','/x:y']
cases=[a+b+c+d for a,b,c,d in itertools.product(schemes,hosts,ports,suffixes)]
r=random.Random(20261105)
alphabet='abcABC012.-_%:@/?#+_[]'
cases.extend('https://'+''.join(r.choice(alphabet) for _ in range(r.randrange(0,100))) for _ in range(10000))
m=[]
for i in range(0,len(cases),100):
 batch=cases[i:i+100]
 a=subprocess.check_output([str(p/'candidate'),*batch],text=True).splitlines()
 b=subprocess.check_output([str(p/'oracle'),*batch],text=True).splitlines()
 assert len(a)==len(b)==len(batch)
 m.extend({'uri':s,'rust':x,'zig':y} for s,x,y in zip(batch,a,b) if x!=y)
result={'cases':len(cases),'mismatches':m}
(p/'result.json').write_text(json.dumps(result,indent=2))
print(json.dumps({'cases':len(cases),'mismatches':len(m),'first':m[:5]},indent=2))
