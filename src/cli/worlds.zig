const std = @import("std");
const jmcl = @import("jmcl_core");

const InstanceWorldsOptions = struct {
    id: []const u8,
    world: ?[]const u8 = null,
    new_name: ?[]const u8 = null,
    name: ?[]const u8 = null,
    label: ?[]const u8 = null,
    replace: bool = false,
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
        } else if (std.mem.eql(u8, argument, "--name") and index + 1 < args.len) {
            options.name = args[index + 1];
            index += 2;
        } else if (std.mem.eql(u8, argument, "--label") and index + 1 < args.len) {
            options.label = args[index + 1];
            index += 2;
        } else if (std.mem.eql(u8, argument, "--replace")) {
            options.replace = true;
            index += 1;
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
        error.BackupNotFound => "backup file not found",
        error.ArchiveTooLarge, error.TooManyEntries, error.FileTooLarge, error.EntryNameTooLong => "the archive exceeds the supported size limits",
        error.ArchiveInvalid, error.EntryNameInvalid => "the archive is not a valid world zip",
        error.ServerNotFound => "server not found",
        error.ServerAlreadyRunning => "stop the server before modifying its worlds",
        error.UnsupportedPlatform => "server world management is only supported on Linux",
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
    if (args.len >= 1 and std.mem.eql(u8, args[0], "export")) return runInstanceExport(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "import")) return runInstanceImport(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "backup")) return runInstanceBackup(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "backups")) return runInstanceBackups(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "restore")) return runInstanceRestore(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "backups-delete")) return runInstanceBackupDelete(service, args[1..], stdout, stderr);
    try stderr.writeAll("usage: jmcl-core instance worlds <list|show|rename|duplicate|delete|export|import|backup|backups|restore|backups-delete> ID [ARGS] [--instances-directory PATH] [--json]\n");
    return 2;
}

fn runInstanceExport(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance worlds export ID WORLD FILE [--instances-directory PATH]\n");
        return 2;
    }
    var options: InstanceWorldsOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseInstanceArgs(&options, args[1..]);
    const world_name = options.world orelse return error.InvalidArguments;
    const file = options.new_name orelse return error.InvalidArguments;

    const context = (try openInstanceContext(service, stderr, options)) orelse return 1;
    defer service.allocator.free(context.minecraft_path);
    defer service.allocator.free(context.minecraft_version);

    const result = service.exportWorld(context.minecraft_path, world_name, file) catch |err| return reportWorldsError(stderr, "export", err);
    try stdout.print("exported {s} -> {s} ({d} bytes, {d} files)\n", .{ world_name, file, result.size_bytes, result.files });
    return 0;
}

fn runInstanceImport(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance worlds import ID FILE [--name NAME] [--replace] [--instances-directory PATH]\n");
        return 2;
    }
    var options: InstanceWorldsOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseInstanceArgs(&options, args[1..]);
    const file = options.world orelse return error.InvalidArguments;

    const context = (try openInstanceContext(service, stderr, options)) orelse return 1;
    defer service.allocator.free(context.minecraft_path);
    defer service.allocator.free(context.minecraft_version);

    const name = service.importWorld(context.minecraft_path, file, options.name, options.replace) catch |err| return reportWorldsError(stderr, "import", err);
    defer service.allocator.free(name);
    try stdout.print("imported {s}\n", .{name});
    return 0;
}

fn runInstanceBackup(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance worlds backup ID WORLD [--label LABEL] [--instances-directory PATH]\n");
        return 2;
    }
    var options: InstanceWorldsOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseInstanceArgs(&options, args[1..]);
    const world_name = options.world orelse return error.InvalidArguments;

    const context = (try openInstanceContext(service, stderr, options)) orelse return 1;
    defer service.allocator.free(context.minecraft_path);
    defer service.allocator.free(context.minecraft_version);

    const file = service.backupWorld(context.minecraft_path, world_name, options.label) catch |err| return reportWorldsError(stderr, "back up", err);
    defer service.allocator.free(file);
    try stdout.print("backup: {s}\n", .{file});
    return 0;
}

