const std = @import("std");
const jmcl = @import("jmcl_core");
const cli_install = @import("install.zig");

pub fn runInstanceDatapacks(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "list")) return runList(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "install")) return runInstall(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "set-version")) return runSetVersion(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "enable")) return runSetEnabled(service, args[1..], stdout, stderr, true);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "disable")) return runSetEnabled(service, args[1..], stdout, stderr, false);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "remove")) return runRemove(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "adopt")) return runAdopt(service, args[1..], stdout, stderr);
    try stderr.writeAll("usage: jmcl-core instance datapacks <list|install|set-version|enable|disable|remove|adopt> ID --world NAME [options]\n");
    return 2;
}

const WorldTarget = struct {
    id: []const u8,
    world: ?[]const u8 = null,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
};

fn instanceVersionAlloc(
    service: *jmcl.Service,
    options: WorldTarget,
    stderr: *std.Io.Writer,
) !?[]const u8 {
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
    return try service.allocator.dupe(u8, document.value.version_id);
}

fn runList(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance datapacks list ID --world NAME [--instances-directory PATH] [--json]\n");
        return 2;
    }
    var options: WorldTarget = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    var json = false;
    var index: usize = 1;
    while (index < args.len) {
        if (std.mem.eql(u8, args[index], "--json")) {
            json = true;
            index += 1;
            continue;
        }
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--world")) {
            options.world = value;
        } else if (std.mem.eql(u8, argument, "--instances-directory")) {
            options.instances_directory = value;
        } else return error.InvalidArguments;
        index += 2;
    }
    const world = options.world orelse {
        try stderr.writeAll("instance datapacks list requires --world\n");
        return 2;
    };

    const path = try jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id);
    defer service.allocator.free(path);
    var list = service.listWorldDatapacks(path, world) catch |err| {
        try writeWorldDatapackError(stderr, err, "list");
        return 1;
    };
    defer list.deinit();

    if (json) {
        var writer: std.json.Stringify = .{ .writer = stdout };
        try writer.beginObject();
        try writer.objectField("entries");
        try writer.write(list.entries);
        try writer.objectField("unmanaged");
        try writer.write(list.unmanaged);
        try writer.endObject();
        try stdout.writeByte('\n');
        return 0;
    }
    for (list.entries) |entry| {
        try stdout.print("{s}\t{s}\t{s}\t{d}\t{s}\t{s}\t{s}\t{s}\n", .{
            @tagName(entry.kind),
            entry.file,
            entry.sha1,
            entry.size,
            @tagName(entry.source.provider),
            entry.source.slug orelse "-",
            if (entry.enabled) "enabled" else "disabled",
            if (entry.as_dependency) "dependency" else "explicit",
        });
    }
    for (list.unmanaged) |unmanaged| {
        try stdout.print("unmanaged\t{s}\n", .{unmanaged});
    }
    return 0;
}

