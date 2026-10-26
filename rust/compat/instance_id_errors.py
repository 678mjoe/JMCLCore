#!/usr/bin/env python3
"""Read-only requested/stored instance ID precedence, no instance mutations."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def snapshot(root):
    return {str(p.relative_to(root)): (p.read_bytes() if p.is_file() else None, p.stat().st_ino, p.stat().st_mode, p.stat().st_mtime_ns, p.stat().st_nlink) for p in [root, *root.rglob("*")]}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--zig", required=True)
    parser.add_argument("--rust", required=True)
    parser.add_argument("--oracle-source", required=True)
    args = parser.parse_args()
    assert subprocess.check_output(["git", "-C", args.oracle_source, "rev-parse", "HEAD"], text=True).strip() == "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    binaries = [Path(args.zig).resolve(), Path(args.rust).resolve()]
    base = {"id":"demo","name":"Demo","version_id":"fake"}
    cases = [("valid",base,"demo",None,None,True)]
    for id in ["../bad", "Upper", "con", "", "has space"]:
        cases.append(("stored-"+id,dict(base,id=id),"demo","INSTANCE_NOT_FOUND","InvalidInstanceId",False))
    cases += [
        ("stored-id-before-semantic",dict(base,id="../bad",name="",schema_version=2),"demo","INSTANCE_NOT_FOUND","InvalidInstanceId",False),
        ("stored-id-after-typed",dict(base,id="../bad",name=None),"demo","INSTANCE_LOAD_FAILED","UnexpectedToken",False),
        ("stored-id-after-enum",dict(base,id="../bad",source="bad"),"demo","INSTANCE_LOAD_FAILED","InvalidEnumTag",False),
        ("mismatch",dict(base,id="other"),"demo","INSTANCE_LOAD_FAILED","InstanceIdMismatch",False),
        ("missing-id",{"name":"Demo","version_id":"fake"},"demo","INSTANCE_LOAD_FAILED","MissingField",False),
        ("requested-invalid",base,"../bad","INSTANCE_NOT_FOUND","InvalidInstanceId",True),
        ("requested-invalid-before-typed",dict(base,name=None),"../bad","INSTANCE_NOT_FOUND","InvalidInstanceId",False),
        ("requested-missing",base,"missing","INSTANCE_NOT_FOUND","FileNotFound",True),
    ]
    count=0
    with tempfile.TemporaryDirectory(prefix="owned-instance-id-") as td:
        root=Path(td);d=root/"instances/demo";d.mkdir(parents=True)
        sibling=root/"sibling";sibling.write_bytes(b"preserve");os.link(sibling,root/"sibling-link")
        for label,document,requested,rpc_error,cli_error,listed in cases:
            (d/"instance.json").write_text(json.dumps(document));before=snapshot(root)
            for operation in ["rpc-get","rpc-list","show","show-json"]:
                values=[]
                for binary in binaries:
                    if operation.startswith("rpc-"):
                        method=operation[4:]
                        wire=json.dumps({"protocol":1,"id":"identity","method":"instance."+method,"params":{"directory":"instances","id":requested}})+"\n"
                        p=subprocess.run([str(binary),"rpc"],input=wire.encode(),cwd=root,capture_output=True,timeout=30)
                        assert p.returncode==0 and not p.stderr
                        v=json.loads(p.stdout)
                        if method=="list": assert len(v["result"]["instances"])==int(listed),(label,v)
                        elif rpc_error: assert v["error"]["code"]==rpc_error,(label,binary,v)
                        else: assert v["result"]["id"]=="demo" and v["result"]["name"]=="Demo"
                    else:
                        p=subprocess.run([str(binary),"instance","show",requested]+(["--json"] if operation.endswith("json") else []),cwd=root,capture_output=True,timeout=30)
                        if requested == "../bad": assert (p.returncode,p.stdout,p.stderr)==(2,b"",b"usage: jmcl-core instance show ID [--instances-directory PATH] [--json]\n"), (label,binary,p)
                        elif cli_error: assert (p.returncode,p.stdout,p.stderr)==(1,b"",f"failed to load instance: {cli_error}\n".encode()),(label,binary,p)
                        else: assert p.returncode==0 and not p.stderr and b"Demo" in p.stdout
                    assert snapshot(root)==before,(label,operation,"disk changed")
                    values.append((p.returncode,p.stdout,p.stderr))
                assert values[0]==values[1],(label,operation,values)
                count+=1
    print(f"PASS: {count} requested/stored instance ID comparisons; exact error precedence and disk preservation")

if __name__=="__main__": main()
