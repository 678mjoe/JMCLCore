const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");
const server_protocol = @import("server.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;

const WorldsListParams = struct {
    directory: []const u8,
    minecraft_version: ?[]const u8 = null,
};

const WorldsTargetParams = struct {
    directory: []const u8,
    world: []const u8,
    minecraft_version: ?[]const u8 = null,
};

const WorldsMutationParams = struct {
    directory: []const u8,
    world: []const u8,
    new_name: []const u8,
    minecraft_version: ?[]const u8 = null,
};

const ServerWorldsListParams = struct {
    directory: []const u8 = core.server_store.default_root,
    id: []const u8,
    minecraft_version: ?[]const u8 = null,
};

const ServerWorldsTargetParams = struct {
    directory: []const u8 = core.server_store.default_root,
    id: []const u8,
    world: []const u8,
    minecraft_version: ?[]const u8 = null,
};

const WorldsError = struct {
    code: []const u8,
    message: []const u8,
};

fn mapWorldsError(err: anyerror, fallback_code: []const u8, fallback_message: []const u8) WorldsError {
    return switch (err) {
        error.InvalidName => .{ .code = "INVALID_PARAMS", .message = "world name is invalid" },
        error.WorldNotFound => .{ .code = "WORLD_NOT_FOUND", .message = "No world matches the given name" },
        error.WorldExists => .{ .code = "WORLD_EXISTS", .message = "A world with that name already exists" },
        error.WorldLocked => .{ .code = "WORLD_LOCKED", .message = "The world is in use by a running game" },
        error.InvalidLevelData => .{ .code = "WORLD_INVALID_LEVEL_DATA", .message = "The world level.dat is malformed" },
        error.IconTooLarge => .{ .code = "WORLD_ICON_TOO_LARGE", .message = "The world icon exceeds the size limit" },
        error.UnsupportedEntry => .{ .code = "WORLD_COPY_FAILED", .message = "The world contains unsupported entries" },
        error.BackupNotFound => .{ .code = "BACKUP_NOT_FOUND", .message = "The backup file does not exist" },
        error.ArchiveTooLarge, error.TooManyEntries, error.FileTooLarge, error.EntryNameTooLong => .{ .code = "WORLD_ARCHIVE_TOO_LARGE", .message = "The archive exceeds the supported size limits" },
        error.ArchiveInvalid, error.EntryNameInvalid => .{ .code = "WORLD_ARCHIVE_INVALID", .message = "The archive is not a valid world zip" },
        else => .{ .code = fallback_code, .message = fallback_message },
    };
}

fn writeWorldEntry(
    json: *std.json.Stringify,
    info: core.worlds.WorldInfo,
    minecraft_version: ?[]const u8,
) std.Io.Writer.Error!void {
    try json.beginObject();
    try json.objectField("name");
    try json.write(info.name);
    try json.objectField("level_name");
    try json.write(if (info.meta) |meta| meta.level_name else null);
    try json.objectField("version_name");
    try json.write(if (info.meta) |meta| meta.version_name else null);
    try json.objectField("version_id");
    try json.write(if (info.meta) |meta| meta.version_id else null);
    try json.objectField("game_mode");
    try json.write(if (info.meta) |meta| if (meta.game_mode) |mode| @tagName(mode) else null else null);
    try json.objectField("hardcore");
    try json.write(if (info.meta) |meta| meta.hardcore else null);
    try json.objectField("cheats");
    try json.write(if (info.meta) |meta| meta.cheats else null);
    try json.objectField("difficulty");
    try json.write(if (info.meta) |meta| meta.difficulty else null);
    try json.objectField("last_played_ms");
    try json.write(if (info.meta) |meta| meta.last_played_ms else null);
    try json.objectField("size_bytes");
    try json.write(info.size_bytes);
    try json.objectField("has_icon");
    try json.write(info.has_icon);
    try json.objectField("locked");
    try json.write(info.locked);
    try json.objectField("version_relation");
    const world_version = if (info.meta) |meta| meta.version_name else null;
    try json.write(@tagName(core.worlds.versionRelation(world_version, minecraft_version)));
    try json.endObject();
}

fn parseParams(
    comptime Params: type,
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
    what: []const u8,
) std.Io.Writer.Error!?std.json.Parsed(Params) {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "missing parameters");
        return null;
    };
    const params = std.json.parseFromValue(Params, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        var message_buffer: [128]u8 = undefined;
        const message = std.fmt.bufPrint(&message_buffer, "{s} parameters are invalid", .{what}) catch "parameters are invalid";
        try writeError(writer, request_id, "INVALID_PARAMS", message);
        return null;
    };
    return params;
}

