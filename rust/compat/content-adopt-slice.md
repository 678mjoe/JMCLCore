# Unmanaged content adoption

The four content families now adopt ordinary unmanaged files through the native
Rust CAS transaction. The opened game/world capability is bridged to the artifact
root; source and destination are not reconstructed as ambient paths. A digest
lock spans import and materialization. Registry publication follows all files,
matching Zig's partial-publication behavior if an earlier file succeeded.

Local evidence against Zig source `5a4310debbcee26b3d2391bab2339f79cd6ec52e`,
executable SHA256 `b9e628efc5b7ea34047ad25129a98756b6f8ec5d0be38193aee2620d87a82bfe`:

- 120 exact RPC/stdout/stderr/status and game-file snapshot comparisons.
- 28 exact CLI/stdout/stderr/status and game-file snapshot comparisons. Each valid
  case explicitly requires success and one adopted file.
- Real CAS and destination inode equality, readonly publication, manual registry,
  ignored extension preservation and second-adoption no-op.

Run `JMCL_ZIG_ORACLE=<pinned binary> cargo test --test content_adopt -- --include-ignored`.
Native Windows execution remains a CI gate; no native Windows success is claimed.
Remote provider installation and version switching are separate unfinished work.

A subtle compatibility boundary: listing follows an explicitly supplied content
subdirectory alias, but CAS import rejects the aliased descendant. Bridging the
child handle instead would accidentally permit adoption the original rejects.
The regression corpus preserves the original rejection and absent registry write.
