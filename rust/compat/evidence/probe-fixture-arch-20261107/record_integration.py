#!/usr/bin/env python3
"""Validate the integrated test-only native probe fixture, no system Java discovery."""
import hashlib,json,os,pathlib,re,subprocess,time
ROOT=pathlib.Path('/workspace/scratch/6fda792bd6ec/JMCLCore-acceptance-20261107')
BASE=pathlib.Path('/workspace/scratch/6fda792bd6ec/jmcl-tools')
OUT=pathlib.Path('/workspace/shared/jmcl-probe-integration-20261107')
TMP=BASE/'probe-integration-owned-20261107'
COMMIT='a924fe6c7572ab8b0fe0a24975c7860cdb966e03'
TREE='8005b88fa3c92a705c48bf2c23ca072eb575f2d4'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def git(*args):return subprocess.check_output(['git','-C',str(ROOT),*args],text=True).strip()
assert git('rev-parse','HEAD')==COMMIT and git('rev-parse','HEAD^{tree}')==TREE
assert not git('status','--porcelain')
TMP.mkdir(exist_ok=True);OUT.mkdir(exist_ok=True);assert not list(TMP.iterdir())
toolchain=BASE/'rustup-home/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin'
env=os.environ.copy();env.pop('JMCL_TEST_JAVA_MODE',None)
env.update(CARGO_HOME=str(BASE/'cargo-home'),RUSTUP_HOME=str(BASE/'rustup-home'),CARGO_TARGET_DIR=str(BASE/'acceptance-matrix-target-20261107'),CARGO_NET_OFFLINE='true',CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',RUSTC=str(toolchain/'rustc'),TMPDIR=str(TMP))
env['PATH']=str(toolchain)+os.pathsep+env['PATH']
checks=[
 ('fmt',['cargo','fmt','--manifest-path','rust/Cargo.toml','--all','--','--check']),
 ('fixture-fmt',['rustfmt','--edition','2024','--check','rust/tests/fixtures/java_probe.rs']),
 ('compile',['cargo','test','--manifest-path','rust/Cargo.toml','--offline','--locked','--all-targets','--no-run']),
 ('clippy',['cargo','clippy','--manifest-path','rust/Cargo.toml','--offline','--locked','--all-targets','--','-D','warnings']),
 ('native-probe',['cargo','test','--manifest-path','rust/Cargo.toml','--offline','--locked','--test','java_process','native_probe_deadline_and_option_sanitization','--','--exact','--nocapture'])]
manifest={'tested_commit':COMMIT,'tested_tree':TREE,'platform':'Linux x86_64 GNU','toolchain':'Rust/Cargo 1.99.1','source_fingerprints':{p:sha(ROOT/p) for p in ['rust/tests/fixtures/java_probe.rs','rust/tests/java_process.rs','rust/src/java/detect.rs','rust/src/java/managed.rs','rust/Cargo.lock']},'checks':[]}
for label,command in checks:
 print('running '+label,flush=True)
 p=subprocess.run(command,cwd=ROOT,env=env,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 log=OUT/(label+'.log');log.write_bytes(json.dumps({'command':command}).encode()+b'\n'+p.stdout+json.dumps({'returncode':p.returncode}).encode()+b'\n')
 assert p.returncode==0,(label,p.returncode,str(log))
 if label=='native-probe':
  text=p.stdout.decode();assert re.search(r'^running 1 test$',text,re.M)
  assert 'test native_probe_deadline_and_option_sanitization ... ok' in text
  assert '1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out;' in text
 manifest['checks'].append({'name':label,'command':command,'returncode':p.returncode,'log':log.name,'sha256':sha(log)})
# Existing hold mode has a two-second owned descendant. Wait for natural exit.
time.sleep(3)
owned=[]
for entry in pathlib.Path('/proc').iterdir():
 if not entry.name.isdigit():continue
 try:cmdline=(entry/'cmdline').read_bytes()
 except (FileNotFoundError,ProcessLookupError):continue
 if any(arg.startswith(str(TMP).encode()+b'/') for arg in cmdline.split(b'\0')):owned.append(entry.name)
assert not owned,('owned descendant remains',owned)
assert not list(TMP.iterdir())
assert git('rev-parse','HEAD')==COMMIT and not git('status','--porcelain')
manifest.update(clean_tree_before_and_after=True,exact_test={'passed':1,'failed':0,'ignored':0,'filtered_out':1},owned_tmp_empty=True,owned_argv_matches_after_natural_exit_wait=0,natural_exit_wait_seconds=3,managed_test_executed=False,system_java_discovery=False,real_jvm_executed=False,production_dependencies_workflows_unchanged=True)
manifest['test_binary_sha256']=sha(BASE/'acceptance-matrix-target-20261107/debug/deps/java_process-ff93e745fff4d412')
(OUT/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({'result':'passed','manifest':str(OUT/'manifest.json'),'exact_probe':'1/0/0','owned_live_matches_after_3s':0}),flush=True)