pub fn writeWorldsListResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params = try parseParams(WorldsListParams, service, writer, request_id, params_value, "worlds list") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "worlds listing directory must not be empty");
        return;
    }
    var worlds = service.listWorlds(params.value.directory) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_LIST_FAILED", "Failed to list worlds");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };
    defer worlds.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("worlds.list");
    try json.objectField("worlds");
    try json.beginArray();
    for (worlds.worlds) |info| try writeWorldEntry(&json, info, params.value.minecraft_version);
    try json.endArray();
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeWorldsGetResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params = try parseParams(WorldsTargetParams, service, writer, request_id, params_value, "world lookup") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.world.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "world lookup requires directory and world");
        return;
    }
    var worlds = service.getWorld(params.value.directory, params.value.world) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_GET_FAILED", "Failed to inspect the world");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };
    defer worlds.deinit();
    const icon = service.readWorldIcon(params.value.directory, params.value.world) catch |err| blk: {
        break :blk switch (err) {
            error.WorldNotFound, error.FileNotFound => null,
            else => {
                const mapped = mapWorldsError(err, "WORLD_GET_FAILED", "Failed to read the world icon");
                try writeError(writer, request_id, mapped.code, mapped.message);
                return;
            },
        };
    };
    defer if (icon) |bytes| service.allocator.free(bytes);

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("worlds.get");
    try json.objectField("world");
    try writeWorldEntry(&json, worlds.worlds[0], params.value.minecraft_version);
    try json.objectField("icon_png_base64");
    if (icon) |bytes| {
        var encoded: [std.base64.standard.Encoder.calcSize(core.worlds.max_icon_bytes)]u8 = undefined;
        try json.write(std.base64.standard.Encoder.encode(&encoded, bytes));
    } else {
        try json.write(@as(?[]const u8, null));
    }
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeMutationResult(
    comptime stage: []const u8,
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
    fallback_code: []const u8,
    fallback_message: []const u8,
    mutate: fn (service: *core.Service, params: WorldsMutationParams) anyerror!void,
) std.Io.Writer.Error!void {
    var params = try parseParams(WorldsMutationParams, service, writer, request_id, params_value, stage) orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.world.len == 0 or input.new_name.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "world mutation requires directory, world, and new_name");
        return;
    }
    mutate(service, input) catch |err| {
        const mapped = mapWorldsError(err, fallback_code, fallback_message);
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };
    var worlds = service.getWorld(input.directory, input.new_name) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_GET_FAILED", "Failed to inspect the world");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };
    defer worlds.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write(stage);
    try json.objectField("world");
    try writeWorldEntry(&json, worlds.worlds[0], input.minecraft_version);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeWorldsRenameResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    try writeMutationResult("worlds.rename", service, writer, request_id, params_value, "WORLD_RENAME_FAILED", "Failed to rename the world", struct {
        fn mutate(svc: *core.Service, input: WorldsMutationParams) !void {
            try svc.renameWorld(input.directory, input.world, input.new_name);
        }
    }.mutate);
}

pub fn writeWorldsDuplicateResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    try writeMutationResult("worlds.duplicate", service, writer, request_id, params_value, "WORLD_COPY_FAILED", "Failed to duplicate the world", struct {
        fn mutate(svc: *core.Service, input: WorldsMutationParams) !void {
            try svc.duplicateWorld(input.directory, input.world, input.new_name);
        }
    }.mutate);
}

pub fn writeWorldsDeleteResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params = try parseParams(WorldsTargetParams, service, writer, request_id, params_value, "world deletion") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.world.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "world deletion requires directory and world");
        return;
    }
    service.deleteWorld(params.value.directory, params.value.world) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_DELETE_FAILED", "Failed to delete the world");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("worlds.delete");
    try json.objectField("deleted");
    try json.write(params.value.world);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn openServerWorldContext(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    directory: []const u8,
    id: []const u8,
    root: *std.Io.Dir,
    server_dir: *std.Io.Dir,
) std.Io.Writer.Error!bool {
    root.* = core.server_store.openRoot(service.io, directory) catch {
        try writeError(writer, request_id, "SERVERS_ROOT_ERROR", "Failed to open the servers root");
        return false;
    };
    var manifest = core.server_store.load(service.allocator, service.io, root.*, id) catch |err| switch (err) {
        error.FileNotFound, error.InvalidServerId => {
            root.close(service.io);
            try writeError(writer, request_id, "SERVER_NOT_FOUND", "No server matches the given id");
            return false;
        },
        else => {
            root.close(service.io);
            try writeError(writer, request_id, "SERVER_LOAD_FAILED", "Failed to load the server");
            return false;
        },
    };
    defer manifest.deinit();
    server_dir.* = core.server_store.openServer(service.io, root.*, id) catch {
        root.close(service.io);
        try writeError(writer, request_id, "SERVER_ROOT_ERROR", "Failed to open the server directory");
        return false;
    };
    return true;
}