const InstallOptions = struct {
    id: []const u8,
    world: ?[]const u8 = null,
    project: ?[]const u8 = null,
    version_id: ?[]const u8 = null,
    provider: jmcl.content_registry.Provider = .modrinth,
    api_key: ?[]const u8 = null,
    with_dependencies: bool = true,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    store_directory: []const u8 = jmcl.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

fn runInstall(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance datapacks install ID --world NAME --project SLUG [--version ID] [--provider modrinth|curseforge] [--api-key KEY] [--no-dependencies] [--instances-directory PATH] [--store-directory PATH] [--workers N] [--retries N]\n");
        return 2;
    }
    var options: InstallOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    var index: usize = 1;
    while (index < args.len) {
        if (std.mem.eql(u8, args[index], "--no-dependencies")) {
            options.with_dependencies = false;
            index += 1;
            continue;
        }
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--world")) {
            options.world = value;
        } else if (std.mem.eql(u8, argument, "--project")) {
            options.project = value;
        } else if (std.mem.eql(u8, argument, "--version")) {
            options.version_id = value;
        } else if (std.mem.eql(u8, argument, "--provider")) {
            if (std.mem.eql(u8, value, "modrinth")) {
                options.provider = .modrinth;
            } else if (std.mem.eql(u8, value, "curseforge")) {
                options.provider = .curseforge;
            } else return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--api-key")) {
            options.api_key = value;
        } else if (std.mem.eql(u8, argument, "--instances-directory")) {
            options.instances_directory = value;
        } else if (std.mem.eql(u8, argument, "--store-directory")) {
            options.store_directory = value;
        } else if (std.mem.eql(u8, argument, "--workers")) {
            options.workers = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
            if (options.workers == 0 or options.workers > 32) return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--retries")) {
            options.retries = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
            if (options.retries > 5) return error.InvalidArguments;
        } else return error.InvalidArguments;
        index += 2;
    }
    const world = options.world orelse {
        try stderr.writeAll("instance datapacks install requires --world\n");
        return 2;
    };
    const project = options.project orelse {
        try stderr.writeAll("instance datapacks install requires --project\n");
        return 2;
    };
    switch (options.provider) {
        .curseforge => {
            if (options.api_key == null) {
                try stderr.writeAll("curseforge installs require --api-key (supplied by the embedding application)\n");
                return 2;
            }
        },
        .modrinth => {
            if (options.api_key != null) {
                try stderr.writeAll("--api-key is only used with --provider curseforge\n");
                return 2;
            }
        },
        .manual => return error.InvalidArguments,
    }

    const game_version = (try instanceVersionAlloc(service, .{ .id = options.id, .world = world, .instances_directory = options.instances_directory }, stderr)) orelse return 1;
    defer service.allocator.free(game_version);
    const path = try jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id);
    defer service.allocator.free(path);
    const store = jmcl.artifact_store.openRoot(service.io, options.store_directory) catch |err| {
        try stderr.print("failed to open artifact store: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer store.close(service.io);

    var reporter_context: cli_install.CliInstallReporter = .{
        .io = service.io,
        .writer = stderr,
    };
    var summary = switch (options.provider) {
        .modrinth => service.installWorldModrinthDatapack(
            path,
            world,
            store,
            game_version,
            project,
            options.version_id,
            .{ .workers = options.workers, .retries = options.retries, .with_dependencies = options.with_dependencies },
            .{ .context = &reporter_context, .callback = cli_install.CliInstallReporter.callback },
        ),
        .curseforge => service.installWorldCurseforgeDatapack(
            path,
            world,
            store,
            game_version,
            options.api_key.?,
            project,
            options.version_id,
            .{ .workers = options.workers, .retries = options.retries, .with_dependencies = options.with_dependencies },
            .{ .context = &reporter_context, .callback = cli_install.CliInstallReporter.callback },
        ),
        .manual => unreachable,
    } catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        try writeWorldDatapackError(stderr, err, "install");
        return 1;
    };
    defer summary.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    for (summary.installed) |installed| {
        try stdout.print("installed\t{s}\t{s}\t{s}\t{s}\n", .{
            installed.file,
            installed.sha1,
            installed.slug orelse "-",
            if (installed.as_dependency) "dependency" else "explicit",
        });
    }
    try stdout.print("skipped\t{d}\n", .{summary.skipped});
    return 0;
}

