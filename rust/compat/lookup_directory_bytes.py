#!/usr/bin/env python3
"""Read-only native directory byte paths; no lifecycle/Java/archive methods."""
import argparse
import errno
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def snapshot(root):
    out={}
    for p in [root,*root.rglob('*')]:
        st=p.lstat()
        value=os.readlink(p) if p.is_symlink() else p.read_bytes() if p.is_file() else None
        out[str(p.relative_to(root))]=(value,st.st_ino,st.st_mode,st.st_mtime_ns,st.st_nlink)
    return out


def native_name(raw):
    # Windows consumes WTF-8, not POSIX surrogateescape. Invalid sequences must
    # be rejected by the Core; don't manufacture an unrelated physical path.
    return raw.decode('utf-8','surrogatepass') if os.name=='nt' else os.fsdecode(raw)


def directory_alias(link,target):
    if os.name=='nt':
        p=subprocess.run(['cmd','/c','mklink','/J',str(link),str(target)],capture_output=True,timeout=15)
        assert p.returncode==0,(p.returncode,p.stdout,p.stderr)
    else:link.symlink_to(target,target_is_directory=True)


def main():
    parser=argparse.ArgumentParser()
    for arg in ['zig','rust','oracle-source']:parser.add_argument('--'+arg,required=True)
    args=parser.parse_args()
    assert subprocess.check_output(['git','-C',args.oracle_source,'rev-parse','HEAD'],text=True).strip()=='5a4310debbcee26b3d2391bab2339f79cd6ec52e'
    binaries=[Path(args.zig).resolve(),Path(args.rust).resolve()]
    count=0;unsupported=[]
    with tempfile.TemporaryDirectory(prefix='owned-lookup-directories-') as td:
        root=Path(td);(root/'sibling').write_bytes(b'keep');os.link(root/'sibling',root/'sibling-link')
        for kind in ['instance','server']:
            native=kind=='instance' or sys.platform=='linux'
            def record(directory):
                d=directory/'demo';d.mkdir(parents=True);(d/(kind+'.json')).write_text(json.dumps({'id':'demo','name':'Demo','version_id':'fake'}))
            def response(binary,directory_token,method,id_token='"demo"',cwd=root):
                wire='{"protocol":1,"id":"directory","method":"'+kind+'.'+method+'","params":{"directory":'+directory_token+',"id":'+id_token+'}}\n'
                p=subprocess.run([str(binary),'rpc'],input=wire.encode(),cwd=cwd,capture_output=True,timeout=30)
                assert p.returncode==0 and not p.stderr,(kind,method,p)
                return p.stdout,json.loads(p.stdout)
            def semantic(value,method,outcome):
                if not native:assert value['error']['code']=='UNSUPPORTED_PLATFORM';return
                if outcome=='ok':
                    v=value['result'];items=v.get(kind+'s',[v]);assert len(items)==1 and items[0]['id']=='demo' and items[0]['name']=='Demo' and items[0]['installed'] is False;return
                word='lookup' if method=='get' else 'listing'
                if outcome=='root':error={'code':kind.upper()+'S_ROOT_ERROR','message':f'Failed to open the {kind}s root'}
                elif outcome=='not-found':error={'code':kind.upper()+'_NOT_FOUND','message':f'No {kind} matches the given id'}
                else:error={'code':'INVALID_PARAMS','message':f'{kind} {word} parameters are invalid' if outcome=='parse' else f'{kind} lookup parameters are out of range' if method=='get' else f'{kind} listing directory must not be empty'}
                assert value['error']==error,(kind,method,outcome,value)
            def check(label,token,outcome,id_token='"demo"',methods=('get','list')):
                nonlocal count
                for method in methods:
                    before=snapshot(root);outputs=[]
                    for binary in binaries:
                        out,v=response(binary,token,method,id_token);semantic(v,method,outcome);outputs.append(out)
                        assert snapshot(root)==before,(kind,label,binary,'disk changed')
                    assert outputs[0]==outputs[1],(kind,label,method,outputs);count+=1
            names=[('ascii',b'plain'),('unicode','世界'.encode()),('opaque',b'raw-\xff'),('surrogate',b'raw-\xed\xa0\x80'),('overlong',b'raw-\xc0\xaf'),('truncated',b'raw-\xe2\x82'),('bad-continuation',b'raw-\xe2A')]
            for label,suffix in names:
                raw=kind.encode()+b'-'+suffix
                outcome='ok'
                try:
                    directory=root/native_name(raw)
                    directory.mkdir()
                except UnicodeDecodeError:
                    assert os.name=='nt';outcome='root';unsupported.append((kind,label,'invalid WTF-8'))
                except OSError as e:
                    assert e.errno==errno.EILSEQ,(label,e)
                    outcome='root';unsupported.append((kind,label,'filesystem EILSEQ'))
                else:record(directory)
                check(label,json.dumps(list(raw)),outcome)
                if label in ['ascii','unicode']:
                    check(label+'-string',json.dumps(raw.decode()),'ok')
            plain=(kind+'-plain').encode()
            # First byte i/s is exactly representable after dynamic f64 rounding;
            # the equivalent quoted fractional lexeme must not round to an integer.
            first=plain[0]
            suffix=','.join(map(str,plain[1:]))
            check('dynamic-rounded',f'[{first}.000000000000001,{suffix}]','ok')
            check('quoted-fraction',f'["{first}.000000000000001",{suffix}]','parse')
            check('typed-string-elements',json.dumps([str(x) for x in plain]),'ok')
            check('empty-array','[]','range');check('empty-string','""','range')
            for label,token in [('overflow','[256]'),('negative','[-1]'),('fraction','[100.5]'),('bool','[true]'),('null-element','[null]'),('wrong-shape','{}')]:check(label,token,'parse')
            check('nul',json.dumps(list((kind+'-nul').encode()+b'\0tail')),'root')
            (root/(kind+'-blocked')).write_bytes(b'file')
            check('blocked',json.dumps(list((kind+'-blocked').encode())),'root')
            check('root-before-id',json.dumps(list((kind+'-blocked').encode())),'root','[255]',('get',))
            check('empty-id-before-root',json.dumps(list((kind+'-blocked').encode())),'range','[]',('get',))
            check('parse-before-empty-id','[256]','parse','[]',('get',))
            check('empty-directory-before-id','[]','range','[255]',('get',))
            check('opaque-path-empty-id','[255]','range','[]',('get',))
            check('nul-path-empty-id','[0]','range','[]',('get',))
            # Existing configured-root aliases: final entry forbidden, ancestor
            # permitted. All targets and records remain inside the owned fixture.
            target=root/(kind+'-alias-target');target.mkdir();record(target)
            alias=root/(kind+'-alias');directory_alias(alias,target)
            check('final-root-alias',json.dumps(list((kind+'-alias').encode())),'root')
            nested=target/'nested';nested.mkdir();record(nested)
            check('ancestor-alias',json.dumps(list((kind+'-alias/nested').encode())),'ok')
            for method in ['get','list']:
                for params in ['["'+kind+'-plain"]',json.dumps([list((kind+'-plain').encode())])]:
                    wire='{"protocol":1,"id":"shape","method":"'+kind+'.'+method+'","params":'+params+'}\n'
                    before=snapshot(root);outputs=[]
                    for binary in binaries:
                        p=subprocess.run([str(binary),'rpc'],input=wire.encode(),cwd=root,capture_output=True,timeout=30)
                        assert p.returncode==0 and not p.stderr and snapshot(root)==before
                        semantic(json.loads(p.stdout),method,'parse');outputs.append(p.stdout)
                    assert outputs[0]==outputs[1];count+=1
            for method in ['get','list']:
                for label,id_token,creates in [('valid','"demo"',True),('opaque','[255]',True),('empty','[]',method=='list')]:
                    outputs=[]
                    for index,binary in enumerate(binaries):
                        cwd=root/(kind+'-new-'+method+'-'+label+'-'+str(index));cwd.mkdir();(cwd/'sibling').write_bytes(b'keep');before=snapshot(cwd)
                        out,v=response(binary,json.dumps(list(b'new-root')),method,id_token,cwd);outputs.append(out)
                        after=snapshot(cwd)
                        if not native:assert v['error']['code']=='UNSUPPORTED_PLATFORM' and after==before
                        elif creates:
                            if method=='list':assert v['result'][kind+'s']==[]
                            else:semantic(v,method,'not-found')
                            assert (cwd/'new-root').is_dir() and not list((cwd/'new-root').iterdir())
                            assert set(after)==set(before)|{'new-root'} and after['sibling']==before['sibling'] and after['.'][1:3]==before['.'][1:3]
                        else:semantic(v,method,'range');assert after==before
                    assert outputs[0]==outputs[1];count+=1
    for kind,label,reason in unsupported:
        outcome='root-error' if kind=='instance' or sys.platform=='linux' else 'unsupported-platform'
        print(f'NOTE: {kind} {label} physical raw-name success unavailable ({reason}); exact {outcome} response compared, ordinary Unicode controls retained')
    print(f'PASS: {count} exact native-directory read cases; platform capability outcomes, aliases, ordering and owned disk effects checked')

if __name__=='__main__':main()
