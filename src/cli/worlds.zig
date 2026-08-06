const std = @import("std");
const jmcl = @import("jmcl_core");

const InstanceWorldsOptions = struct {
    id: []const u8,
    world: ?[]const u8 = null,
    new_name: ?[]const u8 = null,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    json: bool = false,
};

fn parseInstanceArgs(
    options: *InstanceWorldsOptions,
    args: []const []const u8,
) !void {
    var index: usize = 0;
    while (index < args.len) {
        const argument = args[index];
        if (std.mem.eql(u8, argument, "--json")) {
            options.json = true;
            index += 1;
        } else if (std.mem.eql(u8, argument, "--instances-directory") and index + 1 < args.len) {
            options.instances_directory = args[index + 1];
            index += 2;
        } else if (options.world == null) {
            options.world = argument;
            index += 1;
        } else if (options.new_name == null) {
            options.new_name = argument;
            index += 1;
        } else return error.InvalidArguments;
    }
}

fn writeWorldJson(
    json: *std.json.Stringify,
    info: jmcl.worlds.WorldInfo,
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
    try json.write(@tagName(jmcl.worlds.versionRelation(world_version, minecraft_version)));
    try json.endObject();
}

fn printWorldRow(stdout: *std.Io.Writer, info: jmcl.worlds.WorldInfo, minecraft_version: ?[]const u8) !void {
    const world_version = if (info.meta) |meta| meta.version_name else null;
    try stdout.print("{s}\t{s}\t{s}\t{s}\t{d}\t{s}\t{s}\n", .{
        info.name,
        if (info.meta) |meta| meta.level_name orelse "-" else "-",
        world_version orelse "-",
        if (info.meta) |meta| if (meta.game_mode) |mode| @tagName(mode) else "-" else "-",
        info.size_bytes,
        if (info.locked) "locked" else "idle",
        @tagName(jmcl.worlds.versionRelation(world_version, minecraft_version)),
    });
}

fn printWorldDetail(stdout: *std.Io.Writer, info: jmcl.worlds.WorldInfo, minecraft_version: ?[]const u8) !void {
    const world_version = if (info.meta) |meta| meta.version_name else null;
    try stdout.print("name: {s}\n", .{info.name});
    if (info.meta) |meta| {
        try stdout.print("level_name: {s}\n", .{meta.level_name orelse "-"});
        try stdout.print("version_name: {s}\n", .{meta.version_name orelse "-"});
        if (meta.version_id) |version_id| try stdout.print("version_id: {d}\n", .{version_id});
        if (meta.game_mode) |mode| try stdout.print("game_mode: {s}\n", .{@tagName(mode)});
        if (meta.hardcore) |hardcore| try stdout.print("hardcore: {}\n", .{hardcore});
        if (meta.cheats) |cheats| try stdout.print("cheats: {}\n", .{cheats});
        if (meta.difficulty) |difficulty| try stdout.print("difficulty: {d}\n", .{difficulty});
        if (meta.last_played_ms) |last_played| try stdout.print("last_played_ms: {d}\n", .{last_played});
    } else {
        try stdout.writeAll("level.dat: unavailable\n");
    }
    try stdout.print("size_bytes: {d}\n", .{info.size_bytes});
    try stdout.print("has_icon: {}\n", .{info.has_icon});
    try stdout.print("locked: {}\n", .{info.locked});
    try stdout.print("version_relation: {s}\n", .{@tagName(jmcl.worlds.versionRelation(world_version, minecraft_version))});
}

fn reportWorldsError(stderr: *std.Io.Writer, operation: []const u8, err: anyerror) !u8 {
    const message: []const u8 = switch (err) {
        error.InvalidName => "invalid world name",
        error.WorldNotFound => "world not found",
        error.WorldExists => "a world with that name already exists",
        error.WorldLocked => "the world is in use by a running game",
        error.InvalidLevelData => "the world level.dat is malformed",
        error.UnsupportedEntry => "the world contains unsupported entries",
        else => @errorName(err),
    };
    try stderr.print("failed to {s} world: {s}\n", .{ operation, message });
    return 1;
}

const InstanceContext = struct {
    minecraft_path: []const u8,
    minecraft_version: []const u8,
};