const SetVersionOptions = struct {
    id: []const u8,
    world: ?[]const u8 = null,
    project: ?[]const u8 = null,
    version_id: ?[]const u8 = null,
    api_key: ?[]const u8 = null,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    store_directory: []const u8 = jmcl.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

fn runSetVersion(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance datapacks set-version ID --world NAME --project REF --version ID [--api-key KEY] [--instances-directory PATH] [--store-directory PATH] [--workers N] [--retries N]\n");
        return 2;
    }
    var options: SetVersionOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--world")) {
            options.world = value;
        } else if (std.mem.eql(u8, argument, "--project")) {
            options.project = value;
        } else if (std.mem.eql(u8, argument, "--version")) {
            options.version_id = value;
        } else if (std.mem.eql(u8, argument, "--api-key")) {
            options.api_key = value;
        } else if (std.mem.eql(u8, argument, "--instances-directory")) {
            options.instances_directory = value;
        } else if (std.mem.eql(u8, argument, "--store-directory")) {
            options.store_directory = value;
        } else if (std.mem.eql(u8, argument, "--workers")) {
            options.workers = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
            if (options.workers == 0 or options.workers > 32) return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--retries")) {
            options.retries = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
            if (options.retries > 5) return error.InvalidArguments;
        } else return error.InvalidArguments;
        index += 2;
    }
    const world = options.world orelse {
        try stderr.writeAll("instance datapacks set-version requires --world\n");
        return 2;
    };
    if (options.project == null or options.version_id == null) {
        try stderr.writeAll("instance datapacks set-version requires --project and --version\n");
        return 2;
    }

    const game_version = (try instanceVersionAlloc(service, .{ .id = options.id, .world = world, .instances_directory = options.instances_directory }, stderr)) orelse return 1;
    defer service.allocator.free(game_version);
    const path = try jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id);
    defer service.allocator.free(path);
    const store = jmcl.artifact_store.openRoot(service.io, options.store_directory) catch |err| {
        try stderr.print("failed to open artifact store: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer store.close(service.io);

    var reporter_context: cli_install.CliInstallReporter = .{
        .io = service.io,
        .writer = stderr,
    };
    var change = service.setWorldDatapackVersion(
        path,
        world,
        store,
        game_version,
        options.project.?,
        options.version_id.?,
        options.api_key,
        .{ .workers = options.workers, .retries = options.retries },
        .{ .context = &reporter_context, .callback = cli_install.CliInstallReporter.callback },
    ) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        try writeWorldDatapackError(stderr, err, "set-version");
        return 1;
    };
    defer change.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    if (change.changed) {
        try stdout.print("changed\t{s}\t{s}\t{s}\t{s}\n", .{ change.file, change.sha1, change.old_version_id, change.new_version_id });
    } else {
        try stdout.print("unchanged\t{s}\t{s}\n", .{ change.file, change.new_version_id });
    }
    return 0;
}

const SelectorOptions = struct {
    id: []const u8,
    world: ?[]const u8 = null,
    project: ?[]const u8 = null,
    file: ?[]const u8 = null,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
};

fn parseSelectorArgs(options: *SelectorOptions, args: []const []const u8) !void {
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--world")) {
            options.world = value;
        } else if (std.mem.eql(u8, argument, "--project")) {
            options.project = value;
        } else if (std.mem.eql(u8, argument, "--file")) {
            options.file = value;
        } else if (std.mem.eql(u8, argument, "--instances-directory")) {
            options.instances_directory = value;
        } else return error.InvalidArguments;
        index += 2;
    }
}

fn runSetEnabled(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
    enabled: bool,
) !u8 {
    const verb = if (enabled) "enable" else "disable";
    if (args.len == 0) {
        try stderr.print("usage: jmcl-core instance datapacks {s} ID --world NAME (--project REF | --file PATH) [--instances-directory PATH]\n", .{verb});
        return 2;
    }
    var options: SelectorOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseSelectorArgs(&options, args);
    const world = options.world orelse {
        try stderr.print("instance datapacks {s} requires --world\n", .{verb});
        return 2;
    };
    if ((options.project == null) == (options.file == null)) {
        try stderr.print("instance datapacks {s} requires exactly one of --project or --file\n", .{verb});
        return 2;
    }

    const path = try jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id);
    defer service.allocator.free(path);
    var toggle = service.setWorldDatapackEnabled(path, world, options.project, options.file, enabled) catch |err| {
        try writeWorldDatapackError(stderr, err, verb);
        return 1;
    };
    defer toggle.deinit();

    if (toggle.changed) {
        try stdout.print("{s}\t{s}\n", .{ if (enabled) "enabled" else "disabled", toggle.entry.file });
    } else {
        try stdout.print("unchanged\t{s}\t{s}\n", .{ toggle.entry.file, if (enabled) "enabled" else "disabled" });
    }
    return 0;
}

