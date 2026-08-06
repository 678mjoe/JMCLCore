const std = @import("std");
const nbt = @import("nbt.zig");

pub const max_world_name_bytes: usize = 255;
pub const max_worlds: usize = 1024;
pub const max_level_dat_bytes: usize = 32 * 1024 * 1024;
pub const max_icon_bytes: usize = 64 * 1024;
pub const saves_directory_name = "saves";
pub const level_dat_name = "level.dat";
pub const icon_name = "icon.png";
pub const session_lock_name = "session.lock";
pub const copy_marker = ".jmcl-copy-";
pub const trash_marker = ".jmcl-trash-";

pub const GameMode = enum {
    survival,
    creative,
    adventure,
    spectator,
};

pub const LevelMeta = struct {
    level_name: ?[]const u8 = null,
    version_name: ?[]const u8 = null,
    version_id: ?i32 = null,
    game_mode: ?GameMode = null,
    hardcore: ?bool = null,
    cheats: ?bool = null,
    difficulty: ?u8 = null,
    last_played_ms: ?i64 = null,
};

pub const WorldInfo = struct {
    name: []const u8,
    meta: ?LevelMeta,
    size_bytes: u64,
    has_icon: bool,
    locked: bool,
};

pub const List = struct {
    arena: *std.heap.ArenaAllocator,
    worlds: []WorldInfo,

    pub fn deinit(list_self: *List) void {
        const allocator = list_self.arena.child_allocator;
        list_self.arena.deinit();
        allocator.destroy(list_self.arena);
        list_self.* = undefined;
    }
};

pub fn validateName(name: []const u8) !void {
    if (name.len == 0 or name.len > max_world_name_bytes) return error.InvalidName;
    if (std.mem.eql(u8, name, ".") or std.mem.eql(u8, name, "..")) return error.InvalidName;
    if (name[0] == '.') return error.InvalidName;
    if (name[name.len - 1] == ' ' or name[name.len - 1] == '.') return error.InvalidName;
    for (name) |byte| {
        if (byte == 0 or byte == '/' or byte == '\\' or byte < 0x20) return error.InvalidName;
    }
}

pub fn isTempName(name: []const u8) bool {
    return std.mem.indexOf(u8, name, copy_marker) != null or
        std.mem.indexOf(u8, name, trash_marker) != null;
}

fn tempName(buffer: []u8, marker: []const u8, name: []const u8, random: u64) ![]const u8 {
    return std.fmt.bufPrint(buffer, "{s}{s}{x:0>16}", .{ marker, name, random }) catch
        return error.InvalidName;
}

pub fn cleanupTemps(io: std.Io, saves_dir: std.Io.Dir) void {
    var iterator = saves_dir.iterate();
    while (iterator.next(io) catch null) |entry| {
        if (entry.kind != .directory) continue;
        if (!isTempName(entry.name)) continue;
        saves_dir.deleteTree(io, entry.name) catch {};
    }
}

fn statOptional(io: std.Io, dir: std.Io.Dir, path: []const u8) bool {
    _ = dir.statFile(io, path, .{}) catch return false;
    return true;
}

fn gunzipAlloc(allocator: std.mem.Allocator, compressed: []const u8) ![]u8 {
    var input: std.Io.Reader = .fixed(compressed);
    var window: [std.compress.flate.max_window_len]u8 = undefined;
    var decompress = std.compress.flate.Decompress.init(&input, .gzip, &window);
    return decompress.reader.allocRemaining(allocator, .limited(nbt.max_payload_bytes));
}

fn parseLevelMeta(arena: std.mem.Allocator, io: std.Io, world_dir: std.Io.Dir) ?LevelMeta {
    const compressed = world_dir.readFileAlloc(io, level_dat_name, arena, .limited(max_level_dat_bytes + 1)) catch return null;
    if (compressed.len > max_level_dat_bytes) return null;
    const payload = gunzipAlloc(arena, compressed) catch return null;
    const document = nbt.parse(arena, payload) catch return null;
    const data = nbt.findCompound(document.root, "Data") orelse return null;

    var meta: LevelMeta = .{};
    meta.level_name = nbt.findString(data, "LevelName");
    meta.last_played_ms = nbt.findLong(data, "LastPlayed");
    meta.hardcore = if (nbt.findByte(data, "hardcore")) |value| value != 0 else null;
    meta.cheats = if (nbt.findByte(data, "allowCommands")) |value| value != 0 else null;
    meta.difficulty = if (nbt.findByte(data, "Difficulty")) |value| @bitCast(value) else null;
    meta.game_mode = if (nbt.findInt(data, "GameType")) |value| switch (value) {
        0 => .survival,
        1 => .creative,
        2 => .adventure,
        3 => .spectator,
        else => null,
    } else null;
    if (nbt.findCompound(data, "Version")) |version| {
        meta.version_name = nbt.findString(version, "Name");
        meta.version_id = nbt.findInt(version, "Id");
    }
    return meta;
}