fn printBackupRows(stdout: *std.Io.Writer, backups: []const jmcl.worlds.BackupInfo) !void {
    for (backups) |backup| {
        try stdout.print("{s}\t{s}\t{d}\t{d}\n", .{ backup.file, backup.world orelse "-", backup.created_ms, backup.size_bytes });
    }
}

fn runInstanceBackups(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance worlds backups ID [--instances-directory PATH] [--json]\n");
        return 2;
    }
    var options: InstanceWorldsOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseInstanceArgs(&options, args[1..]);
    if (options.world != null) return error.InvalidArguments;

    const context = (try openInstanceContext(service, stderr, options)) orelse return 1;
    defer service.allocator.free(context.minecraft_path);
    defer service.allocator.free(context.minecraft_version);

    var backups = service.listWorldBackups(context.minecraft_path) catch |err| return reportWorldsError(stderr, "list backups for", err);
    defer backups.deinit();

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("backups");
        try json.beginArray();
        for (backups.backups) |backup| {
            try json.beginObject();
            try json.objectField("file");
            try json.write(backup.file);
            try json.objectField("world");
            try json.write(backup.world);
            try json.objectField("created_ms");
            try json.write(backup.created_ms);
            try json.objectField("size_bytes");
            try json.write(backup.size_bytes);
            try json.endObject();
        }
        try json.endArray();
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }
    try printBackupRows(stdout, backups.backups);
    return 0;
}

fn runInstanceRestore(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance worlds restore ID BACKUP [--name NAME] [--replace] [--instances-directory PATH]\n");
        return 2;
    }
    var options: InstanceWorldsOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseInstanceArgs(&options, args[1..]);
    const backup = options.world orelse return error.InvalidArguments;

    const context = (try openInstanceContext(service, stderr, options)) orelse return 1;
    defer service.allocator.free(context.minecraft_path);
    defer service.allocator.free(context.minecraft_version);

    const name = service.restoreWorldBackup(context.minecraft_path, backup, options.name, options.replace) catch |err| return reportWorldsError(stderr, "restore", err);
    defer service.allocator.free(name);
    try stdout.print("restored {s}\n", .{name});
    return 0;
}

fn runInstanceBackupDelete(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance worlds backups-delete ID BACKUP [--instances-directory PATH]\n");
        return 2;
    }
    var options: InstanceWorldsOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseInstanceArgs(&options, args[1..]);
    const backup = options.world orelse return error.InvalidArguments;

    const context = (try openInstanceContext(service, stderr, options)) orelse return 1;
    defer service.allocator.free(context.minecraft_path);
    defer service.allocator.free(context.minecraft_version);

    service.deleteWorldBackup(context.minecraft_path, backup) catch |err| return reportWorldsError(stderr, "delete backup", err);
    try stdout.print("deleted {s}\n", .{backup});
    return 0;
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
    if (args.len >= 1 and std.mem.eql(u8, args[0], "rename")) return runServerRename(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "delete")) return runServerDelete(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "backup")) return runServerBackup(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "backups")) return runServerBackups(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "restore")) return runServerRestore(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "backups-delete")) return runServerBackupDelete(service, args[1..], stdout, stderr);
    try stderr.writeAll("usage: jmcl-core server worlds <list|show|rename|delete|backup|backups|restore|backups-delete> ID [ARGS] [--servers-directory PATH] [--json]\n");
    return 2;
}

fn runServerRename(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len < 3) {
        try stderr.writeAll("usage: jmcl-core server worlds rename ID WORLD NEW_NAME [--servers-directory PATH]\n");
        return 2;
    }
    var options: ServerWorldsOptions = .{ .id = args[0] };
    try parseServerArgs(&options, args[1..]);
    const world_name = options.world orelse return error.InvalidArguments;
    const new_name = options.new_name orelse return error.InvalidArguments;
    service.serverRenameWorld(options.servers_directory, options.id, world_name, new_name) catch |err| return reportWorldsError(stderr, "rename", err);
    try stdout.print("renamed {s} -> {s}\n", .{ world_name, new_name });
    return 0;
}

