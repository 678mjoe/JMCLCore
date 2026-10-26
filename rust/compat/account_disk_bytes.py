#!/usr/bin/env python3
"""Synthetic metadata only; no save/delete/auth requests or credentials."""
import hashlib,json,os,pathlib,subprocess,tempfile
import argparse
parser=argparse.ArgumentParser(description="Read-only synthetic account disk byte-array differential")
parser.add_argument("--zig",required=True)
parser.add_argument("--rust",required=True)
parser.add_argument("--oracle-source",required=True)
args=parser.parse_args()
zig=pathlib.Path(args.zig).resolve()
rust=pathlib.Path(args.rust).resolve()
revision=subprocess.check_output(["git","-C",args.oracle_source,"rev-parse","HEAD"],text=True).strip()
assert revision=="5a4310debbcee26b3d2391bab2339f79cd6ec52e",revision
account={'schema_version':1,'id':'0123456789abcdef0123456789abcdef','player_name':'Player_1','client_id':'00001111-aaaa-2222-bbbb-3333cccc4444','xuid':'123456'}
def encode(obj):return json.dumps(obj,separators=(',',':'))
cases=[('ordinary',encode(account))]
for field in ['id','player_name','client_id','xuid']:
    d=dict(account);d[field]=list(d[field].encode());cases.append((field+'-ascii-array',encode(d)))
for label,token in [('quoted-byte','"80"'),('float-byte','80.0'),('precise-fraction','80.0000000000000001'),('typed-rounds-to-80','80.00000000000000000000000000000000000001'),('overflow','256'),('negative','-1'),('bool','true'),('null','null'),('object','{}'),('nested','[]'),('nonutf8','255'),('zero','0'),('bad-name-ascii','47')]:
    d=dict(account);d['player_name']='PLACEHOLDER';doc=encode(d).replace('"PLACEHOLDER"','['+token+',108,97,121,101,114,95,49]');cases.append((label,doc))
for label,value in [('empty',[]),('none',None),('mixed-typed',['49',50.0,51]),('invalid-nonutf8',[255]),('invalid-punctuation',[47])]:
    d=dict(account);d['xuid']=value;cases.append(('xuid-'+label,encode(d)))
d=dict(account);d['ignored']={'any':[255,None,{},'1']};cases.append(('unknown-ignored',encode(d)))
requests=''.join(encode({'protocol':1,'id':str(i),'method':'account.microsoft.'+method,'params':params})+'\n' for i,(method,params) in enumerate([('get',{'id':account['id']}),('list',{})]))
def snap(root):
    result={}
    for p in [root,*root.rglob('*')]:
        st=p.stat()
        result[str(p.relative_to(root))]=(p.read_bytes() if p.is_file() else None, st.st_ino, st.st_mode, st.st_mtime_ns, st.st_nlink)
    return result

accepted={'ordinary','id-ascii-array','player_name-ascii-array','client_id-ascii-array','xuid-ascii-array','quoted-byte','float-byte','typed-rounds-to-80','xuid-none','xuid-mixed-typed','unknown-ignored'}
for label,doc in [('duplicate-id',encode(account)[:-1]+',"id":"'+account['id']+'"}'),('missing-client',encode({k:v for k,v in account.items() if k!='client_id'})),('array-document','[]'),('id-mismatch',encode(dict(account,id='22222222222222222222222222222222')))]:cases.append((label,doc))
cases.append(('xuid-absent',encode({k:v for k,v in account.items() if k!='xuid'})))
accepted.add('xuid-absent')
rows=[]
with tempfile.TemporaryDirectory(prefix='owned-account-byte-audit-') as td:
    root=pathlib.Path(td);store=root/'accounts';store.mkdir();path=store/(account['id']+'.json');link=root/'preserved-link'
    sibling=dict(account,id='11111111111111111111111111111111',player_name='Sibling');(store/(sibling['id']+'.json')).write_text(encode(sibling))
    for label,doc in cases:
        if link.exists():link.unlink()
        path.write_text(doc);os.link(path,link);before=snap(root);outputs=[]
        for binary in [zig,rust]:
            p=subprocess.run([str(binary),'rpc'],input=requests.encode(),capture_output=True,cwd=root,timeout=20)
            assert p.returncode==0 and not p.stderr,(label,p)
            assert snap(root)==before,(label,'disk mutated',binary)
            parsed=[json.loads(line) for line in p.stdout.splitlines()];assert len(parsed)==2
            assert 'result' in parsed[1], (label,str(binary),p.stdout)
            expected=dict(account)
            if label in ['xuid-none','xuid-absent']:expected['xuid']=None
            if label=='xuid-mixed-typed':expected['xuid']='123'
            if label in accepted:
                assert 'result' in parsed[0],(label,str(binary),parsed)
                assert parsed[0]['result']=={'stage':'account_get','account':expected},(label,str(binary),parsed)
                assert parsed[1]['result']=={'stage':'account_list','accounts':[expected,sibling]},(label,str(binary),parsed)
            else:
                assert parsed[0]['error']=={'code':'ACCOUNT_STORE_ERROR','message':'Stored account metadata is invalid'},(label,str(binary),parsed)
                assert parsed[1]['result']=={'stage':'account_list','accounts':[sibling]},(label,str(binary),parsed)
            outputs.append(p.stdout.decode())
        rows.append({'case':label,'equal':outputs[0]==outputs[1],'zig':outputs[0],'rust':outputs[1]})
assert all(r['equal'] for r in rows),[r for r in rows if not r['equal']]
print(f"PASS: {len(rows)} persisted account byte-string fixtures; get/list only; complete file snapshots unchanged")
for binary in [zig,rust]:print(binary.name,hashlib.sha256(binary.read_bytes()).hexdigest())
