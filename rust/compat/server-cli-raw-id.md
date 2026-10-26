# Early rejection of non-UTF8 local server IDs

Scope is only `server mods|datapacks list|enable|disable|remove`. On Linux, an ID
which cannot be represented as UTF8 is forwarded alone to the existing local
parser, which rejects it as `error: InvalidArguments\n` with exit 1. This happens
before scanning later options. The source ID grammar is strictly lowercase ASCII
alphanumeric with bounded punctuation; a non-UTF8 ID can never be valid. No raw ID
is accepted, no replacement String is invented, and no raw project is decoded.

Pinned source 5a4310debbcee26b3d2391bab2339f79cd6ec52e:
server/server_store.zig validateId (lines 58–68) plus the CLI local methods, each of
which validates ID before selector/options parsing. The exact recognized route
allowlist and existing non-Linux platform rejection precede this narrow Linux
branch. main.rs, hidden dispatch, instance routes, operational parsers, read/root
helpers, guards and provider paths are unchanged.

Base: 4574d4ab3e2743ab1828367a0ede9c205dcdfbad; binary
498c4cad286be30f1359c5df3a5c27846400615aef21feaf1834d40162de6b0d.
The baseline has 256 differences in 400 paired calls: FF, invalid trailing bytes,
WTF8 and overlong UTF8 IDs panic at fallback argv conversion instead of the source
argument error. Ordinary Unicode/invalid ASCII/empty ID controls already match.
Raw-ID cases deliberately include later raw project/unknown arguments, proving
ID rejection must occur before tail scanning, not merely before root opening.

The new hook unit is actual red→green. All 400 pairs now match with zero snapshot
violations; 1,280 prior native-root CLI pairs and five named hook/argument tests
remain green. Every call runs with owned explicit root candidates/working directory,
readonly sentinel peers and a must-not-create root. Full before/after snapshots
and argv/output bytes are retained. Source and implementation paths reject before
any root/guard access; no state/PID, CAS, archive, Java or provider operation occurs.
Clippy/fmt/diff checks pass.

```
cargo test --test server_cli_raw_id
python rust/compat/server_cli_raw_id.py --zig <pinned-binary> \
  --rust <rust-binary> --oracle-source <pinned-source> [--output <raw-results.json>]
```

Non-Linux uses eight unchanged platform-rejection controls rather than the Linux
InvalidArguments expectation. Native execution remains CI evidence, not inferred
from local Linux results. Raw project/other unsupported arguments with text IDs,
instance raw IDs and other routes remain separate compatibility boundaries.
