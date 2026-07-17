const std = @import("std");

pub const VersionType = enum {
    release,
    snapshot,
    old_beta,
    old_alpha,

    pub fn parse(value: []const u8) ?VersionType {
        inline for (std.meta.fields(VersionType)) |field| {
            if (std.mem.eql(u8, value, field.name)) return @enumFromInt(field.value);
        }
        return null;
    }
};

pub const Latest = struct {
    release: []const u8,
    snapshot: []const u8,
};

pub const Version = struct {
    id: []const u8,
    type: VersionType,
    url: []const u8,
    time: []const u8,
    releaseTime: []const u8,
    sha1: ?[]const u8 = null,
    complianceLevel: ?u8 = null,
};

pub const Manifest = struct {
    latest: Latest,
    versions: []const Version,
};

pub const ListOptions = struct {
    kind: ?VersionType = null,
    limit: usize = 20,

    pub fn includes(options: ListOptions, version: Version) bool {
        return options.kind == null or options.kind.? == version.type;
    }
};

pub fn parse(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Manifest) {
    return std.json.parseFromSlice(Manifest, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
    });
}

pub fn writeSelection(
    json: *std.json.Stringify,
    manifest: Manifest,
    options: ListOptions,
) std.Io.Writer.Error!void {
    try json.beginObject();
    try json.objectField("latest");
    try json.write(manifest.latest);
    try json.objectField("versions");
    try json.beginArray();

    var written: usize = 0;
    for (manifest.versions) |version| {
        if (!options.includes(version)) continue;
        if (written == options.limit) break;
        try json.write(version);
        written += 1;
    }

    try json.endArray();
    try json.endObject();
}

test "parses and filters a Mojang version manifest" {
    const source =
        \\{
        \\  "latest": {"release": "1.21.6", "snapshot": "25w20a"},
        \\  "versions": [
        \\    {
        \\      "id": "1.21.6",
        \\      "type": "release",
        \\      "url": "https://example.invalid/1.21.6.json",
        \\      "time": "2025-06-17T12:00:00+00:00",
        \\      "releaseTime": "2025-06-17T12:00:00+00:00",
        \\      "sha1": "0123456789abcdef",
        \\      "complianceLevel": 1,
        \\      "ignored": true
        \\    },
        \\    {
        \\      "id": "25w20a",
        \\      "type": "snapshot",
        \\      "url": "https://example.invalid/25w20a.json",
        \\      "time": "2025-05-15T12:00:00+00:00",
        \\      "releaseTime": "2025-05-15T12:00:00+00:00"
        \\    }
        \\  ]
        \\}
    ;

    var parsed = try parse(std.testing.allocator, source);
    defer parsed.deinit();

    try std.testing.expectEqualStrings("1.21.6", parsed.value.latest.release);
    try std.testing.expectEqual(VersionType.release, parsed.value.versions[0].type);
    try std.testing.expectEqual(@as(?u8, 1), parsed.value.versions[0].complianceLevel);

    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var json: std.json.Stringify = .{ .writer = &output.writer };
    try writeSelection(&json, parsed.value, .{ .kind = .snapshot, .limit = 1 });

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "25w20a") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "1.21.6.json") == null);
}

test "version type parser rejects unknown values" {
    try std.testing.expectEqual(VersionType.old_alpha, VersionType.parse("old_alpha").?);
    try std.testing.expect(VersionType.parse("custom") == null);
}