fn openInstanceContext(
    service: *jmcl.Service,
    stderr: *std.Io.Writer,
    options: InstanceWorldsOptions,
) !?InstanceContext {
    const root = jmcl.instance_store.openRoot(service.io, options.instances_directory) catch |err| {
        try stderr.print("failed to open instances directory: {s}\n", .{@errorName(err)});
        return null;
    };
    defer root.close(service.io);
    var document = jmcl.instance_store.load(service.allocator, service.io, root, options.id) catch |err| {
        try stderr.print("failed to load instance: {s}\n", .{@errorName(err)});
        return null;
    };
    defer document.deinit();
    const path = jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id) catch |err| {
        try stderr.print("failed to resolve instance directory: {s}\n", .{@errorName(err)});
        return null;
    };
    return .{ .minecraft_path = path, .minecraft_version = try service.allocator.dupe(u8, document.value.version_id) };
}

pub fn runInstanceWorlds(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "list")) return runInstanceList(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "show")) return runInstanceShow(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "rename")) return runInstanceRename(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "duplicate")) return runInstanceDuplicate(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "delete")) return runInstanceDelete(service, args[1..], stdout, stderr);
    try stderr.writeAll("usage: jmcl-core instance worlds <list|show|rename|duplicate|delete> ID [WORLD] [NEW_NAME] [--instances-directory PATH] [--json]\n");
    return 2;
}

fn runInstanceList(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance worlds list ID [--instances-directory PATH] [--json]\n");
        return 2;
    }
    var options: InstanceWorldsOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseInstanceArgs(&options, args[1..]);
    if (options.world != null) return error.InvalidArguments;

    const context = (try openInstanceContext(service, stderr, options)) orelse return 1;
    defer service.allocator.free(context.minecraft_path);
    defer service.allocator.free(context.minecraft_version);

    var worlds = service.listWorlds(context.minecraft_path) catch |err| return reportWorldsError(stderr, "list", err);
    defer worlds.deinit();

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("worlds");
        try json.beginArray();
        for (worlds.worlds) |info| try writeWorldJson(&json, info, context.minecraft_version);
        try json.endArray();
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }
    for (worlds.worlds) |info| try printWorldRow(stdout, info, context.minecraft_version);
    return 0;
}

fn runInstanceShow(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance worlds show ID WORLD [--instances-directory PATH] [--json]\n");
        return 2;
    }
    var options: InstanceWorldsOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseInstanceArgs(&options, args[1..]);
    const world_name = options.world orelse return error.InvalidArguments;
    if (options.new_name != null) return error.InvalidArguments;

    const context = (try openInstanceContext(service, stderr, options)) orelse return 1;
    defer service.allocator.free(context.minecraft_path);
    defer service.allocator.free(context.minecraft_version);

    var worlds = service.getWorld(context.minecraft_path, world_name) catch |err| return reportWorldsError(stderr, "inspect", err);
    defer worlds.deinit();

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("world");
        try writeWorldJson(&json, worlds.worlds[0], context.minecraft_version);
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }
    try printWorldDetail(stdout, worlds.worlds[0], context.minecraft_version);
    return 0;
}

fn runInstanceRename(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance worlds rename ID WORLD NEW_NAME [--instances-directory PATH]\n");
        return 2;
    }
    var options: InstanceWorldsOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseInstanceArgs(&options, args[1..]);
    const world_name = options.world orelse return error.InvalidArguments;
    const new_name = options.new_name orelse return error.InvalidArguments;

    const context = (try openInstanceContext(service, stderr, options)) orelse return 1;
    defer service.allocator.free(context.minecraft_path);
    defer service.allocator.free(context.minecraft_version);

    service.renameWorld(context.minecraft_path, world_name, new_name) catch |err| return reportWorldsError(stderr, "rename", err);
    try stdout.print("renamed {s} -> {s}\n", .{ world_name, new_name });
    return 0;
}

fn runInstanceDuplicate(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance worlds duplicate ID WORLD NEW_NAME [--instances-directory PATH]\n");
        return 2;
    }
    var options: InstanceWorldsOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseInstanceArgs(&options, args[1..]);
    const world_name = options.world orelse return error.InvalidArguments;
    const new_name = options.new_name orelse return error.InvalidArguments;

    const context = (try openInstanceContext(service, stderr, options)) orelse return 1;
    defer service.allocator.free(context.minecraft_path);
    defer service.allocator.free(context.minecraft_version);

    service.duplicateWorld(context.minecraft_path, world_name, new_name) catch |err| return reportWorldsError(stderr, "duplicate", err);
    try stdout.print("duplicated {s} -> {s}\n", .{ world_name, new_name });
    return 0;
}

