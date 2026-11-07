# Native Windows original-file adoption identity

## Result

The public standard `windows-2025` runner executed exactly one real `Root::adopt`
integration test successfully on 2026-11-07: **1 passed, 0 failed, 0 ignored**.

- Run: https://github.com/678mjoe/JMCLCore/actions/runs/42391458369
- Job: https://github.com/678mjoe/JMCLCore/actions/runs/42391458369/job/128459732314
- Tested commit: `e0ec798e863c877c3fde77f1f86c1e06c5583d3a`
- Tested complete tree: `5f4599323c9a17bef83737f2890326e0d38f2ee6`
- Production baseline: `bdd6cb3e55d69cb9e0e02037d251635e289207a8`
- Baseline complete tree: `0e8e901bc587cf3c6d7dc1ba78821c4701785e20`
- Unchanged production `rust/src` tree: `99bfa7577af7597ab8069aecd278bb4e9a352296`
- Unchanged Zig `src` tree: `2c182bdb99c9c11b8bb203948c0f9b0ce160ed32`

The tested tree is intentionally different from the production baseline: it adds
Windows test assertions and an isolated verification harness. No production code,
manifest, lockfile, prior workflow, main branch, or production sidecar pin changed.
The later evidence documentation is not the tested commit and does not trigger CI.

## Exact command and scope

```sh
cargo test --offline --locked --manifest-path rust/Cargo.toml --target x86_64-pc-windows-msvc --test content_adopt adoption_reuses_real_file_identity_and_manual_registry -- --exact --nocapture --test-threads=1
```

One owned same-volume TempDir holds the original mod and CAS. Before adoption the
test captures the original file's Windows volume serial and combined high/low file
index through `GetFileInformationByHandle`. After the first adoption, both source
and CAS identities equal that original identity. After a second adoption returns
an empty list, both still equal it. Bytes, CAS/source read-only state, store verify,
Manual registry provider, and untouched `ignore.txt` are checked. A Windows-only
marker proves that the Windows assertions ran. Copy/skip fallback is not accepted.

No administrator or Developer Mode setup, symlink/junction scenario, Java, game,
account, business-network request, archive installer, GUI, or additional store test
is involved. Cargo fetch obtains locked official dependencies before offline test
execution. This is one focused acceptance result, not complete Rust replacement
acceptance or an extension of previously blocked scope.

## Evidence and controls

`evidence.json` retains the command's original merged stdout/stderr pipe bytes as
base64, rather than attempting to reconstruct them from rendered Actions text.
Local independent decoding checked byte count and SHA-256:

- Pipe bytes: 4353; SHA-256 `1b5b75d7f17eef3e8187d0ed227b07a61226a0ae6a965938791b1cf96ada802d`.
- Native test executable: PE32+/x86_64, 3,902,976 bytes; SHA-256 `58c957b12de43c3e219a049839674c47871a09c51efce96d915e575de3f71cde`.
- Rust 1.99.1, host `x86_64-pc-windows-msvc`.
- Test file SHA-256: `28fb816e464c205e2fdf021b9b5ab74d0f80c3aadc310ffbbe9dccf3035abe0c`.
- Workflow SHA-256: `97191040a24f9b97d0e562a2e19fb69d6b399e17d918fc4b5f12ee8ce8853575`.
- `run.py` SHA-256: `dc1323d16b69462f27621598dda41d734d4c9bc2258cf021da4a9a2ba84479ed`.
- `test_run.py` SHA-256: `0d5d068d267b8656b7d363621d2d0f0706a3db07bdf0726555c5b29e6b9b62b4`.

The harness checks actual native OS/architecture and Rust host, pinned original
test bytes, unchanged production tree, baseline tree, allowed diff paths, clean
checkout before/after, fresh build/TMP ownership, native PE header and unique test
executable. It rejects nonzero exit, missing/duplicate Windows marker, zero passed,
ignored/failed cases, wrong selector, scripts, and truncated PE. Timeout handling
retains partial raw bytes and fails closed. Three pure guard tests pass in normal
Python and `python -O`, both locally and on Windows; timeout controls include CRLF,
NUL and non-UTF8 bytes. Independent review additionally tested valid synthetic PE
and rejected ARM64, DLL, and PE32 headers without executing another business test.

Local Linux exact test passed 1/0/0; `cargo fmt --check`, focused offline locked
`cargo clippy --test content_adopt -- -D warnings`, and `git diff --check` passed.
Independent source/workflow/guard review found no remaining publication blockers.