fn serverWorldNames(
    allocator: std.mem.Allocator,
    io: std.Io,
    server_dir: std.Io.Dir,
    names: *[3][]const u8,
) !usize {
    var level_name: []const u8 = "world";
    var properties = core.server_properties.load(allocator, io, server_dir) catch null;
    defer if (properties) |*document| document.deinit();
    if (properties) |*document| {
        for (document.entries.items) |entry| {
            if (std.mem.eql(u8, entry.key, "level-name")) {
                level_name = entry.value;
                break;
            }
        }
    }
    names[0] = level_name;
    names[1] = try std.fmt.allocPrint(allocator, "{s}_nether", .{level_name});
    names[2] = try std.fmt.allocPrint(allocator, "{s}_the_end", .{level_name});
    return 3;
}

pub fn writeServerWorldsListResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerWorldsListParams, service, writer, request_id, params_value, "server worlds listing") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server worlds listing requires directory and id");
        return;
    }
    var root: std.Io.Dir = undefined;
    var server_dir: std.Io.Dir = undefined;
    if (!try openServerWorldContext(service, writer, request_id, params.value.directory, params.value.id, &root, &server_dir)) return;
    defer root.close(service.io);
    defer server_dir.close(service.io);

    var name_arena = std.heap.ArenaAllocator.init(service.allocator);
    defer name_arena.deinit();
    var names: [3][]const u8 = undefined;
    const count = serverWorldNames(name_arena.allocator(), service.io, server_dir, &names) catch {
        try writeError(writer, request_id, "WORLD_LIST_FAILED", "Failed to resolve the server world names");
        return;
    };
    var worlds = core.worlds.collect(service.allocator, service.io, server_dir, names[0..count]) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_LIST_FAILED", "Failed to list server worlds");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };
    defer worlds.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.worlds.list");
    try json.objectField("id");
    try json.write(params.value.id);
    try json.objectField("worlds");
    try json.beginArray();
    for (worlds.worlds) |info| try writeWorldEntry(&json, info, params.value.minecraft_version);
    try json.endArray();
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

const WorldsExportParams = struct {
    directory: []const u8 = core.instance_store.default_root,
    world: []const u8,
    file: []const u8,
};

const WorldsImportParams = struct {
    directory: []const u8 = core.instance_store.default_root,
    file: []const u8,
    name: ?[]const u8 = null,
    replace: bool = false,
    minecraft_version: ?[]const u8 = null,
};

const WorldsBackupParams = struct {
    directory: []const u8 = core.instance_store.default_root,
    world: []const u8,
    label: ?[]const u8 = null,
};

const WorldsRestoreParams = struct {
    directory: []const u8 = core.instance_store.default_root,
    backup: []const u8,
    name: ?[]const u8 = null,
    replace: bool = false,
    minecraft_version: ?[]const u8 = null,
};

const WorldsBackupTargetParams = struct {
    directory: []const u8 = core.instance_store.default_root,
    backup: []const u8,
};

pub fn writeWorldsExportResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params = try parseParams(WorldsExportParams, service, writer, request_id, params_value, "world export") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.world.len == 0 or params.value.file.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "world export requires directory, world, and file");
        return;
    }
    const result = service.exportWorld(params.value.directory, params.value.world, params.value.file) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_EXPORT_FAILED", "Failed to export the world");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("worlds.export");
    try json.objectField("file");
    try json.write(params.value.file);
    try json.objectField("size_bytes");
    try json.write(result.size_bytes);
    try json.objectField("files");
    try json.write(result.files);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeWorldsImportResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params = try parseParams(WorldsImportParams, service, writer, request_id, params_value, "world import") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.file.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "world import requires directory and file");
        return;
    }
    const name = service.importWorld(params.value.directory, params.value.file, params.value.name, params.value.replace) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_IMPORT_FAILED", "Failed to import the world");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };
    defer service.allocator.free(name);
    var worlds = service.getWorld(params.value.directory, name) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_GET_FAILED", "Failed to inspect the imported world");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };
    defer worlds.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("worlds.import");
    try json.objectField("world");
    try writeWorldEntry(&json, worlds.worlds[0], params.value.minecraft_version);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeWorldsBackupResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params = try parseParams(WorldsBackupParams, service, writer, request_id, params_value, "world backup") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.world.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "world backup requires directory and world");
        return;
    }
    const file = service.backupWorld(params.value.directory, params.value.world, params.value.label) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_BACKUP_FAILED", "Failed to back up the world");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };
    defer service.allocator.free(file);

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("worlds.backup");
    try json.objectField("backup");
    try json.write(file);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeBackupEntry(json: *std.json.Stringify, backup: core.worlds.BackupInfo) std.Io.Writer.Error!void {
    try json.beginObject();
    try json.objectField("file");
    try json.write(backup.file);
    try json.objectField("world");
    if (backup.world) |world| {
        try json.write(world);
    } else {
        try json.write(@as(?[]const u8, null));
    }
    try json.objectField("created_ms");
    try json.write(backup.created_ms);
    try json.objectField("size_bytes");
    try json.write(backup.size_bytes);
    try json.endObject();
}