fn runRemove(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance datapacks remove ID --world NAME (--project REF | --file PATH) [--instances-directory PATH]\n");
        return 2;
    }
    var options: SelectorOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    try parseSelectorArgs(&options, args);
    const world = options.world orelse {
        try stderr.writeAll("instance datapacks remove requires --world\n");
        return 2;
    };
    if ((options.project == null) == (options.file == null)) {
        try stderr.writeAll("instance datapacks remove requires exactly one of --project or --file\n");
        return 2;
    }

    const path = try jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id);
    defer service.allocator.free(path);
    var removal = service.removeWorldDatapack(path, world, options.project, options.file) catch |err| {
        try writeWorldDatapackError(stderr, err, "remove");
        return 1;
    };
    defer removal.deinit();
    try stdout.print("removed\t{s}\t{s}\n", .{ removal.entry.file, removal.entry.sha1 });
    return 0;
}

const AdoptOptions = struct {
    id: []const u8,
    world: ?[]const u8 = null,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    store_directory: []const u8 = jmcl.artifact_store.default_directory,
};

fn runAdopt(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance datapacks adopt ID --world NAME [--instances-directory PATH] [--store-directory PATH]\n");
        return 2;
    }
    var options: AdoptOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--world")) {
            options.world = value;
        } else if (std.mem.eql(u8, argument, "--instances-directory")) {
            options.instances_directory = value;
        } else if (std.mem.eql(u8, argument, "--store-directory")) {
            options.store_directory = value;
        } else return error.InvalidArguments;
        index += 2;
    }
    const world = options.world orelse {
        try stderr.writeAll("instance datapacks adopt requires --world\n");
        return 2;
    };

    const path = try jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id);
    defer service.allocator.free(path);
    const store = jmcl.artifact_store.openRoot(service.io, options.store_directory) catch |err| {
        try stderr.print("failed to open artifact store: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer store.close(service.io);

    var adoption = service.adoptWorldDatapacks(path, world, store) catch |err| {
        try writeWorldDatapackError(stderr, err, "adopt");
        return 1;
    };
    defer adoption.deinit();
    for (adoption.adopted) |adopted| {
        try stdout.print("adopted\t{s}\t{s}\t{d}\n", .{ adopted.file, adopted.sha1, adopted.size });
    }
    try stdout.print("adoptedCount\t{d}\n", .{adoption.adopted.len});
    return 0;
}

fn writeWorldDatapackError(stderr: *std.Io.Writer, err: anyerror, operation: []const u8) !void {
    switch (err) {
        error.WorldNotFound => try stderr.writeAll("no world matches the given name\n"),
        error.WorldLocked => try stderr.writeAll("the world is in use by a running game\n"),
        error.NoCompatibleContentVersion => try stderr.writeAll("no release of the project supports the instance's Minecraft version\n"),
        error.IncompatibleContentVersion => try stderr.writeAll("the requested version does not support the instance's Minecraft version\n"),
        error.ContentNotFound => try stderr.writeAll("no registered datapack matches the given selector\n"),
        error.ModrinthVersionProjectMismatch, error.CurseforgeFileProjectMismatch => try stderr.writeAll("the requested version does not belong to the registered project\n"),
        error.ContentFileConflict => try stderr.writeAll("a different registered content entry already occupies the target file\n"),
        error.ContentKindMismatch => try stderr.writeAll("the project is not a datapack project\n"),
        error.UnsupportedProjectType => try stderr.writeAll("the project type is not supported\n"),
        error.CurseforgeApiKeyRequired => try stderr.writeAll("switching a curseforge datapack requires --api-key (supplied by the embedding application)\n"),
        error.CurseforgeDownloadUnavailable => try stderr.writeAll("the project file is not available for third-party downloads\n"),
        error.InvalidCurseforgeId => try stderr.writeAll("curseforge projects and files use numeric ids\n"),
        error.UnsuccessfulHttpStatus => try stderr.writeAll("the content API returned an unsuccessful HTTP status (check the api key)\n"),
        else => try stderr.print("datapack {s} failed: {s}\n", .{ operation, @errorName(err) }),
    }
}
