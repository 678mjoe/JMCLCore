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