pub fn writeWorldsBackupsResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params = try parseParams(WorldsListParams, service, writer, request_id, params_value, "world backups listing") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "world backups listing directory must not be empty");
        return;
    }
    var backups = service.listWorldBackups(params.value.directory) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_BACKUP_FAILED", "Failed to list world backups");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };
    defer backups.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("worlds.backups");
    try json.objectField("backups");
    try json.beginArray();
    for (backups.backups) |backup| try writeBackupEntry(&json, backup);
    try json.endArray();
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeWorldsRestoreResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params = try parseParams(WorldsRestoreParams, service, writer, request_id, params_value, "world restore") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.backup.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "world restore requires directory and backup");
        return;
    }
    const name = service.restoreWorldBackup(params.value.directory, params.value.backup, params.value.name, params.value.replace) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_RESTORE_FAILED", "Failed to restore the backup");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };
    defer service.allocator.free(name);
    var worlds = service.getWorld(params.value.directory, name) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_GET_FAILED", "Failed to inspect the restored world");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };
    defer worlds.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("worlds.restore");
    try json.objectField("world");
    try writeWorldEntry(&json, worlds.worlds[0], params.value.minecraft_version);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeWorldsBackupDeleteResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params = try parseParams(WorldsBackupTargetParams, service, writer, request_id, params_value, "world backup deletion") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.backup.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "world backup deletion requires directory and backup");
        return;
    }
    service.deleteWorldBackup(params.value.directory, params.value.backup) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_BACKUP_FAILED", "Failed to delete the backup");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("worlds.backups.delete");
    try json.objectField("deleted");
    try json.write(params.value.backup);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

const ServerWorldsRenameParams = struct {
    directory: []const u8 = core.server_store.default_root,
    id: []const u8,
    world: []const u8,
    new_name: []const u8,
};

const ServerWorldsBackupParams = struct {
    directory: []const u8 = core.server_store.default_root,
    id: []const u8,
    world: ?[]const u8 = null,
    label: ?[]const u8 = null,
};

const ServerWorldsRestoreParams = struct {
    directory: []const u8 = core.server_store.default_root,
    id: []const u8,
    backup: []const u8,
    replace: bool = false,
};

const ServerWorldsBackupTargetParams = struct {
    directory: []const u8 = core.server_store.default_root,
    id: []const u8,
    backup: []const u8,
};

fn writeServerWorldFailure(writer: *std.Io.Writer, request_id: []const u8, err: anyerror, fallback_code: []const u8, fallback_message: []const u8) std.Io.Writer.Error!void {
    switch (err) {
        error.ServerNotFound => try writeError(writer, request_id, "SERVER_NOT_FOUND", "No server matches the given id"),
        error.ServerAlreadyRunning => try writeError(writer, request_id, "SERVER_ALREADY_RUNNING", "Stop the server before modifying its worlds"),
        else => {
            const mapped = mapWorldsError(err, fallback_code, fallback_message);
            try writeError(writer, request_id, mapped.code, mapped.message);
        },
    }
}

