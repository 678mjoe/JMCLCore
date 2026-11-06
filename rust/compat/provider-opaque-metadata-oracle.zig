const std = @import("std");
test "pure typed byte metadata accepts opaque bytes" {
    const Field = struct { name: []const u8 };
    for ([_][]const u8{ "{\"name\":[]}", "{\"name\":[65,255,0]}", "{\"name\":[\"6_5\",66.0]}" }) |raw| {
        const parsed = try std.json.parseFromSlice(Field, std.testing.allocator, raw, .{});
        defer parsed.deinit();
    }
    try std.testing.expectError(error.Overflow, std.json.parseFromSlice(Field, std.testing.allocator, "{\"name\":[256]}", .{}));
    try std.testing.expectError(error.InvalidNumber, std.json.parseFromSlice(Field, std.testing.allocator, "{\"name\":[65.00000000000001]}", .{}));
}