fn runServerDelete(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len < 2) {
        try stderr.writeAll("usage: jmcl-core server worlds delete ID WORLD [--servers-directory PATH]\n");
        return 2;
    }
    var options: ServerWorldsOptions = .{ .id = args[0] };
    try parseServerArgs(&options, args[1..]);
    const world_name = options.world orelse return error.InvalidArguments;
    service.serverDeleteWorld(options.servers_directory, options.id, world_name) catch |err| return reportWorldsError(stderr, "delete", err);
    try stdout.print("deleted {s}\n", .{world_name});
    return 0;
}

fn runServerBackup(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core server worlds backup ID [WORLD] [--label LABEL] [--servers-directory PATH]\n");
        return 2;
    }
    var options: ServerWorldsOptions = .{ .id = args[0] };
    try parseServerArgs(&options, args[1..]);
    const file = service.serverBackupWorlds(options.servers_directory, options.id, options.world, options.label) catch |err| return reportWorldsError(stderr, "back up", err);
    defer service.allocator.free(file);
    try stdout.print("backup: {s}\n", .{file});
    return 0;
}

fn runServerBackups(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core server worlds backups ID [--servers-directory PATH] [--json]\n");
        return 2;
    }
    var options: ServerWorldsOptions = .{ .id = args[0] };
    try parseServerArgs(&options, args[1..]);
    if (options.world != null) return error.InvalidArguments;
    var backups = service.serverListWorldBackups(options.servers_directory, options.id) catch |err| return reportWorldsError(stderr, "list backups for", err);
    defer backups.deinit();

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("backups");
        try json.beginArray();
        for (backups.backups) |backup| {
            try json.beginObject();
            try json.objectField("file");
            try json.write(backup.file);
            try json.objectField("world");
            try json.write(backup.world);
            try json.objectField("created_ms");
            try json.write(backup.created_ms);
            try json.objectField("size_bytes");
            try json.write(backup.size_bytes);
            try json.endObject();
        }
        try json.endArray();
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }
    try printBackupRows(stdout, backups.backups);
    return 0;
}

fn runServerRestore(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len < 2) {
        try stderr.writeAll("usage: jmcl-core server worlds restore ID BACKUP [--replace] [--servers-directory PATH]\n");
        return 2;
    }
    var options: ServerWorldsOptions = .{ .id = args[0] };
    try parseServerArgs(&options, args[1..]);
    const backup = options.world orelse return error.InvalidArguments;
    service.serverRestoreWorlds(options.servers_directory, options.id, backup, options.replace) catch |err| return reportWorldsError(stderr, "restore", err);
    try stdout.print("restored {s}\n", .{backup});
    return 0;
}

fn runServerBackupDelete(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len < 2) {
        try stderr.writeAll("usage: jmcl-core server worlds backups-delete ID BACKUP [--servers-directory PATH]\n");
        return 2;
    }
    var options: ServerWorldsOptions = .{ .id = args[0] };
    try parseServerArgs(&options, args[1..]);
    const backup = options.world orelse return error.InvalidArguments;
    service.serverDeleteWorldBackup(options.servers_directory, options.id, backup) catch |err| return reportWorldsError(stderr, "delete backup", err);
    try stdout.print("deleted {s}\n", .{backup});
    return 0;
}

const ServerWorldsOptions = struct {
    id: []const u8,
    world: ?[]const u8 = null,
    new_name: ?[]const u8 = null,
    label: ?[]const u8 = null,
    replace: bool = false,
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
        } else if (std.mem.eql(u8, argument, "--label") and index + 1 < args.len) {
            options.label = args[index + 1];
            index += 2;
        } else if (std.mem.eql(u8, argument, "--replace")) {
            options.replace = true;
            index += 1;
        } else if (options.world == null) {
            options.world = argument;
            index += 1;
        } else if (options.new_name == null) {
            options.new_name = argument;
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
