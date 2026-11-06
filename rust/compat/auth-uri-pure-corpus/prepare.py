"""Generate the pure Rust adapter from the checkout, without executing Core."""
from pathlib import Path
import sys
root=Path(__file__).resolve().parents[2]
out=Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True)
s=(root/'src/auth/mod.rs').read_text()
s=s[s.index('fn https('):s.index('fn validate_locale')]
assert s.count('Zeroizing::new(Vec::with_capacity(host.len()))')==1
s=s.replace('Zeroizing::new(Vec::with_capacity(host.len()))','Vec::with_capacity(host.len())')
num=(root/'src/cli_number.rs').read_text().split('\n',1)[1]
(out/'candidate.rs').write_text('mod cli_number {\n'+num+'}\n'+s+'\nfn main(){for arg in std::env::args().skip(1){println!("{}",https(&arg));}}\n')
for name in ['oracle.zig','compare.py']:
 (out/name).write_bytes((Path(__file__).parent/name).read_bytes())
