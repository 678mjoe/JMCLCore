const std = @import("std");
const install_plan = @import("install_plan.zig");
const modrinth = @import("modrinth.zig");

pub const schema_version: u32 = 1;
pub const relative_path = ".jmcl/content.json";
pub const max_registry_bytes: usize = 256 * 1024;
pub const max_entries: usize = 1024;
pub const max_entry_size: u64 = 4 * 1024 * 1024 * 1024;

pub const Provider = enum {
    modrinth,
    manual,
};

pub const Source = struct {
    provider: Provider,
    project_id: ?[]const u8 = null,
    version_id: ?[]const u8 = null,
    slug: ?[]const u8 = null,
};

pub const Kind = enum {
    mod,
};

pub const Entry = struct {
    kind: Kind,
    file: []const u8,
    sha1: []const u8,
    size: u64,
    source: Source,
    enabled: bool = true,
    as_dependency: bool = false,
};

pub const Registry = struct {
    schema_version: u32 = schema_version,
    entries: []Entry = &.{},
};

pub const Document = std.json.Parsed(Registry);

pub fn load(allocator: std.mem.Allocator, io: std.Io, minecraft_dir: std.Io.Dir) !Document {
    const bytes = minecraft_dir.readFileAlloc(
        io,
        relative_path,
        allocator,
        .limited(max_registry_bytes + 1),
    ) catch |err| switch (err) {
        error.FileNotFound => return parse(allocator, "{\"schema_version\":1,\"entries\":[]}"),
        error.StreamTooLong => return error.ContentRegistryTooLarge,
        else => |other| return other,
    };
    defer allocator.free(bytes);
    if (bytes.len > max_registry_bytes) return error.ContentRegistryTooLarge;
    return parse(allocator, bytes);
}

pub fn parse(allocator: std.mem.Allocator, source: []const u8) !Document {
    var document = try std.json.parseFromSlice(Registry, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_registry_bytes,
    });
    errdefer document.deinit();
    try validate(document.value);
    return document;
}

pub fn save(io: std.Io, minecraft_dir: std.Io.Dir, registry: Registry) !void {
    try validate(registry);
    var atomic = try minecraft_dir.createFileAtomic(io, relative_path, .{ .make_path = true, .replace = true });
    defer atomic.deinit(io);
    var buffer: [4096]u8 = undefined;
    var writer: std.Io.File.Writer = .init(atomic.file, io, &buffer);
    var json: std.json.Stringify = .{ .writer = &writer.interface };
    try json.write(registry);
    try writer.interface.writeByte('\n');
    try writer.interface.flush();
    try atomic.file.sync(io);
    try atomic.replace(io);
}

pub fn findByFile(registry: Registry, file: []const u8) ?Entry {
    for (registry.entries) |entry| {
        if (std.mem.eql(u8, entry.file, file)) return entry;
    }
    return null;
}

pub fn findByProject(registry: Registry, provider: Provider, project_id: []const u8) ?Entry {
    for (registry.entries) |entry| {
        if (entry.source.provider != provider) continue;
        const candidate = entry.source.project_id orelse continue;
        if (std.mem.eql(u8, candidate, project_id)) return entry;
    }
    return null;
}

fn validate(registry: Registry) !void {
    if (registry.schema_version != schema_version) return error.UnsupportedContentSchema;
    if (registry.entries.len > max_entries) return error.TooManyContentEntries;
    for (registry.entries, 0..) |entry, index| {
        try install_plan.validateRelativePath(entry.file);
        if (entry.file.len > 512) return error.InvalidContentFile;
        try validateSha1(entry.sha1);
        if (entry.size == 0 or entry.size > max_entry_size) return error.InvalidContentSize;
        try validateSource(entry.source);
        for (registry.entries[index + 1 ..]) |other| {
            if (std.mem.eql(u8, entry.file, other.file)) return error.DuplicateContentFile;
            if (entry.source.provider == other.source.provider) {
                if (entry.source.project_id != null and other.source.project_id != null and
                    std.mem.eql(u8, entry.source.project_id.?, other.source.project_id.?))
                {
                    return error.DuplicateContentProject;
                }
            }
        }
    }
}

