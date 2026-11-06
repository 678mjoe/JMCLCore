const std = @import("std");
test "pure display bytes join and JSON representation" {
    const Fields = struct { name: []const u8, version: ?[]const u8 = null };
    const a = std.testing.allocator;
    const parsed = try std.json.parseFromSlice(Fields, a, "{\"name\":[65,255],\"version\":[66,254]}", .{});
    defer parsed.deinit();
    const display = try std.fmt.allocPrint(a, "{s} {s}", .{parsed.value.name, parsed.value.version.?});
    defer a.free(display);
    try std.testing.expectEqualSlices(u8, "A\xff B\xfe", display);
    const encoded = try std.json.Stringify.valueAlloc(a, display, .{});
    defer a.free(encoded);
    try std.testing.expectEqualStrings("[65,255,32,66,254]", encoded);
    const nul = try std.json.Stringify.valueAlloc(a, @as([]const u8,"A\x00"), .{});
    defer a.free(nul);
    try std.testing.expectEqualStrings("\"A\\u0000\"", nul);
}
