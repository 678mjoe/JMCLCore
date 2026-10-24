#!/usr/bin/env python3
"""Finite raw RPC/CLI GC corpus with real disposable disk fixtures."""
import argparse, hashlib, json, os, subprocess, tempfile
from pathlib import Path

def main():
    p=argparse.ArgumentParser()
    p.add_argument("--zig",required=True,type=Path)
    p.add_argument("--rust",required=True,type=Path)
    a=p.parse_args()
    binaries=[a.zig.resolve(),a.rust.resolve()]
    count=0
    def check(args,data=None,prepare=None):
        nonlocal count
        results=[]
        for binary in binaries:
            with tempfile.TemporaryDirectory() as raw:
                root=Path(raw)
                if prepare:prepare(root)
                r=subprocess.run([str(binary),*args],input=data,cwd=root,capture_output=True,timeout=10)
                results.append((r.returncode,r.stdout,r.stderr))
        assert results[0]==results[1],(args,data,results)
        count+=1
    def objects(root):
        store=root/"cache"
        for data,linked in [(b"eligible",False),(b"linked",True)]:
            digest=hashlib.sha1(data).hexdigest()
            path=store/"objects/sha1"/digest[:2]/digest
            path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)
            if linked:os.link(path,root/"instance-link")
    options=[{},{"store_directory":""},{"store_directory":"missing"},{"store_directory":"cache"},
             {"store_directory":"cache","dry_run":False,"min_age_seconds":0},
             ]
    options += [{"store_directory":"cache","min_age_seconds":v} for v in [0,1,86400,"0","1e2",1.0,-1,1.5,None,True,[],{},18446744073709551615,"18446744073709551615"]]
    options += [{"store_directory":"cache","dry_run":v} for v in [None,0,"false",[],{}]]
    options += [None,[],False,1,"bad",{"store_directory":None},{"store_directory":0},{"store_directory":"cache","unknown":{"token":"synthetic"}}]

    for params in options:
        request={"protocol":1,"id":"gc","method":"store.gc","params":params}
        check(["rpc"],json.dumps(request,separators=(",",":")).encode()+b"\n",objects)
    for args in [[],["bad"],["gc"],["gc","--bad"],["gc","--json"],
                 ["gc","--store-directory"],["gc","--store-directory",""],
                 ["gc","--store-directory","missing"],
                 ["gc","--store-directory","cache"],["gc","--store-directory","cache","--json"],
                 ["gc","--store-directory","cache","--min-age-seconds","0","--execute","--json"]]:
        check(["store",*args],prepare=objects)
    for value in ["0","+0","-0","1_0","_1","1_","1__0","0x10","1.0","1e2","-1","18446744073709551615","18446744073709551616"]:
        check(["store","gc","--store-directory","cache","--min-age-seconds",value,"--json"],prepare=objects)
    print(f"{count} raw GC RPC/CLI fixture cases passed")
if __name__=="__main__":main()
