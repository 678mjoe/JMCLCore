# Server CLI pure numeric grammar

Implementation checkpoint: `bbb4d08defa7a823169ecac4a462bcd1976fb16a`.
Pinned original: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

Server CLI options previously used Rust integer parsing, wrongly rejecting
internal underscores and negative zero accepted by Zig. Two pure red tests
confirmed failures for lifecycle/log and installation arguments. Both parsers
now reuse the independently verified unsigned helper, preserving immediate
validation, grace <=120000, log max bytes 1..1048576, complete u64 cursor/file ID,
workers 1..32 and retries 0..5. Conversion to u8 remains checked.

17 pure lifecycle/log cases and 11 pure installation cases cover accepted syntax,
boundaries, overflow and invalid-earlier duplicates. Four aggregate numeric tests
passed including the preceding store/version tests. All tests call only options,
parse or the pure helper: no service start/stop/restart, logs, installation, EULA
acceptance, Java, GC or network operation ran. Installation fixtures deliberately
leave accept_eula false.

Offline locked build, all-target warning-denied Clippy, fmt and static audit tests
passed (22 normal, 22 optimized). Only Linux execution is established. The pure
installation parser is Linux-gated; no other platform result is implied. No remote
publication or production replacement occurred.