fn sizeRecursive(io: std.Io, dir: std.Io.Dir) u64 {
    var total: u64 = 0;
    var iterator = dir.iterate();
    while (iterator.next(io) catch null) |entry| {
        switch (entry.kind) {
            .file => {
                const stat = dir.statFile(io, entry.name, .{}) catch continue;
                total += stat.size;
            },
            .directory => {
                var child = dir.openDir(io, entry.name, .{ .iterate = true }) catch continue;
                defer child.close(io);
                total += sizeRecursive(io, child);
            },
            else => continue,
        }
    }
    return total;
}

fn worldInfo(arena: std.mem.Allocator, io: std.Io, saves_dir: std.Io.Dir, name: []const u8) !WorldInfo {
    var world_dir = try saves_dir.openDir(io, name, .{ .iterate = true });
    defer world_dir.close(io);
    return .{
        .name = try arena.dupe(u8, name),
        .meta = parseLevelMeta(arena, io, world_dir),
        .size_bytes = sizeRecursive(io, world_dir),
        .has_icon = statOptional(io, world_dir, icon_name),
        .locked = statOptional(io, world_dir, session_lock_name),
    };
}

pub fn list(backing: std.mem.Allocator, io: std.Io, saves_dir: std.Io.Dir) !List {
    const arena = try backing.create(std.heap.ArenaAllocator);
    errdefer backing.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(backing);
    errdefer arena.deinit();
    const result_allocator = arena.allocator();

    var worlds: std.ArrayList(WorldInfo) = .empty;
    var iterator = saves_dir.iterate();
    while (try iterator.next(io)) |entry| {
        if (entry.kind != .directory) continue;
        if (isTempName(entry.name)) continue;
        validateName(entry.name) catch continue;
        if (worlds.items.len >= max_worlds) return error.TooManyWorlds;
        const info = worldInfo(result_allocator, io, saves_dir, entry.name) catch continue;
        try worlds.append(result_allocator, info);
    }
    std.mem.sort(WorldInfo, worlds.items, {}, struct {
        fn lessThan(_: void, a: WorldInfo, b: WorldInfo) bool {
            return std.mem.order(u8, a.name, b.name) == .lt;
        }
    }.lessThan);
    return .{ .arena = arena, .worlds = try worlds.toOwnedSlice(result_allocator) };
}

pub fn emptyList(backing: std.mem.Allocator) !List {
    const arena = try backing.create(std.heap.ArenaAllocator);
    errdefer backing.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(backing);
    return .{ .arena = arena, .worlds = &.{} };
}

pub fn collect(backing: std.mem.Allocator, io: std.Io, parent_dir: std.Io.Dir, names: []const []const u8) !List {
    const arena = try backing.create(std.heap.ArenaAllocator);
    errdefer backing.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(backing);
    errdefer arena.deinit();
    const result_allocator = arena.allocator();

    var worlds: std.ArrayList(WorldInfo) = .empty;
    for (names) |name| {
        validateName(name) catch continue;
        if (worlds.items.len >= max_worlds) return error.TooManyWorlds;
        const item = worldInfo(result_allocator, io, parent_dir, name) catch |err| switch (err) {
            error.FileNotFound => continue,
            error.NotDir => continue,
            else => |other| return other,
        };
        try worlds.append(result_allocator, item);
    }
    return .{ .arena = arena, .worlds = try worlds.toOwnedSlice(result_allocator) };
}

pub fn get(backing: std.mem.Allocator, io: std.Io, saves_dir: std.Io.Dir, name: []const u8) !List {
    try validateName(name);
    const arena = try backing.create(std.heap.ArenaAllocator);
    errdefer backing.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(backing);
    errdefer arena.deinit();
    const result_allocator = arena.allocator();

    var world_dir = saves_dir.openDir(io, name, .{}) catch |err| switch (err) {
        error.FileNotFound => return error.WorldNotFound,
        else => |other| return other,
    };
    defer world_dir.close(io);
    const info = try worldInfo(result_allocator, io, saves_dir, name);
    const worlds = try result_allocator.alloc(WorldInfo, 1);
    worlds[0] = info;
    return .{ .arena = arena, .worlds = worlds };
}