fn runInstanceDelete(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance worlds delete ID WORLD [--instances-directory PATH]\n");
        return 2;
    }
    var options: InstanceWorldsOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseInstanceArgs(&options, args[1..]);
    const world_name = options.world orelse return error.InvalidArguments;
    if (options.new_name != null) return error.InvalidArguments;

    const context = (try openInstanceContext(service, stderr, options)) orelse return 1;
    defer service.allocator.free(context.minecraft_path);
    defer service.allocator.free(context.minecraft_version);

    service.deleteWorld(context.minecraft_path, world_name) catch |err| return reportWorldsError(stderr, "delete", err);
    try stdout.print("deleted {s}\n", .{world_name});
    return 0;
}

fn serverWorldNames(
    allocator: std.mem.Allocator,
    io: std.Io,
    server_dir: std.Io.Dir,
    names: *[3][]const u8,
) !usize {
    var level_name: []const u8 = "world";
    var properties = jmcl.server_properties.load(allocator, io, server_dir) catch null;
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

pub fn runServerWorlds(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "list")) return runServerList(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "show")) return runServerShow(service, args[1..], stdout, stderr);
    try stderr.writeAll("usage: jmcl-core server worlds <list|show> ID [WORLD] [--servers-directory PATH] [--json]\n");
    return 2;
}

const ServerWorldsOptions = struct {
    id: []const u8,
    world: ?[]const u8 = null,
    servers_directory: []const u8 = jmcl.server_store.default_root,
    json: bool = false,
};

fn openServerDir(
    service: *jmcl.Service,
    stderr: *std.Io.Writer,
    options: ServerWorldsOptions,
) !?std.Io.Dir {
    const root = jmcl.server_store.openRoot(service.io, options.servers_directory) catch |err| {
        try stderr.print("failed to open servers directory: {s}\n", .{@errorName(err)});
        return null;
    };
    defer root.close(service.io);
    var manifest = jmcl.server_store.load(service.allocator, service.io, root, options.id) catch |err| {
        try stderr.print("failed to load server: {s}\n", .{@errorName(err)});
        return null;
    };
    defer manifest.deinit();
    const server_dir = jmcl.server_store.openServer(service.io, root, options.id) catch |err| {
        try stderr.print("failed to open server directory: {s}\n", .{@errorName(err)});
        return null;
    };
    return server_dir;
}

fn parseServerArgs(
    options: *ServerWorldsOptions,
    args: []const []const u8,
) !void {
    var index: usize = 0;
    while (index < args.len) {
        const argument = args[index];
        if (std.mem.eql(u8, argument, "--json")) {
            options.json = true;
            index += 1;
        } else if (std.mem.eql(u8, argument, "--servers-directory") and index + 1 < args.len) {
            options.servers_directory = args[index + 1];
            index += 2;
        } else if (options.world == null) {
            options.world = argument;
            index += 1;
        } else return error.InvalidArguments;
    }
}

fn runServerList(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core server worlds list ID [--servers-directory PATH] [--json]\n");
        return 2;
    }
    var options: ServerWorldsOptions = .{ .id = args[0] };
    try parseServerArgs(&options, args[1..]);
    if (options.world != null) return error.InvalidArguments;

    var server_dir = (try openServerDir(service, stderr, options)) orelse return 1;
    defer server_dir.close(service.io);

    var name_arena = std.heap.ArenaAllocator.init(service.allocator);
    defer name_arena.deinit();
    var names: [3][]const u8 = undefined;
    const count = serverWorldNames(name_arena.allocator(), service.io, server_dir, &names) catch |err| {
        try stderr.print("failed to resolve server world names: {s}\n", .{@errorName(err)});
        return 1;
    };
    var worlds = jmcl.worlds.collect(service.allocator, service.io, server_dir, names[0..count]) catch |err| return reportWorldsError(stderr, "list", err);
    defer worlds.deinit();

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("worlds");
        try json.beginArray();
        for (worlds.worlds) |info| try writeWorldJson(&json, info, null);
        try json.endArray();
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }
    for (worlds.worlds) |info| try printWorldRow(stdout, info, null);
    return 0;
}

fn runServerShow(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core server worlds show ID WORLD [--servers-directory PATH] [--json]\n");
        return 2;
    }
    var options: ServerWorldsOptions = .{ .id = args[0] };
    try parseServerArgs(&options, args[1..]);
    const world_name = options.world orelse return error.InvalidArguments;

    var server_dir = (try openServerDir(service, stderr, options)) orelse return 1;
    defer server_dir.close(service.io);

    var worlds = jmcl.worlds.get(service.allocator, service.io, server_dir, world_name) catch |err| return reportWorldsError(stderr, "inspect", err);
    defer worlds.deinit();

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("world");
        try writeWorldJson(&json, worlds.worlds[0], null);
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }
    try printWorldDetail(stdout, worlds.worlds[0], null);
    return 0;
}
