# Server create source validation order

Implementation checkpoint: `beff5d41ceb28c98e87c86e6d6f8da0fffb699ed`.
Pinned original: `5a4310debbcee26b3d2391bab2339f79cd6ec52e`.

The CLI stored source options in a Map and validated only after parsing, allowing
an invalid earlier source to be hidden by a later official/bmclapi value. A pure
red test reproduced this. Each source is now validated at its original argument
position before insertion, as in pinned Zig. Valid repeated sources still use the
last value. Existing final parameter validation remains.

One pure test covers four rejected and two accepted repetitions. It and the three
preceding property-input regressions passed. Offline locked build, all-target
Clippy with warnings denied, fmt and static audit tests passed (22 normal and 22
optimized). Only Linux ran. No create function, network resolution, filesystem,
EULA acceptance, Java or server operation was called by the tests. No live or
native non-Linux acceptance is claimed, and no remote branch was modified.
