#!/usr/bin/env python3
"""Offline, exact-observation differential gate for explicitly implemented slices.

No network, real account, game, or user data. No output normalization: bytes,
exit status, stderr, event order and serial recovery must match the pinned Zig.
This harness is NOT a complete protocol/CLI/disk compatibility claim.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ORACLE_COMMIT = "5a4310debbcee26b3d2391bab2339f79cd6ec52e"


def request(**changes):
    value = {"protocol": 1, "id": "fixture", "method": "ping"}
    value.update(changes)
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode()


def cases():
    for args in [[], ["help"], ["--help"], ["help", "ignored"], ["version"], ["version", "--json"], ["version", "--bad"], ["rpc", "extra"], ["no-such-command"]]:
        yield "cli " + repr(args), args, b""
    samples = [b"", b"\n\r\n", request(), request(method="core.version"), b"not-json", b"{}", b"[]", b"null", b"1", b'"text"', request(id=""), request(id=None), request(id=1), request(method=None), request(method="no-such-method"), request(protocol=2), request(protocol=-1), request(protocol=4294967296), request(protocol=1.5), request(protocol=1.0), request(protocol="1"), request(protocol="1.0"), request(protocol="1e0"), request(protocol="+1"), request(protocol="01"), request(protocol=" 1"), request(protocol="0x1"), request(params=None), b'{"protocol":1,"id":"x","method":"ping","params":{"nested":{"same":1,"same":2}}}', request(params=["secret-not-logged"]), request(params={"access_token":"secret-not-logged"}), request(id="中文\\\"\n\t"), b'{"protocol":1,"id":"a","id":"b","method":"ping"}', b'{"protocol":1,"id":"a","method":"ping","params":{},"params":{}}', b'{"protocol":1,"id":"a","method":"ping","extra":1,"extra":2}', b'{"protocol":1,"id":"a","method":"ping"} trailing', b'{"protocol":1,"id":"\xff","method":"ping"}']
    for i, sample in enumerate(samples):
        yield f"rpc sample {i}", ["rpc"], sample+b"\n"+request()+b"\n"
    yield "unterminated valid", ["rpc"], request()
    yield "CRLF session", ["rpc"], request()+b"\r\n"+request(method="core.version")+b"\r\n"
    for size in [65534,65535,65536,65537,131072]:
        prefix=request()[:-1]+b',"padding":"'
        padded=prefix+b'x'*(size-len(prefix)-2)+b'"}'
        yield f"line boundary {size}", ["rpc"], padded+b"\n"+request()+b"\n"
    yield "oversize EOF", ["rpc"], b'x'*65536


def observe(binary, args, data):
    p=subprocess.run([str(binary), *args],input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=10)
    return p.returncode,p.stdout,p.stderr


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--zig",type=Path,required=True)
    parser.add_argument("--rust",type=Path,required=True)
    parser.add_argument("--oracle-source",type=Path,required=True)
    parser.add_argument("--allow-known-gaps",action="store_true",help="Explicit experimental gate, never a compatibility pass")
    args=parser.parse_args()
    commit=subprocess.check_output(["git","-C",str(args.oracle_source),"rev-parse","HEAD"],text=True).strip()
    if commit!=ORACLE_COMMIT:
        raise SystemExit(f"Oracle source must be pinned {ORACLE_COMMIT}; got {commit}")
    count=0
    gaps=[]
    for name,command,data in cases():
        expected=observe(args.zig.resolve(),command,data)
        actual=observe(args.rust.resolve(),command,data)
        if actual!=expected:
            invalid=b'{"protocol":1,"id":null,"event":"error","error":{"code":"INVALID_REQUEST","message":"Request must be valid JSON with protocol, id, and method fields"}}\n'
            pong=b'{"protocol":1,"id":"fixture","event":"result","result":{"pong":true}}\n'
            known=(name in {"line boundary 65536","line boundary 65537","line boundary 131072"} and expected==(0,invalid+invalid,b"") and actual==(0,invalid+pong,b""))
            if args.allow_known_gaps and known:
                gaps.append(name)
                continue
            print(f"FAIL {name}: expected {expected!r}; actual {actual!r}")
            raise SystemExit(1)
        count+=1
    print(json.dumps({"oracle_commit":commit,"cases_passed":count,"known_unresolved_gaps":gaps,"full_compatibility":False,"normalization":"none","zig_binary_sha256":hashlib.sha256(args.zig.read_bytes()).hexdigest(),"rust_binary_sha256":hashlib.sha256(args.rust.read_bytes()).hexdigest()}))

if __name__=="__main__":main()
