const std = @import("std");
const cf = @import("curseforge");
test "pure provider byte fields" {
    const valid = "{\"data\":{\"id\":1,\"modId\":1,\"fileName\":\"fixture.jar\",\"releaseType\":1,\"hashes\":[{\"value\":\"1111111111111111111111111111111111111111\",\"algo\":1},{\"value\":[255,0],\"algo\":2}],\"fileDate\":\"\",\"fileLength\":1,\"gameVersions\":[[255,0]]}}";
    const parsed = try cf.parseFile(std.testing.allocator, valid);
    defer parsed.deinit();
    try std.testing.expectEqualSlices(u8, &.{255,0}, parsed.value.data.gameVersions[0]);
    try std.testing.expectEqualStrings("1111111111111111111111111111111111111111", cf.sha1Of(parsed.value.data.*));
    const invalid = try std.mem.replaceOwned(u8, std.testing.allocator, valid, "\"algo\":2", "\"algo\":1");
    defer std.testing.allocator.free(invalid);
    try std.testing.expectError(error.InvalidCurseforgeHash, cf.parseFile(std.testing.allocator, invalid));
    const empty = try std.mem.replaceOwned(u8, std.testing.allocator, valid, "[[255,0]]", "[[]]");
    defer std.testing.allocator.free(empty);
    try std.testing.expectError(error.InvalidCurseforgeGameVersions, cf.parseFile(std.testing.allocator, empty));
}
