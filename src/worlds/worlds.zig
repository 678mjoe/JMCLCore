const std = @import("std");
const nbt = @import("nbt.zig");
const zip_writer = @import("zip_writer.zig");
const zip_reader = @import("../install/zip_reader.zig");

pub const max_world_name_bytes: usize = 255;
pub const max_worlds: usize = 1024;
pub const max_level_dat_bytes: usize = 32 * 1024 * 1024;
pub const max_icon_bytes: usize = 64 * 1024;
pub const saves_directory_name = "saves";
pub const backups_directory_name = "backups";
pub const max_import_bytes: usize = 1024 * 1024 * 1024;
pub const max_backup_label_bytes: usize = 40;
pub const level_dat_name = "level.dat";
pub const icon_name = "icon.png";
pub const session_lock_name = "session.lock";
pub const copy_marker = ".jmcl-copy-";
pub const trash_marker = ".jmcl-trash-";
pub const metadata_directory_name = ".jmcl";

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
        .locked = isLocked(io, world_dir) catch true,
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

pub fn isLocked(io: std.Io, world_dir: std.Io.Dir) !bool {
    const lock_file = world_dir.openFile(io, session_lock_name, .{ .mode = .read_write }) catch |err| switch (err) {
        error.FileNotFound => return false,
        else => |other| return other,
    };
    defer lock_file.close(io);
    const acquired = try lock_file.tryLock(io, .exclusive);
    if (acquired) lock_file.unlock(io);
    return !acquired;
}

pub fn openWorld(io: std.Io, saves_dir: std.Io.Dir, name: []const u8) !std.Io.Dir {
    try validateName(name);
    return saves_dir.openDir(io, name, .{}) catch |err| switch (err) {
        error.FileNotFound => return error.WorldNotFound,
        else => |other| return other,
    };
}

pub fn openMutable(io: std.Io, saves_dir: std.Io.Dir, name: []const u8) !std.Io.Dir {
    var world_dir = try openWorld(io, saves_dir, name);
    const locked = isLocked(io, world_dir) catch |err| {
        world_dir.close(io);
        return err;
    };
    if (locked) {
        world_dir.close(io);
        return error.WorldLocked;
    }
    return world_dir;
}

