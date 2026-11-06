const std = @import("std");
const mrpack = @import("core").mrpack;
test "pure pack environment bytes" {
    const prefix = "{\"formatVersion\":1,\"game\":\"minecraft\",\"name\":\"fixture\",\"dependencies\":{\"minecraft\":\"1.20.1\"},\"files\":[{\"path\":\"mods/a.jar\",\"hashes\":{\"sha1\":\"1111111111111111111111111111111111111111\"},\"downloads\":[\"https://example.com/a.jar\"],\"fileSize\":1,\"env\":{\"client\":";
    const suffix = "}}]}";
    const raw = prefix ++ "[117,110,115,117,112,112,111,114,116,101,100]" ++ suffix;
    const parsed = try mrpack.parse(std.testing.allocator, raw);
    defer parsed.deinit();
    try std.testing.expect(!mrpack.shipsToClient(parsed.value.files[0]));
    try std.testing.expectError(error.InvalidModpackEnv, mrpack.parse(std.testing.allocator, prefix ++ "[255]" ++ suffix));
    const invalid_size = try std.mem.replaceOwned(u8, std.testing.allocator, prefix ++ "[255]" ++ suffix, "\"fileSize\":1", "\"fileSize\":0");
    defer std.testing.allocator.free(invalid_size);
    try std.testing.expectError(error.InvalidModpackFileSize, mrpack.parse(std.testing.allocator, invalid_size));
}