pub fn writeServerWorldsRenameResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerWorldsRenameParams, service, writer, request_id, params_value, "server world rename") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.world.len == 0 or input.new_name.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server world rename requires directory, id, world, and new_name");
        return;
    }
    service.serverRenameWorld(input.directory, input.id, input.world, input.new_name) catch |err| {
        try writeServerWorldFailure(writer, request_id, err, "WORLD_RENAME_FAILED", "Failed to rename the server world");
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.worlds.rename");
    try json.objectField("renamed");
    try json.beginObject();
    try json.objectField("old_name");
    try json.write(input.world);
    try json.objectField("new_name");
    try json.write(input.new_name);
    try json.endObject();
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeServerWorldsDeleteResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerWorldsTargetParams, service, writer, request_id, params_value, "server world deletion") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.world.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server world deletion requires directory, id, and world");
        return;
    }
    service.serverDeleteWorld(input.directory, input.id, input.world) catch |err| {
        try writeServerWorldFailure(writer, request_id, err, "WORLD_DELETE_FAILED", "Failed to delete the server world");
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.worlds.delete");
    try json.objectField("deleted");
    try json.write(input.world);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeServerWorldsBackupResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerWorldsBackupParams, service, writer, request_id, params_value, "server worlds backup") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server worlds backup requires directory and id");
        return;
    }
    const file = service.serverBackupWorlds(input.directory, input.id, input.world, input.label) catch |err| {
        try writeServerWorldFailure(writer, request_id, err, "WORLD_BACKUP_FAILED", "Failed to back up the server worlds");
        return;
    };
    defer service.allocator.free(file);

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.worlds.backup");
    try json.objectField("backup");
    try json.write(file);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeServerWorldsBackupsResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerWorldsListParams, service, writer, request_id, params_value, "server backups listing") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server backups listing requires directory and id");
        return;
    }
    var backups = service.serverListWorldBackups(input.directory, input.id) catch |err| {
        try writeServerWorldFailure(writer, request_id, err, "WORLD_BACKUP_FAILED", "Failed to list backups");
        return;
    };
    defer backups.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.worlds.backups");
    try json.objectField("id");
    try json.write(input.id);
    try json.objectField("backups");
    try json.beginArray();
    for (backups.backups) |backup| try writeBackupEntry(&json, backup);
    try json.endArray();
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeServerWorldsRestoreResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerWorldsRestoreParams, service, writer, request_id, params_value, "server worlds restore") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.backup.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server worlds restore requires directory, id, and backup");
        return;
    }
    service.serverRestoreWorlds(input.directory, input.id, input.backup, input.replace) catch |err| {
        try writeServerWorldFailure(writer, request_id, err, "WORLD_RESTORE_FAILED", "Failed to restore the backup");
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.worlds.restore");
    try json.objectField("restored");
    try json.write(input.backup);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeServerWorldsBackupDeleteResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerWorldsBackupTargetParams, service, writer, request_id, params_value, "server backup deletion") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.backup.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server backup deletion requires directory, id, and backup");
        return;
    }
    service.serverDeleteWorldBackup(input.directory, input.id, input.backup) catch |err| {
        try writeServerWorldFailure(writer, request_id, err, "WORLD_BACKUP_FAILED", "Failed to delete the backup");
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.worlds.backups.delete");
    try json.objectField("deleted");
    try json.write(input.backup);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeServerWorldsGetResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerWorldsTargetParams, service, writer, request_id, params_value, "server world lookup") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.id.len == 0 or params.value.world.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server world lookup requires directory, id, and world");
        return;
    }
    var root: std.Io.Dir = undefined;
    var server_dir: std.Io.Dir = undefined;
    if (!try openServerWorldContext(service, writer, request_id, params.value.directory, params.value.id, &root, &server_dir)) return;
    defer root.close(service.io);
    defer server_dir.close(service.io);

    var worlds = core.worlds.get(service.allocator, service.io, server_dir, params.value.world) catch |err| {
        const mapped = mapWorldsError(err, "WORLD_GET_FAILED", "Failed to inspect the server world");
        try writeError(writer, request_id, mapped.code, mapped.message);
        return;
    };
    defer worlds.deinit();
    const icon = core.worlds.readIcon(service.allocator, service.io, server_dir, params.value.world) catch |err| blk: {
        break :blk switch (err) {
            else => null,
        };
    };
    defer if (icon) |bytes| service.allocator.free(bytes);

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.worlds.get");
    try json.objectField("id");
    try json.write(params.value.id);
    try json.objectField("world");
    try writeWorldEntry(&json, worlds.worlds[0], params.value.minecraft_version);
    try json.objectField("icon_png_base64");
    if (icon) |bytes| {
        var encoded: [std.base64.standard.Encoder.calcSize(core.worlds.max_icon_bytes)]u8 = undefined;
        try json.write(std.base64.standard.Encoder.encode(&encoded, bytes));
    } else {
        try json.write(@as(?[]const u8, null));
    }
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}