pub fn readIcon(allocator: std.mem.Allocator, io: std.Io, saves_dir: std.Io.Dir, name: []const u8) !?[]u8 {
    try validateName(name);
    var world_dir = saves_dir.openDir(io, name, .{}) catch |err| switch (err) {
        error.FileNotFound => return error.WorldNotFound,
        else => |other| return other,
    };
    defer world_dir.close(io);
    const bytes = world_dir.readFileAlloc(io, icon_name, allocator, .limited(max_icon_bytes + 1)) catch |err| switch (err) {
        error.FileNotFound => return null,
        else => |other| return other,
    };
    if (bytes.len > max_icon_bytes) {
        allocator.free(bytes);
        return error.IconTooLarge;
    }
    return bytes;
}

fn ensureMutable(io: std.Io, saves_dir: std.Io.Dir, name: []const u8) !void {
    try validateName(name);
    var world_dir = saves_dir.openDir(io, name, .{}) catch |err| switch (err) {
        error.FileNotFound => return error.WorldNotFound,
        else => |other| return other,
    };
    defer world_dir.close(io);
    if (statOptional(io, world_dir, session_lock_name)) return error.WorldLocked;
}

fn ensureFree(io: std.Io, saves_dir: std.Io.Dir, name: []const u8) !void {
    try validateName(name);
    if (statOptional(io, saves_dir, name)) return error.WorldExists;
}

fn gzipWriteAtomic(io: std.Io, world_dir: std.Io.Dir, path: []const u8, payload: []const u8) !void {
    var atomic_file = try world_dir.createFileAtomic(io, path, .{ .replace = true });
    defer atomic_file.deinit(io);
    var file_buffer: [4096]u8 = undefined;
    var file_writer: std.Io.File.Writer = .init(atomic_file.file, io, &file_buffer);
    var window: [std.compress.flate.max_window_len]u8 = undefined;
    var compress = try std.compress.flate.Compress.init(&file_writer.interface, &window, .gzip, .default);
    try compress.writer.writeAll(payload);
    try compress.finish();
    try file_writer.interface.flush();
    try atomic_file.file.sync(io);
    try atomic_file.replace(io);
}

fn setLevelName(backing: std.mem.Allocator, io: std.Io, world_dir: std.Io.Dir, new_name: []const u8) !void {
    var arena = std.heap.ArenaAllocator.init(backing);
    defer arena.deinit();
    const allocator = arena.allocator();

    const compressed = world_dir.readFileAlloc(io, level_dat_name, allocator, .limited(max_level_dat_bytes + 1)) catch |err| switch (err) {
        error.FileNotFound => return error.LevelDataMissing,
        else => |other| return other,
    };
    if (compressed.len > max_level_dat_bytes) return error.InvalidLevelData;
    const payload = gunzipAlloc(allocator, compressed) catch return error.InvalidLevelData;
    const document = nbt.parse(allocator, payload) catch return error.InvalidLevelData;
    const data = nbt.findCompound(document.root, "Data") orelse return error.InvalidLevelData;
    const name_copy = try allocator.dupe(u8, new_name);
    if (!nbt.setString(data, "LevelName", name_copy)) return error.InvalidLevelData;

    const encoded = try nbt.serialize(allocator, document);
    const reparsed = nbt.parse(allocator, encoded) catch return error.InvalidLevelData;
    const expected: nbt.Tag = .{ .compound = document.root };
    const actual: nbt.Tag = .{ .compound = reparsed.root };
    if (!expected.eql(actual)) return error.InvalidLevelData;

    try gzipWriteAtomic(io, world_dir, level_dat_name, encoded);
}

pub fn rename(backing: std.mem.Allocator, io: std.Io, saves_dir: std.Io.Dir, old_name: []const u8, new_name: []const u8) !void {
    if (std.mem.eql(u8, old_name, new_name)) return error.InvalidName;
    try ensureMutable(io, saves_dir, old_name);
    try ensureFree(io, saves_dir, new_name);

    {
        var world_dir = try saves_dir.openDir(io, old_name, .{});
        defer world_dir.close(io);
        setLevelName(backing, io, world_dir, new_name) catch |err| switch (err) {
            error.LevelDataMissing, error.InvalidLevelData => {},
            else => |other| return other,
        };
    }
    try saves_dir.rename(old_name, saves_dir, new_name, io);
}

