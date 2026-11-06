const std = @import("std");
const mrpack = @import("core").mrpack;
test "pure pack hash bytes" {
    const raw = "{\"formatVersion\":1,\"game\":\"minecraft\",\"name\":\"fixture\",\"dependencies\":{\"minecraft\":\"1.20.1\"},\"files\":[{\"path\":\"mods/a.jar\",\"hashes\":{\"sha1\":[" ++ "65," ** 39 ++ "65],\"sha512\":[" ++ "102," ** 127 ++ "102]},\"downloads\":[\"https://example.com/a.jar\"],\"fileSize\":1}]}";
    const parsed = try mrpack.parse(std.testing.allocator, raw);
    defer parsed.deinit();
    try std.testing.expectEqualStrings("A" ** 40, parsed.value.files[0].hashes.sha1);
    try std.testing.expectEqualStrings("f" ** 128, parsed.value.files[0].hashes.sha512.?);
    const bad = try std.mem.replaceOwned(u8, std.testing.allocator, raw, "65", "255");
    defer std.testing.allocator.free(bad);
    try std.testing.expectError(error.InvalidModrinthHash, mrpack.parse(std.testing.allocator, bad));
    const bad_path = try std.mem.replaceOwned(u8, std.testing.allocator, bad, "mods/a.jar", "../a.jar");
    defer std.testing.allocator.free(bad_path);
    try std.testing.expectError(error.UnsafePath, mrpack.parse(std.testing.allocator, bad_path));
}
