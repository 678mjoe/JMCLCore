#!/usr/bin/env python3
"""Build a synthetic-only protocol oracle. Production auth logic is unchanged.
Usage: build_auth_oracle.py ZIG PINNED_REPOSITORY OUTPUT_DIRECTORY
Only the HTTP-adapter import is replaced; no transport can perform networking.
"""
import io, os, pathlib, shutil, subprocess, sys, tarfile
zig, repo, dest = map(pathlib.Path, sys.argv[1:])
repo, dest = repo.resolve(), dest.resolve()
assert dest != repo and repo not in dest.parents, 'output must be outside source repository'
expected = '5a4310debbcee26b3d2391bab2339f79cd6ec52e'
# Read the exact commit, regardless of the checkout's current branch.
dest.mkdir(parents=True, exist_ok=True)
archive = subprocess.check_output(['git', '-C', str(repo), 'archive', expected, 'src'])
with tarfile.open(fileobj=io.BytesIO(archive)) as tree:
    tree.extractall(dest, filter='data')
binary = dest / ('auth-oracle.exe' if os.name == 'nt' else 'auth-oracle')
src = dest / 'src'
for relative in ['root.zig', 'service.zig']:
    p = src / relative
    text = p.read_text()
    old = '@import("auth/http.zig")'
    assert text.count(old) == 1
    p.write_text(text.replace(old, '@import("auth/fake_http.zig")'))
here = pathlib.Path(__file__).resolve().parent
shutil.copyfile(here / 'auth_fake_http.zig', src / 'auth/fake_http.zig')
subprocess.run([str(zig.resolve()), 'build-exe', '--dep', 'jmcl_core', f'-Mroot={here / "auth_oracle.zig"}', f'-Mjmcl_core={src / "root.zig"}', '-O', 'ReleaseSafe', f'-femit-bin={binary}', '--cache-dir', str(dest / 'cache')], check=True)
print(binary)
