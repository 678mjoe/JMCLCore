# Complete pack display-name byte path

Local continuation from `42cf6ec`; the published numeric snapshot is unchanged.

Mrpack and CurseForge name/package-version fields now retain arbitrary bytes via
the existing MetadataBytes decoder. Optional summary/author metadata uses the
same representation. Game versions, loader versions, paths and other fields are
not broadened by this change.

The complete changed path is:
1. Typed source string/byte-array decode and original byte-length validation.
2. Identity construction by name + one ASCII space + optional package version.
3. Manifest construction through ReadManifest/DisplayName, original validation,
   compact JSON field ordering and final newline.
4. Unix/Windows create_read uses the original anchored create_with writer;
   validation stays before directory creation, encoding remains inside its
   original closure, and cleanup ordering is unchanged.
5. Summary JSON emits valid UTF-8 as a string and opaque bytes as a complete byte
   array. CLI writes raw bytes; no lossy replacement or Display conversion.

A separate baseline checkout at `42cf6ec` failed this minimal parse regression:

```
{"formatVersion":1,"game":"minecraft","name":[65,255],"versionId":[66,254],"files":[],"dependencies":{"minecraft":"1.20.1"}}
```

Pure tests exercise both pack formats, UTF-8/NUL/non-UTF-8 names and versions,
Summary JSON, manifest encode/parse round trips, ordinary Manifest byte-for-byte
encoding compatibility, CLI output, each loader mapping, empty name/version and
256/257-byte combined-name limits. The combined length check remains at instance
manifest validation rather than being moved into schema parse.

The pinned Zig 0.16.0 test uses the actual standard-library typed-byte decoding,
formatting and JSON representation primitives used by the source implementation
(service.zig:3689,3783 and Stringify.zig:506). It does not execute the Core.

An additional exact test is limited to an owned tempfile directory. It exercises
create_read, disk-byte readback, opaque-name deletion, pre-write length rejection,
and injected encoding failure cleanup. No archive, download, installation or
user-owned instance directory is touched. This local Linux fixture does not
establish cross-platform filesystem or complete installation acceptance.
