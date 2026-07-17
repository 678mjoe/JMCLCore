const std = @import("std");
const version_metadata = @import("version_metadata.zig");

pub const Object = struct {
    hash: []const u8,
    size: u64,
};

pub const ObjectMap = std.json.ArrayHashMap(Object);

pub const Index = struct {
    objects: ObjectMap,
    virtual: bool = false,
    map_to_resources: bool = false,
};

pub fn parse(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Index) {
    return std.json.parseFromSlice(Index, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
    });
}

pub fn verifySha1(source: []const u8, expected: []const u8) bool {
    return version_metadata.verifySha1(source, expected);
}

test "parses modern and legacy asset index fields" {
    const source =
        \\{
        \\  "objects": {
        \\    "minecraft/sounds/example.ogg": {
        \\      "hash": "0123456789012345678901234567890123456789",
        \\      "size": 42
        \\    },
        \\    "icons/icon.png": {
        \\      "hash": "abcdefabcdefabcdefabcdefabcdefabcdefabcd",
        \\      "size": 24
        \\    }
        \\  },
        \\  "virtual": true,
        \\  "map_to_resources": true,
        \\  "unknown": "ignored"
        \\}
    ;

    var parsed = try parse(std.testing.allocator, source);
    defer parsed.deinit();

    try std.testing.expectEqual(@as(usize, 2), parsed.value.objects.map.count());
    try std.testing.expect(parsed.value.virtual);
    try std.testing.expect(parsed.value.map_to_resources);
    try std.testing.expectEqual(
        @as(u64, 42),
        parsed.value.objects.map.get("minecraft/sounds/example.ogg").?.size,
    );
}

test "asset index SHA-1 verification uses official digest" {
    try std.testing.expect(verifySha1("abc", "a9993e364706816aba3e25717850c26c9cd0d89d"));
}
