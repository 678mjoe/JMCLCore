# Pack hash bytes through pure planning

Continuation from 708c300. Mrpack SHA1 and optional SHA512 fields now retain
typed bytes. File.validate still checks SHA1 then SHA512 at the original point,
after path checks and before file size, URLs and environment checks.

Downloads::mrpack receives a validated Mrpack, as the production inspect path
already requires. It converts validated ASCII SHA1 without loss and reuses it
for both the download record and registry entry. The pre-existing validated
input requirement is now documented; this does not introduce validation of
arbitrary hand-built packs or change the existing downloads[0] assumption.

Both new tests failed before the patch. Pure tests now cover uppercase SHA1
and SHA512 array decoding, both planned SHA1 outputs, server-only skipping
after schema validation, path/hash/size error precedence, and invalid typed
byte values. The pinned original Mrpack pure parser fixture verifies array
hashes and matching invalid-hash/path errors. No plan is executed.

Six schema tests, the Zig pure fixture, formatting, all-target Clippy and 28
static guards in each Python mode pass. No network, Java, archive execution,
installation, remote publication or bounded CI expansion occurs.
