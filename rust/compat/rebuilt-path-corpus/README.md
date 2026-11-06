# Rebuilt pure path schema corpus

Fresh verification for the reconstructed implementation, not recovery of the lost source or its previous evidence.

Use Zig 0.16.0 with oracle source commit `5a4310debbcee26b3d2391bab2339f79cd6ec52e`. From this repository, provide absolute paths for `ZIG`, `ORACLE`, and writable `CACHE`:

```sh
"$ZIG" test --global-cache-dir "$CACHE/global" --cache-dir "$CACHE/local" \
  --dep core -Mroot="$PWD/rust/compat/rebuilt-path-corpus/oracle.zig" \
  -Mcore="$ORACLE/src/root.zig" \
  --test-filter rebuilt_path_oracle_pure_schema_20261106
```

Only one uniquely filtered harness test runs, covering 21 fixture cases. It invokes typed schema parsing, client-side selection and memory assertions only. No archives, network, install, Java, launch or authentication run. Never run the unfiltered oracle test suite for this verification.

Regenerate into a separate output directory with `python3 generate.py /path/to/output`, then compare generated JSON and Zig files. Existing fixtures are committed for independent replay.

Opaque byte FF and exact length boundaries are covered alongside NUL, error precedence, duplicate and skipped-file hash validation, numeric byte forms and overflow. CF required=false and duplicate provider planning are not validated by this Zig typed-schema corpus. Filesystem/platform filename behavior and real installation are not proven.
