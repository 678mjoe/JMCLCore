# Bounded unchanged-frontend bridge evidence

Executed on Linux x86_64, 2026-10-24. This is an actual Core executable / unchanged bridge check, **not** full frontend or application acceptance. No frontend source was edited or published.

## Exact inputs

- Frontend source: [`678mjoe/JMCL-frontend@0382b15692737e7888ec0530dc92c79ba25b2a53`](https://github.com/678mjoe/JMCL-frontend/tree/0382b15692737e7888ec0530dc92c79ba25b2a53). The three imported modules are byte-identical to frontend `main` commit `d6e4a7532af5b48af6a028e8fbd1cff1962091f4`.
- Rust implementation checkpoint: `eed1130b`; frozen executable SHA-256 `3a267c1c9141a318fe961026e627e7445b6599e17a5ba32666279f215d9ba10e`. This is historical evidence, not a claim that later integrated binaries were tested here.
- Zig source: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`; executable SHA-256 `b9e628efc5b7ea34047ad25129a98756b6f8ec5d0be38193aee2620d87a82bfe`.
- Rust 1.99.1, Bun 1.4.4. Standalone native harness dependencies: Tokio 1.53.0, serde_json 1.0.151, serde 1.0.229, async-trait 0.1.91.

## What ran

1. A standalone harness imported the original [`session.rs`](https://github.com/678mjoe/JMCL-frontend/blob/0382b15692737e7888ec0530dc92c79ba25b2a53/src-tauri/src/session.rs) and [`transport.rs`](https://github.com/678mjoe/JMCL-frontend/blob/0382b15692737e7888ec0530dc92c79ba25b2a53/src-tauri/src/transport.rs). It launched the frozen Rust executable through the real local-process transport. **19 wire requests across two sessions passed**: handshake, ping, queued concurrent calls, offline instance CRUD, synthetic public account metadata, typed errors/recovery, close and reopen with the same temporary disk state.
2. The unchanged TypeScript [`CoreSession`](https://github.com/678mjoe/JMCL-frontend/blob/0382b15692737e7888ec0530dc92c79ba25b2a53/src/lib/rpc.ts) was imported through Bun. An external shim replaced only the Tauri IPC boundary and forwarded requests through the unchanged native bridge to the real executable. **11 wrapper calls plus native handshake** completed against each of Rust and pinned Zig, with the two existing wrapper mismatches below explicitly asserted rather than hidden.

All data was temporary, synthetic public metadata. Reproduction requires checking out the exact frontend source, importing those unchanged modules in a standalone Tokio harness, launching the identified Core with `rpc`, then repeating the same sequence against pinned Zig. A TypeScript reproduction must label its IPC shim rather than describe it as native Tauri application IPC. Preserve the binary copy and hashes so concurrent builds cannot change the subject under test.

## Existing frontend wrapper mismatches

These occur with **both cores**, so they are not Rust regressions or original-Core defects:

- [`accountSave(profile)`](https://github.com/678mjoe/JMCL-frontend/blob/0382b15692737e7888ec0530dc92c79ba25b2a53/src/lib/rpc.ts#L1161) sends flat profile fields, whereas the core requires `params.account`. Both return `INVALID_PARAMS`. The generic request API with the correct nested shape successfully exercises account metadata CRUD.
- [`accountGet`](https://github.com/678mjoe/JMCL-frontend/blob/0382b15692737e7888ec0530dc92c79ba25b2a53/src/lib/rpc.ts#L1171) advertises a direct `AccountProfile`, but the wire response is `{stage, account}`. Thus the runtime value has `account.id`, not a top-level `id`.

The Rust protocol was not changed to compensate, and the paused frontend was not repaired by this check.

## Not covered

No packaged application, UI, native Tauri IPC, sidecar discovery, keychain, real authentication, progress-stream operation, SSH, game/server, live service, macOS or Windows frontend acceptance was exercised. No claim of full frontend compatibility or replacement readiness follows from this finite smoke test.

## Current frozen source rerun

On 2026-10-24 the same bounded harness was rerun against source `0b878035f81041ccd891098a8a0eb27e0012f48b`, frozen executable SHA-256 `4201b9b01f9bf20521e0cd92f22bb1644901e292ed6d066cf1d8e0e767a800a0`. The unchanged native bridge again passed **19 requests across two sessions**. The unchanged TypeScript wrapper again completed **11 calls against each core**, with the same two explicitly asserted pre-existing mismatches. Pinned Zig hash and frontend source revisions above were unchanged. No frontend file was modified; all original scope exclusions still apply.