fn ensureMutable(io: std.Io, saves_dir: std.Io.Dir, name: []const u8) !void {
    var world_dir = try openMutable(io, saves_dir, name);
    world_dir.close(io);
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

pub const ExportResult = struct {
    size_bytes: u64,
    files: u32,
};

fn addWorldToZip(zip: *zip_writer.Writer, io: std.Io, parent_dir: std.Io.Dir, dir_name: []const u8, prefix: []const u8, files: *u32) !void {
    var child_dir = try parent_dir.openDir(io, dir_name, .{ .iterate = true });
    defer child_dir.close(io);
    try addDirToZip(zip, io, child_dir, prefix, files);
}

fn addDirToZip(zip: *zip_writer.Writer, io: std.Io, dir: std.Io.Dir, prefix: []const u8, files: *u32) anyerror!void {
    var iterator = dir.iterate();
    while (try iterator.next(io)) |entry| {
        if (entry.kind == .file and std.mem.eql(u8, entry.name, session_lock_name)) continue;
        if (std.mem.eql(u8, entry.name, metadata_directory_name)) continue;
        if (isTempName(entry.name)) continue;
        var path_buffer: [4096]u8 = undefined;
        const rel = std.fmt.bufPrint(&path_buffer, "{s}/{s}", .{ prefix, entry.name }) catch
            return error.EntryNameTooLong;
        switch (entry.kind) {
            .file => {
                try zip.addFile(rel, dir, entry.name);
                files.* += 1;
            },
            .directory => {
                const stat = dir.statFile(io, entry.name, .{}) catch null;
                const seconds = if (stat) |value| value.mtime.toSeconds() else 0;
                try zip.addDirectory(rel, seconds);
                var child_dir = try dir.openDir(io, entry.name, .{ .iterate = true });
                defer child_dir.close(io);
                try addDirToZip(zip, io, child_dir, rel, files);
            },
            else => return error.UnsupportedEntry,
        }
    }
}

pub fn exportWorldSet(backing: std.mem.Allocator, io: std.Io, parent_dir: std.Io.Dir, names: []const []const u8, dest_dir: std.Io.Dir, dest_path: []const u8) !ExportResult {
    for (names) |name| try ensureMutable(io, parent_dir, name);

    var zip = try zip_writer.Writer.create(backing, io, dest_dir, dest_path);
    var files: u32 = 0;
    for (names) |name| {
        addWorldToZip(&zip, io, parent_dir, name, name, &files) catch |err| {
            zip.abort();
            return err;
        };
    }
    const size = zip.finish() catch |err| {
        zip.abort();
        return err;
    };
    return .{ .size_bytes = size, .files = files };
}

pub fn exportWorld(backing: std.mem.Allocator, io: std.Io, saves_dir: std.Io.Dir, name: []const u8, dest_dir: std.Io.Dir, dest_path: []const u8) !ExportResult {
    return exportWorldSet(backing, io, saves_dir, &.{name}, dest_dir, dest_path);
}

fn validateEntryPath(name: []const u8) !void {
    if (name.len == 0 or name.len > zip_reader.max_entry_name_bytes) return error.ArchiveInvalid;
    if (name[0] == '/') return error.ArchiveInvalid;
    for (name) |byte| {
        if (byte == 0 or byte == '\\' or byte == ':') return error.ArchiveInvalid;
    }
    var parts = std.mem.splitScalar(u8, name, '/');
    while (parts.next()) |part| {
        if (std.mem.eql(u8, part, "..")) return error.ArchiveInvalid;
    }
}

fn topComponent(name: []const u8) []const u8 {
    const slash = std.mem.indexOfScalar(u8, name, '/') orelse return name;
    return name[0..slash];
}

fn writeEntryFile(io: std.Io, dest_dir: std.Io.Dir, rel_path: []const u8, bytes: []const u8) !void {
    if (std.mem.lastIndexOfScalar(u8, rel_path, '/')) |slash| {
        try dest_dir.createDirPath(io, rel_path[0..slash]);
    }
    try dest_dir.writeFile(io, .{ .sub_path = rel_path, .data = bytes });
}

const Extracted = struct {
    arena: std.heap.ArenaAllocator,
    jar: []u8,
    entries: std.ArrayList(zip_reader.Entry),

    fn deinit(extracted: *Extracted) void {
        extracted.arena.deinit();
    }
};

fn loadArchive(backing: std.mem.Allocator, io: std.Io, source_dir: std.Io.Dir, source_path: []const u8) !Extracted {
    var extracted: Extracted = .{
        .arena = std.heap.ArenaAllocator.init(backing),
        .jar = undefined,
        .entries = .empty,
    };
    errdefer extracted.deinit();
    const allocator = extracted.arena.allocator();
    extracted.jar = source_dir.readFileAlloc(io, source_path, allocator, .limited(max_import_bytes + 1)) catch |err| switch (err) {
        error.FileNotFound => return error.BackupNotFound,
        else => |other| return other,
    };
    if (extracted.jar.len > max_import_bytes) return error.ArchiveTooLarge;
    var iterator = zip_reader.entries(extracted.jar) catch return error.ArchiveInvalid;
    while (iterator.next() catch return error.ArchiveInvalid) |entry| {
        validateEntryPath(entry.name) catch return error.ArchiveInvalid;
        try extracted.entries.append(allocator, entry);
    }
    if (extracted.entries.items.len == 0) return error.ArchiveInvalid;
    return extracted;
}

fn commonPrefix(entries: []const zip_reader.Entry) ?[]const u8 {
    var prefix: ?[]const u8 = null;
    for (entries) |entry| {
        const slash = std.mem.indexOfScalar(u8, entry.name, '/') orelse return null;
        const top = entry.name[0..slash];
        if (prefix) |existing| {
            if (!std.mem.eql(u8, existing, top)) return null;
        } else {
            prefix = top;
        }
    }
    return prefix;
}

fn extractEntries(backing: std.mem.Allocator, io: std.Io, jar: []const u8, entries: []const zip_reader.Entry, staging_dir: std.Io.Dir, strip: ?[]const u8) !void {
    var scratch = std.heap.ArenaAllocator.init(backing);
    defer scratch.deinit();
    for (entries) |entry| {
        var rel = entry.name;
        if (strip) |prefix| {
            if (std.mem.eql(u8, rel, prefix)) continue;
            if (rel.len > prefix.len and std.mem.startsWith(u8, rel, prefix) and rel[prefix.len] == '/') {
                rel = rel[prefix.len + 1 ..];
            }
        }
        if (rel.len == 0) continue;
        if (rel[rel.len - 1] == '/') {
            try staging_dir.createDirPath(io, rel[0 .. rel.len - 1]);
            continue;
        }
        if (std.mem.eql(u8, rel, session_lock_name)) continue;
        const bytes = zip_reader.readAlloc(scratch.allocator(), jar, entry, max_import_bytes) catch return error.ArchiveInvalid;
        try writeEntryFile(io, staging_dir, rel, bytes);
        _ = scratch.reset(.retain_capacity);
    }
}

fn fileStem(path: []const u8) []const u8 {
    const base = std.fs.path.basename(path);
    const extension = std.fs.path.extension(base);
    return base[0 .. base.len - extension.len];
}

pub fn importWorld(backing: std.mem.Allocator, io: std.Io, saves_dir: std.Io.Dir, source_dir: std.Io.Dir, source_path: []const u8, name: ?[]const u8, replace: bool) ![]u8 {
    var extracted = try loadArchive(backing, io, source_dir, source_path);
    defer extracted.deinit();

    const prefix = commonPrefix(extracted.entries.items);
    const target = name orelse prefix orelse fileStem(source_path);
    try validateName(target);
    const target_copy = try backing.dupe(u8, target);
    errdefer backing.free(target_copy);

    var random_bytes: [8]u8 = undefined;
    try io.randomSecure(&random_bytes);
    var name_buffer: [max_world_name_bytes + 32]u8 = undefined;
    const staging_name = try tempName(&name_buffer, copy_marker, target, std.mem.readInt(u64, &random_bytes, .little));
    saves_dir.deleteTree(io, staging_name) catch {};
    defer saves_dir.deleteTree(io, staging_name) catch {};

    {
        var staging_dir = try saves_dir.createDirPathOpen(io, staging_name, .{});
        defer staging_dir.close(io);
        try extractEntries(backing, io, extracted.jar, extracted.entries.items, staging_dir, prefix);
    }

    var trash_name: ?[]const u8 = null;
    defer if (trash_name) |trash| saves_dir.deleteTree(io, trash) catch {};
    if (statOptional(io, saves_dir, target)) {
        if (!replace) return error.WorldExists;
        // Import and backup restore share this path. Probe immediately before
        // replacing the target, using the same live-session guard as the other
        // world mutations. This is not exclusion against an external game
        // starting after the probe; callers must still serialize mutations.
        try ensureMutable(io, saves_dir, target);
        var trash_buffer: [max_world_name_bytes + 32]u8 = undefined;
        const trash = try tempName(&trash_buffer, trash_marker, target, std.mem.readInt(u64, &random_bytes, .little));
        try saves_dir.rename(target, saves_dir, trash, io);
        trash_name = trash;
    }
    saves_dir.rename(staging_name, saves_dir, target, io) catch |err| {
        if (trash_name) |trash| saves_dir.rename(trash, saves_dir, target, io) catch {};
        return err;
    };
    return target_copy;
}

pub fn importWorldSet(backing: std.mem.Allocator, io: std.Io, parent_dir: std.Io.Dir, source_dir: std.Io.Dir, source_path: []const u8, replace: bool) !void {
    var extracted = try loadArchive(backing, io, source_dir, source_path);
    defer extracted.deinit();
    const allocator = extracted.arena.allocator();

    var components: std.ArrayList([]const u8) = .empty;
    for (extracted.entries.items) |entry| {
        if (std.mem.indexOfScalar(u8, entry.name, '/') == null) return error.ArchiveInvalid;
        const top = topComponent(entry.name);
        validateName(top) catch return error.ArchiveInvalid;
        var found = false;
        for (components.items) |existing| {
            if (std.mem.eql(u8, existing, top)) found = true;
        }
        if (!found) try components.append(allocator, top);
    }

    var random_bytes: [8]u8 = undefined;
    try io.randomSecure(&random_bytes);
    var name_buffer: [max_world_name_bytes + 32]u8 = undefined;
    const staging_name = try tempName(&name_buffer, copy_marker, "import", std.mem.readInt(u64, &random_bytes, .little));
    parent_dir.deleteTree(io, staging_name) catch {};
    defer parent_dir.deleteTree(io, staging_name) catch {};

    {
        var staging_dir = try parent_dir.createDirPathOpen(io, staging_name, .{});
        defer staging_dir.close(io);
        try extractEntries(backing, io, extracted.jar, extracted.entries.items, staging_dir, null);
    }

    for (components.items) |component| {
        var trash_buffer: [max_world_name_bytes + 32]u8 = undefined;
        var trash_name: ?[]const u8 = null;
        if (statOptional(io, parent_dir, component)) {
            if (!replace) return error.WorldExists;
            const trash = try tempName(&trash_buffer, trash_marker, component, std.mem.readInt(u64, &random_bytes, .little));
            try parent_dir.rename(component, parent_dir, trash, io);
            trash_name = trash;
        }
        var rel_buffer: [max_world_name_bytes + 40]u8 = undefined;
        const staging_rel = std.fmt.bufPrint(&rel_buffer, "{s}/{s}", .{ staging_name, component }) catch
            return error.InvalidName;
        parent_dir.rename(staging_rel, parent_dir, component, io) catch |err| {
            if (trash_name) |trash| parent_dir.rename(trash, parent_dir, component, io) catch {};
            return err;
        };
        if (trash_name) |trash| parent_dir.deleteTree(io, trash) catch {};
    }
}

pub fn sanitizeLabel(allocator: std.mem.Allocator, label: []const u8) !?[]u8 {
    if (label.len == 0) return null;
    var out: std.ArrayList(u8) = .empty;
    for (label) |byte| {
        if (out.items.len >= max_backup_label_bytes) break;
        if (std.ascii.isAlphanumeric(byte) or byte == '-' or byte == '_' or byte == '.') {
            try out.append(allocator, byte);
        } else if (out.items.len > 0 and out.items[out.items.len - 1] != '-') {
            try out.append(allocator, '-');
        }
    }
    while (out.items.len > 0 and (out.items[out.items.len - 1] == '-' or out.items[out.items.len - 1] == '.')) {
        _ = out.pop();
    }
    if (out.items.len == 0) return null;
    return try out.toOwnedSlice(allocator);
}

pub fn backupFileName(buffer: []u8, world: []const u8, now_ms: i64, label: ?[]const u8) ![]const u8 {
    const epoch_seconds_value: std.time.epoch.EpochSeconds = .{ .secs = @intCast(@divFloor(now_ms, 1000)) };
    const epoch_day = epoch_seconds_value.getEpochDay();
    const year_day = epoch_day.calculateYearDay();
    const month_day = year_day.calculateMonthDay();
    const day_seconds = epoch_seconds_value.getDaySeconds();
    return if (label) |text|
        std.fmt.bufPrint(buffer, "{s}-{d:0>4}{d:0>2}{d:0>2}-{d:0>2}{d:0>2}{d:0>2}-{s}.zip", .{
            world,                         year_day.year,                    month_day.month.numeric(),          month_day.day_index + 1,
            day_seconds.getHoursIntoDay(), day_seconds.getMinutesIntoHour(), day_seconds.getSecondsIntoMinute(), text,
        }) catch return error.InvalidName
    else
        std.fmt.bufPrint(buffer, "{s}-{d:0>4}{d:0>2}{d:0>2}-{d:0>2}{d:0>2}{d:0>2}.zip", .{
            world,                         year_day.year,                    month_day.month.numeric(),          month_day.day_index + 1,
            day_seconds.getHoursIntoDay(), day_seconds.getMinutesIntoHour(), day_seconds.getSecondsIntoMinute(),
        }) catch return error.InvalidName;
}

pub fn backupWorld(backing: std.mem.Allocator, io: std.Io, parent_dir: std.Io.Dir, backups_dir: std.Io.Dir, name: []const u8, label: ?[]const u8, now_ms: i64) ![]u8 {
    var arena = std.heap.ArenaAllocator.init(backing);
    defer arena.deinit();
    const clean_label = if (label) |text| try sanitizeLabel(arena.allocator(), text) else null;

    var name_buffer: [max_world_name_bytes + max_backup_label_bytes + 32]u8 = undefined;
    const file_name = try backupFileName(&name_buffer, name, now_ms, clean_label);
    if (statOptional(io, backups_dir, file_name)) return error.WorldExists;
    _ = try exportWorld(backing, io, parent_dir, name, backups_dir, file_name);
    return try backing.dupe(u8, file_name);
}

pub const BackupInfo = struct {
    file: []const u8,
    world: ?[]const u8,
    created_ms: i64,
    size_bytes: u64,
};

pub const BackupList = struct {
    arena: *std.heap.ArenaAllocator,
    backups: []BackupInfo,

    pub fn deinit(list_self: *BackupList) void {
        const allocator = list_self.arena.child_allocator;
        list_self.arena.deinit();
        allocator.destroy(list_self.arena);
        list_self.* = undefined;
    }
};

fn parseBackupName(file: []const u8) ?[]const u8 {
    if (!std.mem.endsWith(u8, file, ".zip")) return null;
    const stem = file[0 .. file.len - 4];
    var index: usize = 0;
    while (index + 16 <= stem.len) : (index += 1) {
        if (stem[index] != '-') continue;
        const marker = stem[index + 1 .. index + 16];
        var digits = true;
        for (marker[0..8]) |byte| {
            if (!std.ascii.isDigit(byte)) digits = false;
        }
        if (marker[8] != '-') digits = false;
        for (marker[9..15]) |byte| {
            if (!std.ascii.isDigit(byte)) digits = false;
        }
        if (!digits) continue;
        const after = index + 16;
        if (after != stem.len and stem[after] != '-') continue;
        if (index == 0) return null;
        return stem[0..index];
    }
    return null;
}

pub fn listBackups(backing: std.mem.Allocator, io: std.Io, backups_dir: std.Io.Dir) !BackupList {
    const arena = try backing.create(std.heap.ArenaAllocator);
    errdefer backing.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(backing);
    errdefer arena.deinit();
    const allocator = arena.allocator();

    var backups: std.ArrayList(BackupInfo) = .empty;
    var iterator = backups_dir.iterate();
    while (try iterator.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.name, ".zip")) continue;
        if (backups.items.len >= max_worlds) return error.TooManyWorlds;
        const stat = backups_dir.statFile(io, entry.name, .{}) catch continue;
        try backups.append(allocator, .{
            .file = try allocator.dupe(u8, entry.name),
            .world = if (parseBackupName(entry.name)) |world| try allocator.dupe(u8, world) else null,
            .created_ms = stat.mtime.toMilliseconds(),
            .size_bytes = stat.size,
        });
    }
    std.mem.sort(BackupInfo, backups.items, {}, struct {
        fn lessThan(_: void, a: BackupInfo, b: BackupInfo) bool {
            return a.created_ms > b.created_ms;
        }
    }.lessThan);
    return .{ .arena = arena, .backups = try backups.toOwnedSlice(allocator) };
}

