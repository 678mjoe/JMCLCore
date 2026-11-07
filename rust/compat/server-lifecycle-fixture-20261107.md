# Linux server lifecycle fixture acceptance

## Test-only isolation

The two existing `server_lifecycle` tests
`detached_start_command_status_restart_stop` and
`stop_escalates_only_verified_synthetic_children` retain their original major-21
fixture by default. Setting `JMCL_TEST_SERVER_FIXTURE_MAJOR=99` enables an
explicit, Linux x86_64 acceptance mode; other configured values fail.

This mode changes only the owned shell fixture's reported specification/version
and its owned installation receipt to major 99. Production detection, selection,
server commands and lifecycle code are unchanged. The receipt's `server.jar`
contains six literal bytes, `benign`; it is not an archive or Minecraft server.

Before starting, the test calls the actual `discover` and bounded `probe`
functions. The explicit fixture must be first and report major 99; every other
candidate must successfully report major 21. Actual selection must return the
explicit fixture. Removing it from the resulting in-memory detection must yield
`NoCompatibleJava` for major 99. The production selector requires exact major and
architecture equality, not a minimum version. This avoids a fallback to the
installed major-21 JVM if the fixture later fails to probe.

These checks **execute installed JVM version/property probes**. They do not
provide a no-Java-execution test. The probe arguments are exactly
`-XshowSettings:properties -version`, with a 10-second per-candidate timeout and
separate 256 KiB stdout/stderr limits. Production probe removes the three Java
options variables, but otherwise inherits the supplied environment. The external
runner therefore supplies only PATH, locale, timezone, TMPDIR and the opt-in.
It does not modify system directories or hide platform candidates.

The reviewed executor has three executable aliases under `/usr/lib/jvm`, all
resolving to the same installed OpenJDK 21. Their paths, resolved executable,
release metadata and file hashes are checked before running. Production RPC
repeats discovery/probing after test preflight. This relies on that stable,
non-adversarial executor state; it is not a general isolation boundary for an
arbitrary machine with a different or concurrently changed candidate set.

## Execution and cleanup contract

Run the built test binary by absolute path, one exact test at a time, with
`--exact --nocapture --test-threads=1`. The minimal process environment has
`PATH=/usr/local/sbin` (checked to have no `java`), `LANG=C`, `LC_ALL=C`, `TZ=UTC`,
a dedicated owned `TMPDIR`, and `JMCL_TEST_SERVER_FIXTURE_MAJOR=99`. It does not
inherit JAVA_HOME, Java option variables, CLASSPATH, LD variables or shell startup
variables. System Java directories remain visible.

Every successful start/restart must return the canonical owned fixture path.
The persisted state, returned PID, production process-identity check and actual
`/proc/PID/cmdline` must agree. The supervisor invokes that exact path with the
random identity argument, `-jar server.jar nogui`; the executable is the owned
shell fixture, not the installed JVM. Normal tests assert stopped status and
PID-record removal. Fixture Drop attempts production stop on normal or panic
unwind. On cleanup error it reports and preserves its temporary directory.

The external runner additionally waits for each observed child and supervisor
to have no live command line matching that owned fixture root, and checks that
the dedicated temporary directory is empty before proceeding. This is a live
owned-process check, not a claim that the operating system has immediately
reaped every exited process.

Do not apply a timeout that simply kills the test: the supervisor detaches.
If a test hangs, inspect the retained owned root and state, verify the exact
fixture path, PID identity and supervisor command before using production stop
for that fixture. Do not delete its directory first or kill by process name.
A failure, unknown candidate, incorrect selected path or remaining owned process
stops the next test and requires targeted inspection/cleanup.

No runtime download, installation, archive processing, real Minecraft server,
network-provider request, Tauri interaction or other-platform execution is part
of these two tests. This file describes the isolation and execution contract;
run-specific identities and results belong in a separate dated evidence record.
