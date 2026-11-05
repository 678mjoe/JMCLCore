const std = @import("std");
test "pure typed number tokens preserve binary128 precision" {
    inline for (.{ u8, u32, u64 }) |T| {
        try std.testing.expectError(error.InvalidNumber, std.json.parseFromSlice(T, std.testing.allocator, "1.0000000000000001", .{}));
        const rounded = try std.json.parseFromSlice(T, std.testing.allocator, "1.00000000000000000000000000000000001", .{});
        defer rounded.deinit();
        try std.testing.expectEqual(@as(T, 1), rounded.value);
    }
    const wide = try std.json.parseFromSlice(u64, std.testing.allocator, "9007199254740993.0", .{});
    defer wide.deinit();
    try std.testing.expectEqual(@as(u64, 9007199254740993), wide.value);
    const maximum = try std.json.parseFromSlice(u64, std.testing.allocator, "18446744073709551615.0", .{});
    defer maximum.deinit();
    try std.testing.expectEqual(std.math.maxInt(u64), maximum.value);
    try std.testing.expectError(error.InvalidNumber, std.json.parseFromSlice(u64, std.testing.allocator, "18446744073709551615.1", .{}));
}
