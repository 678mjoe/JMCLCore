# Requested and persisted instance ID errors

The pinned Zig `instance.get` handler maps a failed manifest load with
`InvalidInstanceId` to `INSTANCE_NOT_FOUND`, whether the invalid ID came from
the request or the persisted record. Rust previously recognized only an invalid
request ID or an absent file. The correction adds the persisted error to the
lookup mapping only; delete and CLI behavior are unchanged.

`instance_id_errors.py` performs 56 exact read-only RPC/CLI comparisons against
Zig commit `5a4310debbcee26b3d2391bab2339f79cd6ec52e`: valid records, invalid
stored IDs, requested invalid/absent IDs, valid-but-mismatched stored IDs,
missing fields, parse-before-semantic errors, and list omission. Positive lookup
and list expectations prevent error/error matches from counting as success.
Every operation preserves the complete owned directory snapshot, including
unrelated hardlinked bytes, inode, mode, mtime and link count. No creation,
deletion, installation, lifecycle or Java methods are called.

```sh
python rust/compat/instance_id_errors.py --zig "$JMCL_ZIG_ORACLE" \
  --rust rust/target/debug/jmcl-core-rust --oracle-source "$JMCL_ZIG_SOURCE"
```

The corpus passed on Linux; other native hosts require their CI execution.
