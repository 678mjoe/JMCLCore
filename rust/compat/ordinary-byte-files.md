# Isolated ordinary-file execution capability

ReadPlan::ordinary_files is pure and returns no capability unless loader is
`none` and both native-extraction and projection lists are empty. Its capability
contains only the existing ByteFile view and declared total. Execute delegates
to install::execute_bytes; there is no second scheduler, downloader, event or CAS
implementation. The shared executor, public String wrapper, store transactions,
install RPC, processor Java, receipt writing and launch code are unchanged.

This internal seam has no install RPC routing. Only owned synthetic fixtures use
it. A filename ending in .jar is still uninterpreted payload; no archive parser
is called. File execution does not write install.json or establish installed state.
Digest locks remain visible under the accepted layout difference. Failed size or
hash checks retain the lock and created empty hash-prefix directory; staged
payloads are removed by the existing transaction cleanup, not by fixture masking.

Fixtures are Linux-only. Unix raw bytes are not a claim that macOS filesystems
accept every byte; Windows already has a separate UTF-8/WTF-8 path conversion
boundary which rejects arbitrary Unix byte strings. The adapter adds no native
conversion policy. Windows/macOS native verification, Zig execution, receipt and
full installation integration remain separate. No URL policy or CAS lock change.
