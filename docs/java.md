# Java Runtime Selection

JMCL Core probes local Java executables and selects a runtime for a resolved
Minecraft version.

```sh
jmcl-core java detect
jmcl-core java detect --path /custom/jdk/bin/java --json
jmcl-core java select 26.2
```

## Discovery

Candidates are considered in this order:

1. Explicit `--path` or RPC `paths` entries.
2. `JAVA_HOME/bin/java`.
3. Platform installations.
4. Every executable Java entry found through `PATH`.

macOS discovery uses `/usr/libexec/java_home -V`. Linux discovery scans
`/usr/lib/jvm` and `/usr/java`. Candidates resolving to the same `java.home`
are deduplicated.

Windows currently supports explicit paths, `JAVA_HOME`, and `PATH`. Registry
and managed-runtime discovery remain future work.

## Probe

Each candidate runs without a shell:

```text
java -XshowSettings:properties -version
```

Both stdout and stderr are captured with a 256 KiB limit and ten-second
deadline. `JAVA_TOOL_OPTIONS`, `_JAVA_OPTIONS`, and `JDK_JAVA_OPTIONS` are
removed from the probe environment so user-injected flags cannot alter the
result.

The probe records:

- Java specification, runtime, and implementation versions.
- Vendor, VM name, and `java.home`.
- JVM process architecture and data model.

Architecture aliases such as `amd64` and `aarch64` normalize to `x86_64` and
`arm64`. A conflicting `os.arch` and `sun.arch.data.model` is rejected.

## Compatibility

`javaVersion.majorVersion` is treated as an exact automatic-selection
requirement. A newer Java is not silently selected for an older requirement,
because legacy Minecraft and mod loaders can fail on removed APIs or stronger
module boundaries.

When metadata omits `javaVersion`, JMCL Core requires Java 8. The selected JVM
architecture must match the target architecture used by install planning and
native classifier selection.

The metadata `component` remains an opaque managed-runtime channel. It is not
used to infer Java versions because multiple component names can represent the
same major release.

## Current Scope

The selector only uses installed runtimes. It does not yet download Mojang Java
Runtime components or persist per-instance Java choices. Those capabilities can
be added without changing the probe and compatibility model.