pub fn emptyBackupList(backing: std.mem.Allocator) !BackupList {
    const arena = try backing.create(std.heap.ArenaAllocator);
    errdefer backing.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(backing);
    return .{ .arena = arena, .backups = &.{} };
}

pub fn validateBackupFileName(file: []const u8) !void {
    if (file.len == 0 or file.len > max_world_name_bytes + max_backup_label_bytes + 32) return error.InvalidName;
    if (!std.mem.endsWith(u8, file, ".zip")) return error.InvalidName;
    if (std.mem.indexOfAny(u8, file, "/\\") != null) return error.InvalidName;
}

pub fn deleteBackup(io: std.Io, backups_dir: std.Io.Dir, file: []const u8) !void {
    try validateBackupFileName(file);
    backups_dir.deleteFile(io, file) catch |err| switch (err) {
        error.FileNotFound => return error.BackupNotFound,
        else => |other| return other,
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
    const lock_file = try root_dir.createFile(io, session_lock_name, .{});
    try lock_file.lock(io, .exclusive);
    root_dir.close(io);

    try std.testing.expectError(error.WorldLocked, duplicate(io, saves_dir, "Alpha", "Beta"));
    lock_file.close(io);

    try duplicate(io, saves_dir, "Alpha", "Beta");
    try std.testing.expectError(error.FileNotFound, saves_dir.statFile(io, "Beta/" ++ session_lock_name, .{}));
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
    const lock_file = try locked_dir.createFile(io, session_lock_name, .{});
    try lock_file.lock(io, .exclusive);
    defer lock_file.close(io);
    var stale_dir = try saves_dir.createDirPathOpen(io, "Stale", .{});
    try stale_dir.writeFile(io, .{ .sub_path = session_lock_name, .data = "x" });
    stale_dir.close(io);

    var worlds = try list(std.testing.allocator, io, saves_dir);
    defer worlds.deinit();
    try std.testing.expectEqual(@as(usize, 3), worlds.worlds.len);
    try std.testing.expectEqualStrings("Gamma", worlds.worlds[0].name);
    try std.testing.expect(worlds.worlds[0].has_icon);
    try std.testing.expect(!worlds.worlds[0].locked);
    try std.testing.expect(worlds.worlds[0].meta == null);
    try std.testing.expectEqualStrings("Locked", worlds.worlds[1].name);
    try std.testing.expect(worlds.worlds[1].locked);
    try std.testing.expectEqualStrings("Stale", worlds.worlds[2].name);
    try std.testing.expect(!worlds.worlds[2].locked);
}

test "export and import round-trip a world" {
    const io = std.testing.io;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.createDirPath(io, "saves");
    var saves_dir = try tmp.dir.openDir(io, "saves", .{ .iterate = true });
    defer saves_dir.close(io);
    try saves_dir.createDirPath(io, "Alpha/region");
    var region = try saves_dir.openDir(io, "Alpha/region", .{});
    try region.writeFile(io, .{ .sub_path = "r.0.0.mca", .data = "chunk-data" });
    region.close(io);
    var alpha = try saves_dir.openDir(io, "Alpha", .{});
    try alpha.writeFile(io, .{ .sub_path = "level.dat", .data = "nbt" });
    try alpha.writeFile(io, .{ .sub_path = session_lock_name, .data = "lock" });
    alpha.close(io);

    const result = try exportWorld(std.testing.allocator, io, saves_dir, "Alpha", tmp.dir, "alpha.zip");
    try std.testing.expect(result.files == 2);
    try std.testing.expect(result.size_bytes > 0);

    const imported = try importWorld(std.testing.allocator, io, saves_dir, tmp.dir, "alpha.zip", "Beta", false);
    defer std.testing.allocator.free(imported);
    try std.testing.expectEqualStrings("Beta", imported);
    const level = try saves_dir.readFileAlloc(io, "Beta/level.dat", std.testing.allocator, .limited(64));
    defer std.testing.allocator.free(level);
    try std.testing.expectEqualStrings("nbt", level);
    const chunk = try saves_dir.readFileAlloc(io, "Beta/region/r.0.0.mca", std.testing.allocator, .limited(64));
    defer std.testing.allocator.free(chunk);
    try std.testing.expectEqualStrings("chunk-data", chunk);
    try std.testing.expectError(error.FileNotFound, saves_dir.statFile(io, "Beta/" ++ session_lock_name, .{}));

    try std.testing.expectError(error.WorldExists, importWorld(std.testing.allocator, io, saves_dir, tmp.dir, "alpha.zip", "Beta", false));
}

test "export keeps the world registry out of vanilla archives" {
    const io = std.testing.io;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.createDirPath(io, "saves");
    var saves_dir = try tmp.dir.openDir(io, "saves", .{ .iterate = true });
    defer saves_dir.close(io);
    try saves_dir.createDirPath(io, "Alpha/datapacks");
    try saves_dir.createDirPath(io, "Alpha/" ++ metadata_directory_name);
    var alpha = try saves_dir.openDir(io, "Alpha", .{});
    try alpha.writeFile(io, .{ .sub_path = "level.dat", .data = "nbt" });
    try alpha.writeFile(io, .{ .sub_path = "datapacks/pack.zip", .data = "zip-bytes" });
    try alpha.writeFile(io, .{ .sub_path = metadata_directory_name ++ "/content.json", .data = "{}" });
    alpha.close(io);

    const result = try exportWorld(std.testing.allocator, io, saves_dir, "Alpha", tmp.dir, "alpha.zip");
    try std.testing.expect(result.files == 2);
    const archive = try tmp.dir.readFileAlloc(io, "alpha.zip", std.testing.allocator, .limited(max_import_bytes));
    defer std.testing.allocator.free(archive);
    var iterator = try zip_reader.entries(archive);
    var saw_datapack = false;
    while (try iterator.next()) |entry| {
        try std.testing.expect(std.mem.indexOf(u8, entry.name, metadata_directory_name) == null);
        if (std.mem.eql(u8, entry.name, "Alpha/datapacks/pack.zip")) saw_datapack = true;
    }
    try std.testing.expect(saw_datapack);
}

test "import replace rejects a locked target and preserves existing files" {
    const io = std.testing.io;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.createDirPath(io, "saves");
    var saves_dir = try tmp.dir.openDir(io, "saves", .{ .iterate = true });
    defer saves_dir.close(io);
    var source = try saves_dir.createDirPathOpen(io, "Source", .{});
    try source.writeFile(io, .{ .sub_path = "level.dat", .data = "archive" });
    source.close(io);
    _ = try exportWorld(std.testing.allocator, io, saves_dir, "Source", tmp.dir, "world.zip");
    var target = try saves_dir.createDirPathOpen(io, "Target", .{});
    try target.writeFile(io, .{ .sub_path = "level.dat", .data = "live-world" });
    try target.writeFile(io, .{ .sub_path = "keep.txt", .data = "keep" });
    const lock_file = try target.createFile(io, session_lock_name, .{});
    target.close(io);
    defer lock_file.close(io);
    try lock_file.lock(io, .exclusive);

    // Existing-target behavior stays WorldExists without overwrite permission.
    try std.testing.expectError(error.WorldExists, importWorld(std.testing.allocator, io, saves_dir, tmp.dir, "world.zip", "Target", false));
    const attempted = importWorld(std.testing.allocator, io, saves_dir, tmp.dir, "world.zip", "Target", true);
    defer if (attempted) |name| std.testing.allocator.free(name) else |_| {};
    try std.testing.expectError(error.WorldLocked, attempted);
    const existing = try saves_dir.readFileAlloc(io, "Target/level.dat", std.testing.allocator, .limited(64));
    defer std.testing.allocator.free(existing);
    try std.testing.expectEqualStrings("live-world", existing);
    const kept = try saves_dir.readFileAlloc(io, "Target/keep.txt", std.testing.allocator, .limited(64));
    defer std.testing.allocator.free(kept);
    try std.testing.expectEqualStrings("keep", kept);
    var iterator = saves_dir.iterate();
    var entries: usize = 0;
    while (try iterator.next(io)) |entry| {
        try std.testing.expect(std.mem.eql(u8, entry.name, "Source") or std.mem.eql(u8, entry.name, "Target"));
        entries += 1;
    }
    try std.testing.expectEqual(@as(usize, 2), entries);
}

test "import replace swaps the existing world" {
    const io = std.testing.io;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.createDirPath(io, "saves");
    var saves_dir = try tmp.dir.openDir(io, "saves", .{ .iterate = true });
    defer saves_dir.close(io);
    try saves_dir.createDirPath(io, "Alpha");
    var alpha = try saves_dir.openDir(io, "Alpha", .{});
    try alpha.writeFile(io, .{ .sub_path = "level.dat", .data = "old" });
    alpha.close(io);
    _ = try exportWorld(std.testing.allocator, io, saves_dir, "Alpha", tmp.dir, "alpha.zip");
    try saves_dir.deleteFile(io, "Alpha/level.dat");
    var alpha2 = try saves_dir.openDir(io, "Alpha", .{});
    try alpha2.writeFile(io, .{ .sub_path = "level.dat", .data = "corrupted" });
    try alpha2.writeFile(io, .{ .sub_path = "extra.txt", .data = "junk" });
    // A stale session.lock file is not a running game and must not block restore.
    try alpha2.writeFile(io, .{ .sub_path = session_lock_name, .data = "stale" });
    alpha2.close(io);

    const imported = try importWorld(std.testing.allocator, io, saves_dir, tmp.dir, "alpha.zip", "Alpha", true);
    defer std.testing.allocator.free(imported);
    const level = try saves_dir.readFileAlloc(io, "Alpha/level.dat", std.testing.allocator, .limited(64));
    defer std.testing.allocator.free(level);
    try std.testing.expectEqualStrings("old", level);
    try std.testing.expectError(error.FileNotFound, saves_dir.statFile(io, "Alpha/extra.txt", .{}));
}

test "backup list parses names with and without labels" {
    try std.testing.expectEqualStrings("Alpha", parseBackupName("Alpha-20240102-030405.zip").?);
    try std.testing.expectEqualStrings("My World", parseBackupName("My World-20240102-030405-pre-boss.zip").?);
    try std.testing.expect(parseBackupName("random.zip") == null);
    try std.testing.expect(parseBackupName("Alpha-20240102-030405") == null);
    try std.testing.expect(parseBackupName("-20240102-030405.zip") == null);
}

test "entry path validation rejects traversal" {
    try validateEntryPath("world/level.dat");
    try std.testing.expectError(error.ArchiveInvalid, validateEntryPath("../evil"));
    try std.testing.expectError(error.ArchiveInvalid, validateEntryPath("a/../../evil"));
    try std.testing.expectError(error.ArchiveInvalid, validateEntryPath("/abs"));
    try std.testing.expectError(error.ArchiveInvalid, validateEntryPath("C:\\evil"));
}

test "backup and delete round-trip through backups dir" {
    const io = std.testing.io;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.createDirPath(io, "saves");
    var saves_dir = try tmp.dir.openDir(io, "saves", .{ .iterate = true });
    defer saves_dir.close(io);
    try saves_dir.createDirPath(io, "Alpha");
    var alpha = try saves_dir.openDir(io, "Alpha", .{});
    try alpha.writeFile(io, .{ .sub_path = "level.dat", .data = "nbt" });
    alpha.close(io);
    try tmp.dir.createDirPath(io, "backups");
    var backups_dir = try tmp.dir.openDir(io, "backups", .{ .iterate = true });
    defer backups_dir.close(io);

    const file = try backupWorld(std.testing.allocator, io, saves_dir, backups_dir, "Alpha", "pre boss", 1704164646000);
    defer std.testing.allocator.free(file);
    try std.testing.expectEqualStrings("Alpha-20240102-030406-pre-boss.zip", file);

    var backups = try listBackups(std.testing.allocator, io, backups_dir);
    defer backups.deinit();
    try std.testing.expectEqual(@as(usize, 1), backups.backups.len);
    try std.testing.expectEqualStrings("Alpha", backups.backups[0].world.?);
    try std.testing.expect(backups.backups[0].size_bytes > 0);

    try deleteBackup(io, backups_dir, file);
    try std.testing.expectError(error.BackupNotFound, deleteBackup(io, backups_dir, file));
    try std.testing.expectError(error.InvalidName, deleteBackup(io, backups_dir, "../escape.zip"));
}
