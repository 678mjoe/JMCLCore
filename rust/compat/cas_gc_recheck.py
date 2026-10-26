#!/usr/bin/env python3
"""Run only the named ignored CAS test, with an owned-child deadline and reap."""
import argparse, pathlib, subprocess, sys
p = argparse.ArgumentParser()
p.add_argument('--test-binary', required=True)
p.add_argument('--timeout', type=float, default=45)
a = p.parse_args()
assert 0 < a.timeout <= 300
binary = str(pathlib.Path(a.test_binary).resolve())
listed = subprocess.run([binary, '--list'], capture_output=True, text=True, check=True, timeout=15)
names = [line.removesuffix(': test') for line in listed.stdout.splitlines()
         if line.endswith('::gc_rechecks_stale_preflight_after_owned_transaction: test')]
assert len(names) == 1, listed.stdout
child = subprocess.Popen([binary, '--ignored', '--exact', names[0], '--nocapture'],
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE)
try:
    out, err = child.communicate(timeout=a.timeout)
except subprocess.TimeoutExpired:
    child.kill()  # Only this exact owned test child, never a server/process lookup.
    out, err = child.communicate()  # Reap before reporting; fixture path is retained.
    sys.stdout.buffer.write(out); sys.stderr.buffer.write(err)
    raise SystemExit('Owned CAS test deadline exceeded; diagnostic fixture retained')
sys.stdout.buffer.write(out); sys.stderr.buffer.write(err)
assert child.returncode == 0, f'Owned test failed ({child.returncode}); fixture retained'
assert b'1 passed' in out and b'0 failed' in out
print('PASS: deterministic stale-preflight recheck and descriptor exclusion/release; not a GC blocking proof')
