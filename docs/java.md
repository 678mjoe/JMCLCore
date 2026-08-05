# Java Runtime Selection

JMCL Core probes local Java executables, downloads managed runtimes when no
compatible local runtime exists, and selects a runtime for a resolved
Minecraft version.

```sh
jmcl-core java detect
jmcl-core java detect --path /custom/jdk/bin/java --json
jmcl-core java select 26.2
jmcl-core java runtime list
jmcl-core java runtime install --component java-runtime-gamma
jmcl-core java runtime install --major 8
jmcl-core java runtime remove mojang-java-runtime-gamma-mac-os-arm64
```

## Discovery

Local candidates are considered in this order:

1. Explicit `--path` or RPC `paths` entries.
2. `JAVA_HOME/bin/java`.
3. Platform installations.
4. Every executable Java entry found through `PATH`.

macOS discovery uses `/usr/libexec/java_home -V`. Linux discovery scans
`/usr/lib/jvm` and `/usr/java`. Candidates resolving to the same `java.home`
are deduplicated.

Windows currently supports explicit paths, `JAVA_HOME`, and `PATH`. Registry
discovery remains future work because managed runtimes cover the no-local-Java
case.

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

## Managed Runtimes

When no compatible local runtime exists, JMCL Core downloads a managed
runtime. Two providers are supported; the choice is deterministic and driven
by capability, never by network failure order:

1. **Mojang** (`mojang`) — the vanilla `javaVersion.component` channel
   (`jre-legacy`, `java-runtime-alpha|beta|gamma|delta|epsilon`). Preferred
   whenever the component exists for the target platform. Per-file downloads
   use the `raw` variant only and are SHA-1 + size verified, like every other
   Mojang artifact.
2. **Azul Zulu** (`zulu`) — covers the platforms Mojang does not ship:
   `jre-legacy` (Java 8) and `java-runtime-alpha` (Java 16) on
   `mac-os-arm64`/`windows-arm64`, and every major on `linux-arm64`. Packages
   are resolved through the Azul metadata API (`latest` GA JRE, no JavaFX)
   and the archive is SHA-256 + size verified before extraction.

Runtimes install under the artifact store:

```text
<store>/runtimes/<provider>-<id>/<platform>/   runtime tree
<store>/runtimes/<provider>-<id>/<platform>/.jmcl-runtime.json   receipt
```

`id` is the Mojang component name or the Zulu major version (for example
`zulu-8`). The receipt records the provider, component or Azul
`package_uuid`, probed major version, source URLs, hashes, and install time;
it backs `java.runtime.list` and integrity re-verification. Installation
stages into a temporary sibling directory and atomically renames on success;
interrupted staging directories are removed on the next attempt. Mojang
symlink entries are recreated as symlinks with targets validated to stay
inside the runtime tree.

The JVM home is discovered after extraction (Mojang macOS uses
`jre.bundle/Contents/Home`, Zulu macOS uses `zulu-*.jre/Contents/Home`,
Windows and Linux use the tree root) and the runtime is probed before the
receipt is written. A probe result that contradicts the expected major fails
the install.

`java.runtime.install` accepts either an exact Mojang `component` name or a
`major` version. A major request resolves to the newest known Mojang
component with that major on the target platform (known table, verified by
probe after install), and falls back to Zulu when Mojang ships no component
for that major and platform.

Managed runtime downloads always go to the origin services. `source:bmclapi`
does not mirror the Java runtime catalogs and never rewrites these URLs.

## Selection Policy

`java_policy` on `launch.plan`, `launch.execute`, and `install.execute`
accepts `auto` (default), `local`, or `managed`:

- `auto`: local exact-major match first, then an installed managed runtime
  (exact metadata component first, then any managed runtime with the required
  major), then a managed download.
- `local`: local runtimes only; never uses or downloads managed runtimes.
- `managed`: installed managed runtime first, then local exact-major, then a
  managed download.

`launch.plan` never downloads; it reports `JAVA_INCOMPATIBLE` when only a
download would satisfy the requirement. `launch.execute` and
`install.execute` perform the download under `auto`/`managed` with the usual
progress events.

## Forced Override

`launch.plan` and `launch.execute` accept `java_override` (CLI
`--java-override`), an absolute path to a Java executable. The override is
probed like any candidate, but the exact-major and architecture gates are
skipped; the caller takes responsibility for the consequences. It is mutually
exclusive with `java_paths`, is never persisted into instance manifests, and
is surfaced as `forced: true` plus the probed runtime facts in results and
events. Use it for diagnostics and experiments (for example running legacy
Minecraft on a newer patched JVM); it is not an installation or selection
mechanism, and loader installer processors still require a compatible Java.

## RPC

```json
{"protocol":1,"id":"jr-1","method":"java.runtime.list","params":{"store_directory":".jmclcore"}}
{"protocol":1,"id":"jr-2","method":"java.runtime.install","params":{"major":8,"store_directory":".jmclcore","workers":8,"retries":2}}
{"protocol":1,"id":"jr-3","method":"java.runtime.remove","params":{"runtime":"mojang-java-runtime-gamma-mac-os-arm64","store_directory":".jmclcore"}}
```

`java.runtime.install` emits the same throttled `progress` events with
monotonic `bytes_processed` as game installs. `launch.plan`, `launch.execute`,
and `install.execute` accept `java_policy`; the launch methods also accept
`java_override`.

## Current Scope

Not yet implemented: Windows registry discovery, LZMA download variants,
additional providers, and a server-side forced override. The metadata
`component` field is authoritative only for the Mojang provider; major
compatibility remains the final gate enforced by the post-install probe.
