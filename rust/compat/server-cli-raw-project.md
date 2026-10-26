# Local server CLI project bytes and early text-ID validation

Only `server mods|datapacks list|enable|disable|remove` OS-argument interception
changes. Private local project selectors retain bytes until the existing
`content::local_project` validator, after root/manifest/lifecycle guard and, for
datapacks, properties/world acquisition. Valid UTF8 selectors retain the same
validation. No replacement/lossy String is made. Non-ASCII project references
(including valid Unicode) are rejected by the source's ASCII project grammar.
LIST still rejects the project option at argument parsing; duplicate values remain
last-wins and consumed options are interpreted by the existing local parser.

Invalid text IDs, like previously handled opaque IDs, are forwarded alone to the
existing ID validator before examining later arguments. No ID acceptance expands.
The non-Linux platform gate remains first. main.rs, public operational Params,
adoption/provider/hidden routes, lifecycle/store code and existing helpers do not
change. Other raw arguments remain independent boundaries, not claimed fixed.

Source pin: 5a4310debbcee26b3d2391bab2339f79cd6ec52e. Relevant paths:
`src/cli/server_content.zig` SelectorOptions/parseSelectorArgs/runSetEnabled/runRemove;
`src/cli/server_datapacks.zig` corresponding local methods;
`src/service.zig` setServerContentEnabled/removeServerContent and datapack wrappers,
then setInstanceContentEnabled/removeInstanceContent;
`src/content/modrinth.zig` validateProjectRef (ASCII bytes, length 1–64).

Baseline 19f6a0c64812736dec130b023525cc9afb538b44, executable SHA256
0c7301a860f4c9dfb65031f19516b7c465356c7164f0292e8403dcd1402ec215:
200 paired calls expose 144 differences, all Rust argv conversion panics (101).
56 valid Unicode/ASCII selector, all-UTF8 invalid-ID and already-fixed raw-ID
controls match. Full raw
per-engine snapshots show no filesystem changes. The pinned executable hash is
0d49ff4130720c67397c3f673afaaf226f4b1f8384f68ea599e43340030c164b.

The corpus uses explicit owned roots, existing empty registries and opaque readonly
hardlink peers. Every mutation selector either fails validation or matches no
entry: exact expected errors are asserted independently of cross-engine equality.
It covers invalid text/raw IDs, root/missing server/manifest errors, malformed
unheld lifecycle-lock leaf, properties precedence, both selectors, unknown/missing
arguments, consumed options, duplicate raw→valid/valid→raw/empty values. server.pid
is absent before/after; no process identities/signals, CAS, install, archive, Java
or provider requests occur. Output diagnostics retain raw argv and complete
before/after mode/inode/link-count/mtime/file-byte snapshots.

```
cargo test --test server_cli_raw_project
python rust/compat/server_cli_raw_project.py --zig <pinned-binary> \
  --rust <rust-binary> --oracle-source <pinned-source> [--output <raw-results.json>]
```

Linux: all 200 pairs now match with zero invariant failures; the 24 all-UTF8
invalid-ID controls remain byte-identical to the old normal CLI path. The prior
400 raw-ID and 1,280 native-root pairs, six named hook/argument tests (including
the explicitly invoked ignored raw-file oracle case), Clippy and diff checks
pass. Final executable SHA256:
0212c75c2881d7ea0316f9c096c0dd44de20e99c4063f82490259c8fd54a105f.

Non-Linux runs eight
unchanged platform-gate controls, not Linux server operations; native acceptance
is separate CI evidence.