fn copyRecursive(io: std.Io, source_dir: std.Io.Dir, dest_dir: std.Io.Dir) !void {
    var iterator = source_dir.iterate();
    while (try iterator.next(io)) |entry| {
        switch (entry.kind) {
            .file => {
                if (std.mem.eql(u8, entry.name, session_lock_name)) continue;
                try source_dir.copyFile(entry.name, dest_dir, entry.name, io, .{});
            },
            .directory => {
                if (isTempName(entry.name)) continue;
                try dest_dir.createDir(io, entry.name, .default_dir);
                var child_source = try source_dir.openDir(io, entry.name, .{ .iterate = true });
                defer child_source.close(io);
                var child_dest = try dest_dir.openDir(io, entry.name, .{});
                defer child_dest.close(io);
                try copyRecursive(io, child_source, child_dest);
            },
            else => return error.UnsupportedEntry,
        }
    }
}

pub fn duplicate(io: std.Io, saves_dir: std.Io.Dir, source_name: []const u8, dest_name: []const u8) !void {
    if (std.mem.eql(u8, source_name, dest_name)) return error.InvalidName;
    try ensureMutable(io, saves_dir, source_name);
    try ensureFree(io, saves_dir, dest_name);

    var random_bytes: [8]u8 = undefined;
    try io.randomSecure(&random_bytes);
    var name_buffer: [max_world_name_bytes + 32]u8 = undefined;
    const staging_name = try tempName(&name_buffer, copy_marker, dest_name, std.mem.readInt(u64, &random_bytes, .little));
    saves_dir.deleteTree(io, staging_name) catch {};
    errdefer saves_dir.deleteTree(io, staging_name) catch {};

    var source_dir = try saves_dir.openDir(io, source_name, .{ .iterate = true });
    defer source_dir.close(io);
    var staging_dir = try saves_dir.createDirPathOpen(io, staging_name, .{});
    defer staging_dir.close(io);
    try copyRecursive(io, source_dir, staging_dir);
    try saves_dir.rename(staging_name, saves_dir, dest_name, io);
}

pub fn remove(io: std.Io, saves_dir: std.Io.Dir, name: []const u8) !void {
    try ensureMutable(io, saves_dir, name);

    var random_bytes: [8]u8 = undefined;
    try io.randomSecure(&random_bytes);
    var name_buffer: [max_world_name_bytes + 32]u8 = undefined;
    const trash_name = try tempName(&name_buffer, trash_marker, name, std.mem.readInt(u64, &random_bytes, .little));
    saves_dir.deleteTree(io, trash_name) catch {};

    try saves_dir.rename(name, saves_dir, trash_name, io);
    saves_dir.deleteTree(io, trash_name) catch |err| {
        saves_dir.rename(trash_name, saves_dir, name, io) catch {};
        return err;
    };
}

pub const VersionRelation = enum {
    same,
    different,
    unknown,
};

pub fn versionRelation(world_version: ?[]const u8, minecraft_version: ?[]const u8) VersionRelation {
    const world = world_version orelse return .unknown;
    const instance = minecraft_version orelse return .unknown;
    if (world.len == 0 or instance.len == 0) return .unknown;
    return if (std.mem.eql(u8, world, instance)) .same else .different;
}

test "validateName rejects traversal and separators" {
    try validateName("My World");
    try validateName("世界 1.20");
    try std.testing.expectError(error.InvalidName, validateName(""));
    try std.testing.expectError(error.InvalidName, validateName("."));
    try std.testing.expectError(error.InvalidName, validateName(".."));
    try std.testing.expectError(error.InvalidName, validateName(".hidden"));
    try std.testing.expectError(error.InvalidName, validateName("a/b"));
    try std.testing.expectError(error.InvalidName, validateName("a\\b"));
    try std.testing.expectError(error.InvalidName, validateName("trail "));
    try std.testing.expectError(error.InvalidName, validateName("trail."));
    try std.testing.expectError(error.InvalidName, validateName("bad\x01name"));
}

test "versionRelation compares world and instance versions" {
    try std.testing.expectEqual(VersionRelation.same, versionRelation("1.20.1", "1.20.1"));
    try std.testing.expectEqual(VersionRelation.different, versionRelation("1.20.1", "1.16.5"));
    try std.testing.expectEqual(VersionRelation.unknown, versionRelation(null, "1.20.1"));
    try std.testing.expectEqual(VersionRelation.unknown, versionRelation("1.20.1", null));
    try std.testing.expectEqual(VersionRelation.unknown, versionRelation("", "1.20.1"));
}

