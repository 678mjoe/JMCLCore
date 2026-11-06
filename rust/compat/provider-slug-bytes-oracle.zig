const std = @import("std");
const cf = @import("curseforge");
test "pure provider slug bytes" {
    const valid = "{\"data\":{\"id\":1,\"slug\":[97,45,90,95,57,46],\"name\":\"\",\"classId\":6}}";
    const parsed = try cf.parseMod(std.testing.allocator, valid);
    defer parsed.deinit();
    try std.testing.expectEqualStrings("a-Z_9.", parsed.value.data.slug);
    try std.testing.expectError(error.InvalidCurseforgeSlug, cf.parseMod(std.testing.allocator,
        "{\"data\":{\"id\":1,\"slug\":[255],\"name\":\"\",\"classId\":6}}"));
    try std.testing.expectError(error.InvalidCurseforgeId, cf.parseMod(std.testing.allocator,
        "{\"data\":{\"id\":0,\"slug\":[255],\"name\":\"\",\"classId\":6}}"));
}
