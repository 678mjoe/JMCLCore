#!/usr/bin/env python3
"""Copy pinned source and inject metadata fetch; never modify the original oracle.
The optional artifact-fetch guard also replaces the copied downloadOnce body.
This is an injected service oracle, not the unmodified production wire oracle.
"""
import argparse, pathlib, subprocess, shutil, hashlib, json, os
p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--output',required=True);p.add_argument('--zig',required=True);p.add_argument('--global-cache');p.add_argument('--deny-artifact-fetch',action='store_true');a=p.parse_args();source=pathlib.Path(a.source).resolve();out=pathlib.Path(a.output).resolve()
assert out != source and source not in out.parents and out not in source.parents, 'fixture output must be separate from pinned source'
sha=subprocess.check_output(['git','-C',str(source),'rev-parse','HEAD'],text=True).strip()
assert sha=='5a4310debbcee26b3d2391bab2339f79cd6ec52e',sha
assert not subprocess.check_output(['git','-C',str(source),'status','--porcelain','--','src'],text=True).strip()
out.mkdir(parents=True,exist_ok=True);shutil.copytree(source/'src',out/'src',dirs_exist_ok=True)
p=out/'src/service.zig';before=p.read_text();start=before.index('    fn fetchBodyWithHeader(');end=before.index('\n    fn fetchBodyTask(',start)
replacement='''    fn fetchBodyWithHeader(self: Service, url: []const u8, max_bytes: usize, extra_header: ?https_fallback.ExtraHeader) !ResponseBody {
        if (extra_header) |header| try header.validate();
        const env = self.environ_map orelse return error.MissingFixture;
        const path = env.get("JMCL_CONTENT_FIXTURES") orelse return error.MissingFixture;
        const bytes = try std.Io.Dir.cwd().readFileAlloc(self.io, path, self.allocator, .limited(32 * 1024 * 1024));
        defer self.allocator.free(bytes);
        var document = try std.json.parseFromSlice(std.json.Value, self.allocator, bytes, .{});
        defer document.deinit();
        const value = document.value.object.get(url) orelse return error.MissingFixture;
        const body = value.string;
        if (body.len > max_bytes) return error.ResponseBodyTooLarge;
        const owned = try self.allocator.dupe(u8, body);
        return .{.allocator=self.allocator,.storage=owned,.content=owned};
    }
'''
after=before[:start]+replacement+before[end:];p.write_text(after)
import difflib
patch=''.join(difflib.unified_diff(before.splitlines(True),after.splitlines(True),fromfile='a/src/service.zig',tofile='b/src/service.zig'))
(out/'fixture-transport.patch').write_text(patch)
(out/'provenance.json').write_text(json.dumps({'source_head':sha,'optimization':'ReleaseSafe','source_service_sha256':hashlib.sha256(before.encode()).hexdigest(),'patched_service_sha256':hashlib.sha256(after.encode()).hexdigest(),'patch_sha256':hashlib.sha256(patch.encode()).hexdigest()},indent=2))
if a.deny_artifact_fetch:
    installer=out/'src/install/installer.zig';original=installer.read_text()
    begin=original.index('fn downloadOnce(');body=original.index(') !void {',begin)+len(') !void {')
    depth=1;end=body
    while depth:
        if original[end]=='{':depth+=1
        elif original[end]=='}':depth-=1
        end+=1
    guarded=original[:body]+"\n    _ = client; _ = root; _ = destination_path; _ = expected; _ = url; _ = sync_file; _ = progress;\n    return error.FixtureArtifactFetchForbidden;\n}"+original[end:]
    installer.write_text(guarded)
    guard_patch=''.join(difflib.unified_diff(original.splitlines(True),guarded.splitlines(True),fromfile='a/src/install/installer.zig',tofile='b/src/install/installer.zig'))
    (out/'artifact-fetch-guard.patch').write_text(guard_patch)
    provenance=json.loads((out/'provenance.json').read_text())
    provenance.update(deny_artifact_fetch=True,source_installer_sha256=hashlib.sha256(original.encode()).hexdigest(),guarded_installer_sha256=hashlib.sha256(guarded.encode()).hexdigest(),guard_patch_sha256=hashlib.sha256(guard_patch.encode()).hexdigest())
    (out/'provenance.json').write_text(json.dumps(provenance,indent=2))
binary=out/('jmcl-content-fixture-oracle.exe' if os.name=='nt' else 'jmcl-content-fixture-oracle')
subprocess.run([a.zig,'build-exe','-OReleaseSafe','--dep','jmcl_core',f'-Mroot={out}/src/main.zig',f'-Mjmcl_core={out}/src/root.zig',f'-femit-bin={binary}','--global-cache-dir',str(pathlib.Path(a.global_cache).resolve() if a.global_cache else out/'zig-cache')],check=True)
provenance=json.loads((out/'provenance.json').read_text());provenance['binary_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest();(out/'provenance.json').write_text(json.dumps(provenance,indent=2))
print(binary)
