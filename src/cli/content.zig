const std = @import("std");
const jmcl = @import("jmcl_core");
const cli_install = @import("install.zig");

pub fn runWithKind(
    kind: jmcl.content_registry.Kind,
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "list")) return runContentList(kind, service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "install")) return runContentInstall(kind, service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "set-version")) return runContentSetVersion(kind, service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "remove")) return runContentRemove(kind, service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "adopt")) return runContentAdopt(kind, service, args[1..], stdout, stderr);
    try stderr.print("usage: jmcl-core instance {s} <list|install|set-version|remove|adopt> ID [options]\n", .{kind.directory()});
    return 2;
}

fn manifestModLoader(manifest: jmcl.instance_store.Manifest) !jmcl.modrinth.Loader {
    if (manifest.fabric_loader != null) return .fabric;
    if (manifest.neoforge_version != null) return .neoforge;
    if (manifest.forge_version != null) return .forge;
    return error.InstanceHasNoLoader;
}

const ContentListOptions = struct {
    id: []const u8,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    json: bool = false,
};

fn runContentList(
    kind: jmcl.content_registry.Kind,
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.print("usage: jmcl-core instance {s} list ID [--instances-directory PATH] [--json]\n", .{kind.directory()});
        return 2;
    }
    var options: ContentListOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    var index: usize = 1;
    while (index < args.len) {
        if (std.mem.eql(u8, args[index], "--json")) {
            options.json = true;
            index += 1;
        } else if (std.mem.eql(u8, args[index], "--instances-directory") and index + 1 < args.len) {
            options.instances_directory = args[index + 1];
            index += 2;
        } else return error.InvalidArguments;
    }

    const root = jmcl.instance_store.openRoot(service.io, options.instances_directory) catch |err| {
        try stderr.print("failed to open instances directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    var document = jmcl.instance_store.load(service.allocator, service.io, root, options.id) catch |err| {
        try stderr.print("failed to load instance: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer document.deinit();
    const path = try jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id);
    defer service.allocator.free(path);
    const minecraft = jmcl.installer.openRoot(service.io, path) catch |err| {
        try stderr.print("failed to open instance directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer minecraft.close(service.io);

    var list = service.listInstanceContent(minecraft, kind) catch |err| {
        try stderr.print("failed to list instance content: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer list.deinit();

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("entries");
        try json.write(list.entries);
        try json.objectField("unmanaged");
        try json.write(list.unmanaged);
        try json.endObject();
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

const ContentInstallOptions = struct {
    id: []const u8,
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

fn runContentInstall(
    kind: jmcl.content_registry.Kind,
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.print("usage: jmcl-core instance {s} install ID --project SLUG [--version ID] [--provider modrinth|curseforge] [--api-key KEY] [--no-dependencies] [--instances-directory PATH] [--store-directory PATH] [--workers N] [--retries N]\n", .{kind.directory()});
        return 2;
    }
    var options: ContentInstallOptions = .{ .id = args[0] };
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
        if (std.mem.eql(u8, argument, "--project")) {
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
    const project = options.project orelse {
        try stderr.print("instance {s} install requires --project\n", .{kind.directory()});
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

    const root = jmcl.instance_store.openRoot(service.io, options.instances_directory) catch |err| {
        try stderr.print("failed to open instances directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    var document = jmcl.instance_store.load(service.allocator, service.io, root, options.id) catch |err| {
        try stderr.print("failed to load instance: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer document.deinit();
    const loader: ?jmcl.modrinth.Loader = switch (kind) {
        .mod => manifestModLoader(document.value) catch {
            try stderr.writeAll("instance has no mod loader; mods require Fabric, NeoForge, or Forge\n");
            return 1;
        },
        .resourcepack, .shaderpack => null,
    };
    const path = try jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id);
    defer service.allocator.free(path);
    const minecraft = jmcl.installer.openRoot(service.io, path) catch |err| {
        try stderr.print("failed to open instance directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer minecraft.close(service.io);
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
        .modrinth => service.installModrinthContent(
            minecraft,
            store,
            kind,
            document.value.version_id,
            loader,
            project,
            options.version_id,
            .{ .workers = options.workers, .retries = options.retries, .with_dependencies = options.with_dependencies },
            .{ .context = &reporter_context, .callback = cli_install.CliInstallReporter.callback },
        ),
        .curseforge => service.installCurseforgeContent(
            minecraft,
            store,
            kind,
            document.value.version_id,
            loader,
            options.api_key.?,
            project,
            options.version_id,
            .{ .workers = options.workers, .retries = options.retries, .with_dependencies = options.with_dependencies },
            .{ .context = &reporter_context, .callback = cli_install.CliInstallReporter.callback },
        ),
        .manual => unreachable,
    } catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        switch (err) {
            error.NoCompatibleContentVersion => switch (kind) {
                .mod => try stderr.print("no {s} release of {s} supports Minecraft {s}\n", .{ @tagName(loader.?), project, document.value.version_id }),
                .resourcepack, .shaderpack => try stderr.print("no release of {s} supports Minecraft {s}\n", .{ project, document.value.version_id }),
            },
            error.IncompatibleContentVersion => try stderr.print("the requested version does not support Minecraft {s}\n", .{document.value.version_id}),
            error.ContentFileConflict => try stderr.writeAll("a different registered content entry already occupies the target file\n"),
            error.ContentKindMismatch => try stderr.print("{s} is not a {s} project\n", .{ project, @tagName(kind) }),
            error.UnsupportedProjectType => try stderr.writeAll("the project type is not supported\n"),
            error.CurseforgeDownloadUnavailable => try stderr.writeAll("the project file is not available for third-party downloads\n"),
            error.InvalidCurseforgeId => try stderr.writeAll("curseforge projects and files use numeric ids\n"),
            error.UnsuccessfulHttpStatus => try stderr.writeAll("the content API returned an unsuccessful HTTP status (check the api key)\n"),
            else => try stderr.print("content installation failed: {s}\n", .{@errorName(err)}),
        }
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

const ContentSetVersionOptions = struct {
    id: []const u8,
    project: ?[]const u8 = null,
    version_id: ?[]const u8 = null,
    api_key: ?[]const u8 = null,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    store_directory: []const u8 = jmcl.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

fn runContentSetVersion(
    kind: jmcl.content_registry.Kind,
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.print("usage: jmcl-core instance {s} set-version ID --project REF --version ID [--api-key KEY] [--instances-directory PATH] [--store-directory PATH] [--workers N] [--retries N]\n", .{kind.directory()});
        return 2;
    }
    var options: ContentSetVersionOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--project")) {
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
    if (options.project == null or options.version_id == null) {
        try stderr.print("instance {s} set-version requires --project and --version\n", .{kind.directory()});
        return 2;
    }

    const root = jmcl.instance_store.openRoot(service.io, options.instances_directory) catch |err| {
        try stderr.print("failed to open instances directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    var document = jmcl.instance_store.load(service.allocator, service.io, root, options.id) catch |err| {
        try stderr.print("failed to load instance: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer document.deinit();
    const loader: ?jmcl.modrinth.Loader = switch (kind) {
        .mod => manifestModLoader(document.value) catch {
            try stderr.writeAll("instance has no mod loader; mods require Fabric, NeoForge, or Forge\n");
            return 1;
        },
        .resourcepack, .shaderpack => null,
    };
    const path = try jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id);
    defer service.allocator.free(path);
    const minecraft = jmcl.installer.openRoot(service.io, path) catch |err| {
        try stderr.print("failed to open instance directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer minecraft.close(service.io);
    const store = jmcl.artifact_store.openRoot(service.io, options.store_directory) catch |err| {
        try stderr.print("failed to open artifact store: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer store.close(service.io);

    var reporter_context: cli_install.CliInstallReporter = .{
        .io = service.io,
        .writer = stderr,
    };
    var change = service.setInstanceContentVersion(
        minecraft,
        store,
        kind,
        document.value.version_id,
        loader,
        options.project.?,
        options.version_id.?,
        options.api_key,
        .{ .workers = options.workers, .retries = options.retries },
        .{ .context = &reporter_context, .callback = cli_install.CliInstallReporter.callback },
    ) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        switch (err) {
            error.ContentNotFound => try stderr.print("no registered {s} matches the given project\n", .{@tagName(kind)}),
            error.IncompatibleContentVersion => try stderr.print("the requested version does not support Minecraft {s}\n", .{document.value.version_id}),
            error.ModrinthVersionProjectMismatch, error.CurseforgeFileProjectMismatch => try stderr.writeAll("the requested version does not belong to the registered project\n"),
            error.ContentFileConflict => try stderr.writeAll("a different registered content entry already occupies the target file\n"),
            error.CurseforgeApiKeyRequired => try stderr.writeAll("switching a curseforge entry requires --api-key (supplied by the embedding application)\n"),
            error.CurseforgeDownloadUnavailable => try stderr.writeAll("the project file is not available for third-party downloads\n"),
            else => try stderr.print("content version change failed: {s}\n", .{@errorName(err)}),
        }
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

const ContentRemoveOptions = struct {
    id: []const u8,
    project: ?[]const u8 = null,
    file: ?[]const u8 = null,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
};

fn runContentRemove(
    kind: jmcl.content_registry.Kind,
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.print("usage: jmcl-core instance {s} remove ID (--project REF | --file PATH) [--instances-directory PATH]\n", .{kind.directory()});
        return 2;
    }
    var options: ContentRemoveOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--project")) {
            options.project = value;
        } else if (std.mem.eql(u8, argument, "--file")) {
            options.file = value;
        } else if (std.mem.eql(u8, argument, "--instances-directory")) {
            options.instances_directory = value;
        } else return error.InvalidArguments;
        index += 2;
    }
    if ((options.project == null) == (options.file == null)) {
        try stderr.print("instance {s} remove requires exactly one of --project or --file\n", .{kind.directory()});
        return 2;
    }

    const root = jmcl.instance_store.openRoot(service.io, options.instances_directory) catch |err| {
        try stderr.print("failed to open instances directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    var document = jmcl.instance_store.load(service.allocator, service.io, root, options.id) catch |err| {
        try stderr.print("failed to load instance: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer document.deinit();
    const path = try jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id);
    defer service.allocator.free(path);
    const minecraft = jmcl.installer.openRoot(service.io, path) catch |err| {
        try stderr.print("failed to open instance directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer minecraft.close(service.io);

    var removal = service.removeInstanceContent(minecraft, kind, options.project, options.file) catch |err| {
        switch (err) {
            error.ContentNotFound => try stderr.print("no registered {s} matches the given selector\n", .{@tagName(kind)}),
            else => try stderr.print("failed to remove content: {s}\n", .{@errorName(err)}),
        }
        return 1;
    };
    defer removal.deinit();
    try stdout.print("removed\t{s}\t{s}\n", .{ removal.entry.file, removal.entry.sha1 });
    return 0;
}

const ContentAdoptOptions = struct {
    id: []const u8,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    store_directory: []const u8 = jmcl.artifact_store.default_directory,
};

fn runContentAdopt(
    kind: jmcl.content_registry.Kind,
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.print("usage: jmcl-core instance {s} adopt ID [--instances-directory PATH] [--store-directory PATH]\n", .{kind.directory()});
        return 2;
    }
    var options: ContentAdoptOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--instances-directory")) {
            options.instances_directory = value;
        } else if (std.mem.eql(u8, argument, "--store-directory")) {
            options.store_directory = value;
        } else return error.InvalidArguments;
        index += 2;
    }

    const root = jmcl.instance_store.openRoot(service.io, options.instances_directory) catch |err| {
        try stderr.print("failed to open instances directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    var document = jmcl.instance_store.load(service.allocator, service.io, root, options.id) catch |err| {
        try stderr.print("failed to load instance: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer document.deinit();
    const path = try jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id);
    defer service.allocator.free(path);
    const minecraft = jmcl.installer.openRoot(service.io, path) catch |err| {
        try stderr.print("failed to open instance directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer minecraft.close(service.io);
    const store = jmcl.artifact_store.openRoot(service.io, options.store_directory) catch |err| {
        try stderr.print("failed to open artifact store: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer store.close(service.io);

    var adoption = service.adoptInstanceContent(minecraft, store, kind) catch |err| {
        try stderr.print("failed to adopt unmanaged content: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer adoption.deinit();
    for (adoption.adopted) |adopted| {
        try stdout.print("adopted\t{s}\t{s}\t{d}\n", .{ adopted.file, adopted.sha1, adopted.size });
    }
    try stdout.print("adoptedCount\t{d}\n", .{adoption.adopted.len});
    return 0;
}
