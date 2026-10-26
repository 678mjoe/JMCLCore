#!/usr/bin/env python3
"""Read-only lookup-ID bytes, including source-defined root creation effects."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def snapshot(root):
    return {str(p.relative_to(root)): (p.read_bytes() if p.is_file() else None,p.stat().st_ino,p.stat().st_mode,p.stat().st_mtime_ns,p.stat().st_nlink) for p in [root,*root.rglob('*')]}


def main():
    parser=argparse.ArgumentParser()
    for arg in ['zig','rust','oracle-source']: parser.add_argument('--'+arg,required=True)
    args=parser.parse_args()
    assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
    binaries=[Path(args.zig).resolve(),Path(args.rust).resolve()]
    tokens=[
        ('string','"demo"','ok'),('ascii','[100,101,109,111]','ok'),
        ('utf8','[195,169]','not-found'),('opaque','[255]','not-found'),('nul','[0]','not-found'),
        ('upper','[68,101,109,111]','not-found'),('traversal','[46,46,47,120]','not-found'),
        ('overlong',json.dumps([97]*65),'not-found'),
        ('empty','[]','range'),('empty-string','""','range'),
        ('fraction','[100.5,101,109,111]','parse'),
        ('float-rounded','[100.000000000000001,101,109,111]','ok'),
        ('quoted-not-rounded','["100.000000000000001",101,109,111]','parse'),
        ('float-integral','[100.0,101,109,111]','ok'),
        ('underscore','["1__00",101,109,111]','ok'),('plus','["+100",101,109,111]','ok'),
        ('overflow','[256]','parse'),('negative','[-1]','parse'),('nested','[[100]]','parse'),
        ('null-element','[null]','parse'),('bool-element','[true]','parse'),('null','null','parse'),('object','{}','parse'),
    ]
    count=0
    with tempfile.TemporaryDirectory(prefix='owned-lookup-id-bytes-') as td:
        root=Path(td);(root/'blocked').write_bytes(b'not directory');(root/'sibling').write_bytes(b'preserve');os.link(root/'sibling',root/'sibling-link')
        for kind in ['instance','server']:
            d=root/(kind+'s')/'demo';d.mkdir(parents=True);(d/(kind+'.json')).write_text(json.dumps({'id':'demo','name':'Demo','version_id':'fake'}))
            native=kind=='instance' or sys.platform=='linux'
            cases=[]
            for label,token,expected in tokens:
                for blocked in [False,True]:
                    directory='blocked' if blocked else kind+'s'
                    outcome='root' if blocked and expected in ['ok','not-found'] else expected
                    cases.append((label+('-blocked' if blocked else ''),'{"directory":'+json.dumps(directory)+',"id":'+token+'}',outcome))
            for field,token in [('unknown','1e400'),('unknown','1e9999'),('command','{}'),('properties','[]'),('java_paths','true'),('grace_ms','{}'),('max_bytes','null'),('cursor','[]'),('file_id','"bad"'),('version_id','false'),('source','[]')]:
                cases.append(('ignored-'+field+'-'+token,'{"directory":'+json.dumps(kind+'s')+',"id":[100,101,109,111],"'+field+'":'+token+'}','ok'))
            cases += [('missing-id','{"directory":'+json.dumps(kind+'s')+'}','parse'),('missing-directory','{"id":[100]}','parse'),('sequence','["'+kind+'s","demo"]','parse'),('empty-directory','{"directory":"","id":[100]}','range'),('bad-directory','{"directory":false,"id":[100]}','parse')]

            def execute(binary,params,cwd):
                wire='{"protocol":1,"id":"lookup","method":"'+kind+'.get","params":'+params+'}\n'
                p=subprocess.run([str(binary),'rpc'],input=wire.encode(),cwd=cwd,capture_output=True,timeout=30)
                assert p.returncode==0 and not p.stderr,(kind,binary,p)
                return p.stdout,json.loads(p.stdout)

            def semantic(value,outcome):
                if not native: assert value['error']['code']=='UNSUPPORTED_PLATFORM';return
                if outcome=='ok':
                    v=value['result'];assert v['id']=='demo' and v['name']=='Demo' and v['installed'] is False
                    return
                expected={
                    'parse':('INVALID_PARAMS',f'{kind} lookup parameters are invalid'),
                    'range':('INVALID_PARAMS',f'{kind} lookup parameters are out of range'),
                    'not-found':(kind.upper()+'_NOT_FOUND',f'No {kind} matches the given id'),
                    'root':(kind.upper()+'S_ROOT_ERROR',f'Failed to open the {kind}s root'),
                }[outcome]
                assert value['error']==dict(zip(['code','message'],expected)),(kind,outcome,value)

            for label,params,outcome in cases:
                before=snapshot(root);outputs=[]
                for binary in binaries:
                    out,v=execute(binary,params,root);semantic(v,outcome);outputs.append(out)
                    assert snapshot(root)==before,(kind,label,'disk changed')
                assert outputs[0]==outputs[1],(kind,label,outputs);count+=1
            # Unknown id values on list remain ignored. This does not change the
            # separate known server.list large-number/Value parsing limitation.
            for id_token in ['[255]','[256]','{}']:
                wire='{"protocol":1,"id":"list","method":"'+kind+'.list","params":{"directory":'+json.dumps(kind+'s')+',"id":'+id_token+'}}\n'
                before=snapshot(root);outputs=[]
                for binary in binaries:
                    p=subprocess.run([str(binary),'rpc'],input=wire.encode(),cwd=root,capture_output=True,timeout=30)
                    assert p.returncode==0 and not p.stderr and snapshot(root)==before
                    v=json.loads(p.stdout)
                    if native: assert [x['id'] for x in v['result'][kind+'s']]==['demo']
                    else: assert v['error']['code']=='UNSUPPORTED_PLATFORM'
                    outputs.append(p.stdout)
                assert outputs[0]==outputs[1];count+=1
            # The outer RPC parser rejects duplicate fields before either
            # typed/raw lookup view runs, including reversed invalid values.
            duplicate_fields = [
                '"id":"demo","id":null', '"id":null,"id":"demo"',
                '"id":[256],"id":[100,101,109,111]',
                '"id":[100,101,109,111],"id":[256]',
                '"directory":false,"directory":"servers"',
                '"directory":"servers","directory":false',
                '"id":"demo","id":"other","java_paths":false',
                '"java_paths":false,"id":"demo","id":"other"',
                '"id":"demo","id":"other","unknown":1e400',
                '"id":"demo","id":"other","unknown":!',
            ]
            for method in ['get','list']:
                for fields in duplicate_fields:
                    prefix = '' if '"directory"' in fields else '"directory":'+json.dumps(kind+'s')+','
                    wire='{"protocol":1,"id":"duplicate","method":"'+kind+'.'+method+'","params":{'+prefix+fields+'}}\n'
                    before=snapshot(root);outputs=[]
                    for binary in binaries:
                        p=subprocess.run([str(binary),'rpc'],input=wire.encode(),cwd=root,capture_output=True,timeout=30)
                        assert p.returncode==0 and not p.stderr and snapshot(root)==before
                        assert json.loads(p.stdout)['error']['code']=='INVALID_REQUEST'
                        outputs.append(p.stdout)
                    assert outputs[0]==outputs[1];count+=1
            # get is a read API, but baseline openRoot creates a missing directory.
            # Verify this precise side effect only in isolated disposable fixtures.
            for label,token,outcome,creates in [('valid','[100,101,109,111]','not-found',True),('opaque','[255]','not-found',True),('empty','[]','range',False),('overflow','[256]','parse',False)]:
                outputs=[]
                for index,binary in enumerate(binaries):
                    cwd=root/(kind+'-new-'+label+'-'+str(index));cwd.mkdir();(cwd/'sibling').write_bytes(b'unchanged')
                    before=snapshot(cwd);out,v=execute(binary,'{"directory":"new-root","id":'+token+'}',cwd);semantic(v,outcome);outputs.append(out)
                    new=cwd/'new-root';assert new.exists()==(native and creates)
                    if new.exists(): assert new.is_dir() and list(new.iterdir())==[]
                    after=snapshot(cwd)
                    assert after['sibling']==before['sibling'] and set(after)==set(before)|({'new-root'} if native and creates else set())
                    if not (native and creates): assert after==before
                    else: assert after['.'][1:3]==before['.'][1:3]
                assert outputs[0]==outputs[1];count+=1
    print(f'PASS: {count} exact lookup ID/ignored-field/root-effect cases; owned metadata only; no lifecycle or Java calls')

if __name__=='__main__':main()
