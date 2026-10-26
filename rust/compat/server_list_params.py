#!/usr/bin/env python3
"""Directory-only server listing parameters; ignored huge numbers stay ignored."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def snapshot(root):
    return {str(p.relative_to(root)):(p.read_bytes() if p.is_file() else None,p.stat().st_ino,p.stat().st_mode,p.stat().st_mtime_ns,p.stat().st_nlink) for p in [root,*root.rglob('*')]}


def main():
    parser=argparse.ArgumentParser()
    for arg in ['zig','rust','oracle-source']:parser.add_argument('--'+arg,required=True)
    args=parser.parse_args()
    assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
    binaries=[Path(args.zig).resolve(),Path(args.rust).resolve()]
    cases=[('ordinary','{"directory":"servers"}','ok')]
    for field,value in [('unknown','1e400'),('unknown','1e9999'),('unknown','{"nested":[1e400]}'),('id','1e400'),('id','{}'),('command','1e400'),('properties','1e400'),('java_paths','1e400'),('grace_ms','1e400'),('max_bytes','1e400'),('cursor','1e400'),('file_id','1e400'),('source','1e400')]:
        cases.append(('ignored-'+field+'-'+value,'{"directory":"servers","'+field+'":'+value+'}','ok'))
    for label,value in [('bool','true'),('null','null'),('object','{}'),('huge','1e400')]:cases.append(('directory-'+label,'{"directory":'+value+',"unknown":1e400}','invalid'))
    cases += [('empty','{"directory":"","unknown":1e400}','empty'),('missing','{"unknown":1e400}','invalid'),('blocked','{"directory":"blocked","unknown":1e400}','root'),('sequence','["servers"]','invalid'),('scalar','1e400','invalid')]
    count=0
    with tempfile.TemporaryDirectory(prefix='owned-server-list-') as td:
        root=Path(td)
        for id in ['zebra','alpha']:
            d=root/'servers'/id;d.mkdir(parents=True);(d/'server.json').write_text(json.dumps({'id':id,'name':id.title(),'version_id':'fake'}))
        (root/'blocked').write_bytes(b'file');(root/'sibling').write_bytes(b'preserve');os.link(root/'sibling',root/'sibling-link')
        before=snapshot(root)
        for label,params,expected in cases:
            wire='{"protocol":1,"id":"listing","method":"server.list","params":'+params+'}\n'
            outputs=[]
            for binary in binaries:
                p=subprocess.run([str(binary),'rpc'],input=wire.encode(),cwd=root,capture_output=True,timeout=30)
                assert p.returncode==0 and not p.stderr and snapshot(root)==before
                v=json.loads(p.stdout)
                if sys.platform!='linux':assert v['error']['code']=='UNSUPPORTED_PLATFORM'
                elif expected=='ok':
                    assert [d['id'] for d in v['result']['servers']]==['alpha','zebra']
                    assert [d['name'] for d in v['result']['servers']]==['Alpha','Zebra']
                    assert all(d['installed'] is False for d in v['result']['servers'])
                else:
                    code,message={'invalid':('INVALID_PARAMS','server listing parameters are invalid'),'empty':('INVALID_PARAMS','server listing directory must not be empty'),'root':('SERVERS_ROOT_ERROR','Failed to open the servers root')}[expected]
                    assert v['error']=={'code':code,'message':message},(label,binary,v)
                outputs.append(p.stdout)
            assert outputs[0]==outputs[1],(label,outputs);count+=1
        outputs=[]
        for index,binary in enumerate(binaries):
            cwd=root/('missing-'+str(index));cwd.mkdir();(cwd/'sibling').write_bytes(b'preserve');before=snapshot(cwd)
            wire=b'{"protocol":1,"id":"listing","method":"server.list","params":{"directory":"new-root","unknown":1e400}}\n'
            p=subprocess.run([str(binary),'rpc'],input=wire,cwd=cwd,capture_output=True,timeout=30)
            assert p.returncode==0 and not p.stderr
            v=json.loads(p.stdout);after=snapshot(cwd)
            if sys.platform=='linux':
                assert v['result']['servers']==[] and (cwd/'new-root').is_dir() and not list((cwd/'new-root').iterdir())
                assert set(after)==set(before)|{'new-root'} and after['sibling']==before['sibling'] and after['.'][1:3]==before['.'][1:3]
            else:assert v['error']['code']=='UNSUPPORTED_PLATFORM' and after==before
            outputs.append(p.stdout)
        assert outputs[0]==outputs[1];count+=1
    print(f'PASS: {count} exact directory-only server list cases; ignored large numbers, error ordering and owned disk effects')

if __name__=='__main__':main()