fn validateSource(source: Source) !void {
    switch (source.provider) {
        .modrinth => {
            try modrinth.validateVersionId(source.project_id orelse return error.InvalidContentSource);
            try modrinth.validateVersionId(source.version_id orelse return error.InvalidContentSource);
            if (source.slug) |slug| try modrinth.validateProjectRef(slug);
        },
        .manual => {
            if (source.project_id != null or source.version_id != null or source.slug != null) {
                return error.InvalidContentSource;
            }
        },
    }
}

fn validateSha1(value: []const u8) !void {
    if (value.len != 40) return error.InvalidContentSha1;
    for (value) |byte| if (!std.ascii.isHex(byte)) return error.InvalidContentSha1;
}

test "round-trips an empty and a populated registry" {
    const io = std.testing.io;
    const allocator = std.testing.allocator;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();

    var empty = try load(allocator, io, temporary.dir);
    defer empty.deinit();
    try std.testing.expectEqual(@as(usize, 0), empty.value.entries.len);

    const registry: Registry = .{
        .entries = @constCast(&[_]Entry{
            .{
                .kind = .mod,
                .file = "mods/sodium.jar",
                .sha1 = "1111111111111111111111111111111111111111",
                .size = 100,
                .source = .{
                    .provider = .modrinth,
                    .project_id = "AANobbMI",
                    .version_id = "XyZaBcDe",
                    .slug = "sodium",
                },
                .as_dependency = false,
            },
            .{
                .kind = .mod,
                .file = "mods/manual.jar",
                .sha1 = "2222222222222222222222222222222222222222",
                .size = 200,
                .source = .{ .provider = .manual },
                .enabled = false,
                .as_dependency = false,
            },
        }),
    };
    try save(io, temporary.dir, registry);

    var loaded = try load(allocator, io, temporary.dir);
    defer loaded.deinit();
    try std.testing.expectEqual(@as(usize, 2), loaded.value.entries.len);
    try std.testing.expectEqualStrings("sodium.jar", std.fs.path.basename(loaded.value.entries[0].file));
    try std.testing.expectEqualStrings("AANobbMI", loaded.value.entries[0].source.project_id.?);
    try std.testing.expect(!loaded.value.entries[1].enabled);
    try std.testing.expect(findByProject(loaded.value, .modrinth, "AANobbMI") != null);
    try std.testing.expect(findByProject(loaded.value, .modrinth, "ZZZZZZZZ") == null);
    try std.testing.expect(findByFile(loaded.value, "mods/manual.jar") != null);
}

test "rejects invalid registry documents" {
    const allocator = std.testing.allocator;
    const duplicate_file =
        \\{"schema_version":1,"entries":[
        \\{"kind":"mod","file":"mods/a.jar","sha1":"1111111111111111111111111111111111111111","size":1,"source":{"provider":"manual"}},
        \\{"kind":"mod","file":"mods/a.jar","sha1":"2222222222222222222222222222222222222222","size":2,"source":{"provider":"manual"}}
        \\]}
    ;
    try std.testing.expectError(error.DuplicateContentFile, parse(allocator, duplicate_file));

    const unsafe_path =
        \\{"schema_version":1,"entries":[
        \\{"kind":"mod","file":"../escape.jar","sha1":"1111111111111111111111111111111111111111","size":1,"source":{"provider":"manual"}}
        \\]}
    ;
    try std.testing.expectError(error.UnsafePath, parse(allocator, unsafe_path));

    const manual_with_ids =
        \\{"schema_version":1,"entries":[
        \\{"kind":"mod","file":"mods/a.jar","sha1":"1111111111111111111111111111111111111111","size":1,"source":{"provider":"manual","project_id":"AANobbMI"}}
        \\]}
    ;
    try std.testing.expectError(error.InvalidContentSource, parse(allocator, manual_with_ids));

    const future_schema =
        \\{"schema_version":99,"entries":[]}
    ;
    try std.testing.expectError(error.UnsupportedContentSchema, parse(allocator, future_schema));
}