test "duplicate and remove round-trip a world directory" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const io = std.testing.io;
    try tmp.dir.createDirPath(io, "saves");
    var saves_dir = try tmp.dir.openDir(io, "saves", .{ .iterate = true });
    defer saves_dir.close(io);

    var world_dir = try saves_dir.createDirPathOpen(io, "Alpha/region", .{});
    try world_dir.writeFile(io, .{ .sub_path = "r.0.0.mca", .data = "chunk-data" });
    world_dir.close(io);
    var root_dir = try saves_dir.openDir(io, "Alpha", .{});
    try root_dir.writeFile(io, .{ .sub_path = session_lock_name, .data = "lock" });
    root_dir.close(io);

    try std.testing.expectError(error.WorldLocked, duplicate(io, saves_dir, "Alpha", "Beta"));
    try saves_dir.deleteFile(io, "Alpha/" ++ session_lock_name);

    try duplicate(io, saves_dir, "Alpha", "Beta");
    const copied = try saves_dir.readFileAlloc(io, "Beta/region/r.0.0.mca", std.testing.allocator, .limited(1024));
    defer std.testing.allocator.free(copied);
    try std.testing.expectEqualStrings("chunk-data", copied);
    try std.testing.expectError(error.WorldExists, duplicate(io, saves_dir, "Alpha", "Beta"));

    try remove(io, saves_dir, "Beta");
    try std.testing.expectError(error.WorldNotFound, remove(io, saves_dir, "Beta"));
    try std.testing.expectError(error.FileNotFound, saves_dir.statFile(io, "Beta", .{}));
}

test "rename rewrites LevelName and keeps other data" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const io = std.testing.io;
    try tmp.dir.createDirPath(io, "saves");
    var saves_dir = try tmp.dir.openDir(io, "saves", .{ .iterate = true });
    defer saves_dir.close(io);
    var world_dir = try saves_dir.createDirPathOpen(io, "Old Name", .{});
    defer world_dir.close(io);

    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const allocator = arena.allocator();

    var version_entries = [_]nbt.Entry{
        .{ .name = "Name", .tag = .{ .string = "1.20.1" } },
        .{ .name = "Id", .tag = .{ .int = 3465 } },
    };
    var data_entries = [_]nbt.Entry{
        .{ .name = "LevelName", .tag = .{ .string = "Old Name" } },
        .{ .name = "GameType", .tag = .{ .int = 0 } },
        .{ .name = "Version", .tag = .{ .compound = &version_entries } },
    };
    var root_entries = [_]nbt.Entry{
        .{ .name = "Data", .tag = .{ .compound = &data_entries } },
    };
    const encoded = try nbt.serialize(allocator, .{ .name = "", .root = &root_entries });
    try gzipWriteAtomic(io, world_dir, level_dat_name, encoded);

    try rename(std.testing.allocator, io, saves_dir, "Old Name", "New Name");

    var renamed_dir = try saves_dir.openDir(io, "New Name", .{});
    defer renamed_dir.close(io);
    const meta = parseLevelMeta(allocator, io, renamed_dir).?;
    try std.testing.expectEqualStrings("New Name", meta.level_name.?);
    try std.testing.expectEqualStrings("1.20.1", meta.version_name.?);
    try std.testing.expectEqual(@as(?i32, 3465), meta.version_id);
    try std.testing.expectEqual(GameMode.survival, meta.game_mode.?);
    try std.testing.expectError(error.WorldNotFound, rename(std.testing.allocator, io, saves_dir, "Old Name", "Again"));
}

test "list reports metadata and skips temp directories" {
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    const io = std.testing.io;
    try tmp.dir.createDirPath(io, "saves");
    var saves_dir = try tmp.dir.openDir(io, "saves", .{ .iterate = true });
    defer saves_dir.close(io);

    var world_dir = try saves_dir.createDirPathOpen(io, "Gamma", .{});
    try world_dir.writeFile(io, .{ .sub_path = "icon.png", .data = "png-bytes" });
    world_dir.close(io);
    var temp_dir = try saves_dir.createDirPathOpen(io, ".jmcl-copy-stale", .{});
    temp_dir.close(io);
    var locked_dir = try saves_dir.createDirPathOpen(io, "Locked", .{});
    try locked_dir.writeFile(io, .{ .sub_path = session_lock_name, .data = "x" });
    locked_dir.close(io);

    var worlds = try list(std.testing.allocator, io, saves_dir);
    defer worlds.deinit();
    try std.testing.expectEqual(@as(usize, 2), worlds.worlds.len);
    try std.testing.expectEqualStrings("Gamma", worlds.worlds[0].name);
    try std.testing.expect(worlds.worlds[0].has_icon);
    try std.testing.expect(!worlds.worlds[0].locked);
    try std.testing.expect(worlds.worlds[0].meta == null);
    try std.testing.expectEqualStrings("Locked", worlds.worlds[1].name);
    try std.testing.expect(worlds.worlds[1].locked);
}
