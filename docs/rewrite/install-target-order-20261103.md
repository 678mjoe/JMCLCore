# Install CLI target validation order

Implementation checkpoint: `be9f73445d739e5a13286498754d6df1b2540d8c`.
Pinned original: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

The String-based execute/prepare parser previously validated only the final OS
and architecture selection. Invalid or empty earlier values could be hidden by
later valid options. A pure red regression confirmed execute accepted bad then
linux. Each occurrence now validates immediately; architecture aliases remain
normalized, valid repetitions still take the final value, and final defensive
selection validation remains. No execution controller was changed.

One pure parser test covers 12 invalid and 2 valid sequences. All 23 vanilla
regressions pass, and 12 malformed CLI sequences match the pinned original's
complete output and status with no writes or transport construction.

The latest binary was then checked against all seven new finite scripts:
16 prepare schema + 9 prepare argv + 24 execute argv + 39 instance read + 30
pure top-level diagnostics + 4 server property syntax + 12 install target order
= 134 exact paired cases. Only the two explicitly owned instance-missing-root
cases create their expected empty directory; all other trees remain unchanged.
Latest Linux executable SHA256:
`db67310f00a1af0bf31b049b9aa0afb1ba563071708a30e2fd9d0afc3795396b`.

Offline locked build, all-target warning-denied Clippy, fmt and static audit tests
pass (22 normal, 22 optimized). Source audit still counts 106 names, 105 static
associations and one explicitly blocked java.runtime.install, with zero full
compatibility claims and replacement_allowed=false.

Only x86_64-unknown-linux-gnu is installed in this environment. This record does
not claim native Windows/macOS acceptance, live provider/account behavior,
real server lifecycle/properties publication, Minecraft launch, or the restricted
Java acquisition/archive work. No remote publication or production-main change
was performed. A bounded final source review found no further confirmed pure
parameter defect, which is not proof that no defects remain.
