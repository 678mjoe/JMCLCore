# Pack discriminator typed bytes

Continuation from 3095e31. Mrpack.game and Curseforge.manifestType now decode
typed bytes and compare exact byte constants at their existing validation sites.
They are not passed to paths, transports or downstream output.

The new pure regression failed before this change. It covers accepted byte-array
spellings, unsupported opaque bytes, Mrpack format error before game error,
and Curseforge kind error before invalid name. The pinned actual Mrpack and
Curseforge parser fixture confirms the same outcomes.

Nine schema tests, the Zig pure fixture, formatting, all-target Clippy and
28 static guards in each Python mode pass. No archive, installation, Java,
network execution, remote write or bounded CI expansion is included.
