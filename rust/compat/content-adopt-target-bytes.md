# Adoption target byte decoding — COMPAT-CAS-001

This slice changes only local adoption parameter decoding and ordered native
root selection. It does **not** establish whole-CAS parity with Zig.
**COMPAT-CAS-001** records the user-accepted persistent digest-lock layout
difference; acceptance does not establish concurrent safety.

## Source contract and implementation

Pinned source: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.
`protocol/content.zig` parses directory/store byte strings, checks directory range,
opens game root, then CAS root. `protocol/datapacks.zig` parses directory/world/store
bytes, checks directory/world range, opens CAS root, then the guarded existing
world through `Service.adoptWorldDatapacks`. Native conversion remains inside
these ordered branches; it is not performed eagerly before the first root.

The existing dynamic byte decoder preserves numeric lexemes. Default store behavior
is unchanged for absent input; explicit null is not converted into a default. The
new tests never exercise absent/null/empty/default stores: every filesystem case
uses an explicit nonempty owned store path, with separately labeled invalid-element
parse controls. Existing native Root helpers and Store::open(Path) are reused.
`Root::adopt`, digest transactions, import/materialize, no-follow policy, world-lock
internals, shared store implementation and fallback behavior are byte-identical.
UTF-8 world results use the original String serializer wrapper; opaque world bytes
use its existing internal byte-capable serializer.

## Evidence has three separate contracts

1. Exact source wire, registry and content/peer effects; successful/repeated adoption,
   same-volume CAS/source/peer inode equality, SHA1, bytes and readonly publication.
2. Independent per-engine expected CAS trees: Zig has no digest lock leaf after
   these adoption cases; Rust has exactly one empty regular digest lock per adopted
   digest, with stable identity across repeat. Each post-call raw snapshot (file bytes, inode/device, mode, mtime and link count)
   is retained in per-engine diagnostics outside cross-fixture equality. Compared
   states are canonical path/content/readonly projections; content.json is parsed
   for that projection, but its captured raw bytes are separately required to
   match exactly between source and Rust. Every layout difference is printed,
   not filtered into an equality assertion.
3. ASCII/Unicode byte inputs also have independent Rust String-input twins, comparing
   full relative game/CAS effects. Opaque native paths have no JSON Unicode String
   equivalent and are explicitly capability/expected-state tests instead. Invalid
   byte elements likewise have no String-equivalence claim.

Linux evidence:94 paired sequences,468 total calls including46 String twins;
prior decoder55 wire mismatches, new decoder0. The44 successful source/Rust canonical
CAS-layout differences remain visible under **COMPAT-CAS-001**. The user accepted
keeping the existing locks and documenting this layout difference on 2026-10-26,
with separate concurrency verification still required.
The ordinary blocked-root unit was red with INVALID_PARAMS and now reaches
INSTALL_ROOT_ERROR without creating its explicit owned store. Existing180 mutable
world and368 read-only world RPC comparisons remain exact. Preinitialized error
fixtures preserve full snapshots; separate fresh-store errors permit only the
source-ordered CAS scaffold creation while preserving game/peer/sibling state.

```
cargo test --test content_adopt_target_bytes
python rust/compat/content_adopt_target_bytes.py \
  --zig /path/to/pinned/jmcl-core --rust /path/to/jmcl-core-rust \
  --oracle-source /path/to/pinned/source
```

Ordinary cases run every native OS; held owned flock fixtures are Unix-only, not
Windows lock evidence. Physical raw names use FF/Unix or WTF-8/Windows; only exact
EILSEQ/ERROR_INVALID_NAME capability outcomes suppress those cases. Other errors
fail. Readonly failures are not allowed away. Windows fixtures are retained rather
than clearing shared readonly attributes for cleanup. Local results are Linux
only; this is not Windows/macOS native acceptance.

## Existing lock artifact, history and limits

Adoption's Rust transaction and its technical note originate in merged commit
`234562fa8b9ec2b9baa37ec41d90dad530c032e5` (milestone3 /PR5), predating byte decoding.
`content-adopt-slice.md` documents the digest lock spanning import/materialize;
`install-slice.md` documents avoiding a GC drop/reacquire window. Those statements
were historical rationale, not original approval of the extra persistent lock
artifact. The explicit 2026-10-26 user decision now accepts keeping the mechanism
and recording its layout difference; it does not retroactively prove concurrency.

Zig service adoption calls importObject/materialize without acquiring the digest
lock. Rust's per-file transaction holds it through import/materialize and entry
assembly, drops at the end of that iteration, then saves the registry outside the
transaction. Closing releases the OS lock but retains the empty leaf. Both GC
implementations acquire the same digest lock and recheck link count/age before
deletion; fresh linked objects are already excluded by link count. Any protection
against a particular adoption/GC race is source-derived inference, not a concurrent
experiment. Existing documented Windows GC lock-contention behavior is separate;
no new race, lock bypass or corruption claim is made.

The user accepted this bounded layout deviation (COMPAT-CAS-001), not full-disk
source equivalence. Separate concurrent-safety verification remains pending. No lock is deleted, no
transaction protection is removed, and no snapshot is silently normalized. All
payloads are opaque owned files; no archive interpretation, extraction, provider
requests, Java, process identities/signals or concurrent GC experiments run.
