const std = @import("std");
const properties = @import("properties");
test "pure precedence corpus" {
    const long_key = [_]u8{'k'} ** 257;
    const keys = [_][]const u8{ "", &long_key, "bad\\nkey", "bad\\u0000key", "\xff" };
    for (keys) |key| {
        const input = try std.mem.concat(std.testing.allocator, u8, &.{key, "=\xff\n"});
        defer std.testing.allocator.free(input);
        try std.testing.expectError(error.InvalidServerPropertyKey, properties.parse(std.testing.allocator, input));
        const bad_escape = try std.mem.concat(std.testing.allocator, u8, &.{key, "=\\uZZZZ\n"});
        defer std.testing.allocator.free(bad_escape);
        try std.testing.expectError(error.InvalidServerProperties, properties.parse(std.testing.allocator, bad_escape));
    }
    try std.testing.expectError(error.InvalidServerPropertyValue, properties.parse(std.testing.allocator, "good=\xff\n"));
}
