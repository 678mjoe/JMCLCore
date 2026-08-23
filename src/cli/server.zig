const std = @import("std");
const builtin = @import("builtin");
const jmcl = @import("jmcl_core");
const common = @import("common.zig");
const install_cli = @import("install.zig");
const cli_worlds = @import("worlds.zig");
const cli_server_content = @import("server_content.zig");
const cli_server_datapacks = @import("server_datapacks.zig");

const unsupported_platform_message = "Server management is currently supported only on Linux";

pub fn run(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "create")) return runServerCreate(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "list")) return runServerList(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "show")) return runServerShow(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "delete")) return runServerDelete(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "install")) return runServerInstall(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "properties")) return runServerProperties(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "worlds")) return cli_worlds.runServerWorlds(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "mods")) return cli_server_content.runServerMods(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "datapacks")) return cli_server_datapacks.runServerDatapacks(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "start")) return runServerStart(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "stop")) return runServerStop(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "restart")) return runServerRestart(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "status")) return runServerStatus(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "logs")) return runServerLogs(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "command")) return runServerCommand(service, args[1..], stdout, stderr);
    try stderr.writeAll("usage: jmcl-core server <create|list|show|delete|install|properties|worlds|mods|datapacks|start|stop|restart|status|logs|command> [options]\n");
    return 2;
}

fn requireLinux(stderr: *std.Io.Writer) std.Io.Writer.Error!bool {
    if (comptime builtin.os.tag == .linux) return true;
    try stderr.writeAll(unsupported_platform_message ++ "\n");
    return false;
}

fn installationStatus(
    service: *jmcl.Service,
    root: std.Io.Dir,
    manifest: jmcl.server_store.Manifest,
) !bool {
    const server_dir = jmcl.server_store.openServer(service.io, root, manifest.id) catch |err| switch (err) {
        error.FileNotFound => return false,
        else => |other| return other,
    };
    defer server_dir.close(service.io);
    return jmcl.server_store.isInstalled(service.allocator, service.io, server_dir, manifest);
}

fn writeServerJson(
    json: *std.json.Stringify,
    manifest: jmcl.server_store.Manifest,
    installed: bool,
) std.Io.Writer.Error!void {
    try json.beginObject();
    try json.objectField("id");
    try json.write(manifest.id);
    try json.objectField("name");
    try json.write(manifest.name);
    try json.objectField("version_id");
    try json.write(manifest.version_id);
    try json.objectField("fabric_loader");
    try json.write(manifest.fabric_loader);
    try json.objectField("neoforge_version");
    try json.write(manifest.neoforge_version);
    try json.objectField("forge_version");
    try json.write(manifest.forge_version);
    try json.objectField("source");
    try json.write(@tagName(manifest.source));
    try json.objectField("accept_eula");
    try json.write(manifest.accept_eula);
    try json.objectField("java_path");
    try json.write(manifest.java_path);
    try json.objectField("installed");
    try json.write(installed);
    try json.endObject();
}

const ServerCreateOptions = struct {
    id: []const u8,
    version_id: ?[]const u8 = null,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    name: ?[]const u8 = null,
    servers_directory: []const u8 = jmcl.server_store.default_root,
    source: jmcl.DownloadSource = .official,
    java_path: ?[]const u8 = null,
    json: bool = false,
};

fn parseServerCreateOptions(args: []const []const u8) !ServerCreateOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;
    try jmcl.server_store.validateId(args[0]);
    var options: ServerCreateOptions = .{ .id = args[0] };
    var index: usize = 1;
    while (index < args.len) {
        const argument = args[index];
        if (std.mem.eql(u8, argument, "--json")) {
            options.json = true;
            index += 1;
            continue;
        }
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--version")) {
            options.version_id = value;
        } else if (std.mem.eql(u8, argument, "--fabric-loader")) {
            options.fabric_loader = value;
        } else if (std.mem.eql(u8, argument, "--neoforge-version")) {
            options.neoforge_version = value;
        } else if (std.mem.eql(u8, argument, "--forge-version")) {
            options.forge_version = value;
        } else if (std.mem.eql(u8, argument, "--name")) {
            options.name = value;
        } else if (std.mem.eql(u8, argument, "--servers-directory")) {
            options.servers_directory = value;
        } else if (std.mem.eql(u8, argument, "--java-path")) {
            options.java_path = value;
        } else if (std.mem.eql(u8, argument, "--source")) {
            options.source = jmcl.DownloadSource.parse(value) orelse return error.InvalidArguments;
        } else return error.InvalidArguments;
        index += 2;
    }
    if (options.version_id == null) return error.InvalidArguments;
    const loader_count: u8 = @as(u8, @intFromBool(options.fabric_loader != null)) +
        @intFromBool(options.neoforge_version != null) +
        @intFromBool(options.forge_version != null);
    if (loader_count > 1) return error.InvalidArguments;
    return options;
}

fn runServerCreate(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    const options = parseServerCreateOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core server create ID --version VERSION [--fabric-loader VERSION | --neoforge-version VERSION | --forge-version VERSION] [--name NAME] [--source SOURCE] [--java-path PATH] [--servers-directory PATH] [--json]\n");
        return 2;
    };
    var metadata = service.resolveMetadataPreferred(.{
        .id = options.version_id.?,
        .fabric_loader = options.fabric_loader,
        .neoforge_version = options.neoforge_version,
        .forge_version = options.forge_version,
    }, options.source) catch |err| {
        try stderr.print("failed to resolve Minecraft version: {s}\n", .{@errorName(err)});
        return 1;
    };
    metadata.deinit();
    const root = jmcl.server_store.openRoot(service.io, options.servers_directory) catch |err| {
        try stderr.print("failed to open servers directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    const manifest: jmcl.server_store.Manifest = .{
        .id = options.id,
        .name = options.name orelse options.id,
        .version_id = options.version_id.?,
        .fabric_loader = options.fabric_loader,
        .neoforge_version = options.neoforge_version,
        .forge_version = options.forge_version,
        .source = options.source,
        .java_path = options.java_path,
    };
    jmcl.server_store.create(service.io, root, manifest) catch |err| {
        try stderr.print("failed to create server: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("server");
        try writeServerJson(&json, manifest, false);
        try json.endObject();
        try stdout.writeByte('\n');
    } else {
        try stdout.print("server\t{s}\nname\t{s}\nversion\t{s}\nsource\t{s}\ninstalled\tfalse\n", .{
            manifest.id,
            manifest.name,
            manifest.version_id,
            @tagName(manifest.source),
        });
    }
    return 0;
}

const ServerListOptions = struct {
    servers_directory: []const u8 = jmcl.server_store.default_root,
    json: bool = false,
};

fn parseServerListOptions(args: []const []const u8) !ServerListOptions {
    var options: ServerListOptions = .{};
    var index: usize = 0;
    while (index < args.len) {
        if (std.mem.eql(u8, args[index], "--json")) {
            options.json = true;
            index += 1;
        } else if (std.mem.eql(u8, args[index], "--servers-directory")) {
            if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
            options.servers_directory = args[index + 1];
            index += 2;
        } else return error.InvalidArguments;
    }
    return options;
}

fn runServerList(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    const options = parseServerListOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core server list [--servers-directory PATH] [--json]\n");
        return 2;
    };
    const root = jmcl.server_store.openRoot(service.io, options.servers_directory) catch |err| {
        try stderr.print("failed to open servers directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    var servers = jmcl.server_store.list(service.allocator, service.io, root) catch |err| {
        try stderr.print("failed to list servers: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer servers.deinit();
    const installed = service.allocator.alloc(bool, servers.items.len) catch {
        try stderr.writeAll("failed to read server installation status: OutOfMemory\n");
        return 1;
    };
    defer service.allocator.free(installed);
    for (servers.items, installed) |document, *status| {
        status.* = installationStatus(service, root, document.value) catch |err| {
            try stderr.print("failed to read server installation status: {s}\n", .{@errorName(err)});
            return 1;
        };
    }
    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("servers");
        try json.beginArray();
        for (servers.items, installed) |document, status| try writeServerJson(&json, document.value, status);
        try json.endArray();
        try json.endObject();
        try stdout.writeByte('\n');
    } else {
        for (servers.items, installed) |document, status| {
            try stdout.print("{s}\t{s}\t{s}\t{s}\t{s}\n", .{
                document.value.id,
                document.value.version_id,
                @tagName(document.value.source),
                if (status) "installed" else "not-installed",
                document.value.name,
            });
        }
    }
    return 0;
}

const ServerTargetOptions = struct {
    id: []const u8,
    servers_directory: []const u8 = jmcl.server_store.default_root,
    json: bool = false,
};

fn parseServerTargetOptions(args: []const []const u8, allow_json: bool) !ServerTargetOptions {
    if (args.len == 0) return error.InvalidArguments;
    try jmcl.server_store.validateId(args[0]);
    var options: ServerTargetOptions = .{ .id = args[0] };
    var index: usize = 1;
    while (index < args.len) {
        if (allow_json and std.mem.eql(u8, args[index], "--json")) {
            options.json = true;
            index += 1;
        } else if (std.mem.eql(u8, args[index], "--servers-directory")) {
            if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
            options.servers_directory = args[index + 1];
            index += 2;
        } else return error.InvalidArguments;
    }
    return options;
}

fn runServerShow(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    const options = parseServerTargetOptions(args, true) catch {
        try stderr.writeAll("usage: jmcl-core server show ID [--servers-directory PATH] [--json]\n");
        return 2;
    };
    const root = jmcl.server_store.openRoot(service.io, options.servers_directory) catch |err| {
        try stderr.print("failed to open servers directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    var document = jmcl.server_store.load(service.allocator, service.io, root, options.id) catch |err| {
        try stderr.print("failed to load server: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer document.deinit();
    const installed = installationStatus(service, root, document.value) catch |err| {
        try stderr.print("failed to read server installation status: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("server");
        try writeServerJson(&json, document.value, installed);
        try json.endObject();
        try stdout.writeByte('\n');
    } else {
        try stdout.print("server\t{s}\nname\t{s}\nversion\t{s}\nsource\t{s}\ninstalled\t{s}\n", .{
            document.value.id,
            document.value.name,
            document.value.version_id,
            @tagName(document.value.source),
            if (installed) "true" else "false",
        });
        if (document.value.fabric_loader) |loader| try stdout.print("fabric-loader\t{s}\n", .{loader});
        if (document.value.neoforge_version) |version| try stdout.print("neoforge-version\t{s}\n", .{version});
        if (document.value.forge_version) |version| try stdout.print("forge-version\t{s}\n", .{version});
    }
    return 0;
}

fn runServerDelete(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    const options = parseServerTargetOptions(args, false) catch {
        try stderr.writeAll("usage: jmcl-core server delete ID [--servers-directory PATH]\n");
        return 2;
    };
    const root = jmcl.server_store.openRoot(service.io, options.servers_directory) catch |err| {
        try stderr.print("failed to open servers directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    const server_dir = jmcl.server_store.openServer(service.io, root, options.id) catch |err| {
        try stderr.print("failed to open server directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer server_dir.close(service.io);
    var stopped_guard = jmcl.server_lifecycle.lockStopped(service.allocator, service.io, server_dir) catch |err| switch (err) {
        error.ServerAlreadyRunning => {
            try stderr.writeAll("stop the server before deleting it\n");
            return 1;
        },
        else => {
            try stderr.print("failed to validate the server lifecycle status: {s}\n", .{@errorName(err)});
            return 1;
        },
    };
    defer stopped_guard.deinit();
    jmcl.server_store.delete(service.allocator, service.io, root, options.id) catch |err| {
        try stderr.print("failed to delete server: {s}\n", .{@errorName(err)});
        return 1;
    };
    try stdout.print("deleted\t{s}\n", .{options.id});
    return 0;
}

const ServerInstallOptions = struct {
    id: []const u8,
    accept_eula: bool = false,
    servers_directory: []const u8 = jmcl.server_store.default_root,
    store_directory: []const u8 = jmcl.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
    java_paths: std.ArrayList([]const u8) = .empty,
    json: bool = false,

    fn deinit(options: *ServerInstallOptions, allocator: std.mem.Allocator) void {
        options.java_paths.deinit(allocator);
    }
};

fn parseServerInstallOptions(allocator: std.mem.Allocator, args: []const []const u8) !ServerInstallOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;
    try jmcl.server_store.validateId(args[0]);
    var options: ServerInstallOptions = .{ .id = args[0] };
    errdefer options.deinit(allocator);
    var index: usize = 1;
    while (index < args.len) {
        const argument = args[index];
        if (std.mem.eql(u8, argument, "--accept-eula")) {
            options.accept_eula = true;
            index += 1;
            continue;
        }
        if (std.mem.eql(u8, argument, "--json")) {
            options.json = true;
            index += 1;
            continue;
        }
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--servers-directory")) {
            options.servers_directory = value;
        } else if (std.mem.eql(u8, argument, "--store-directory")) {
            options.store_directory = value;
        } else if (std.mem.eql(u8, argument, "--workers")) {
            options.workers = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
            if (options.workers == 0 or options.workers > 32) return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--retries")) {
            options.retries = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
            if (options.retries > 5) return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--java-path")) {
            try options.java_paths.append(allocator, value);
        } else return error.InvalidArguments;
        index += 2;
    }
    return options;
}

fn runServerInstall(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    var options = parseServerInstallOptions(service.allocator, args) catch {
        try stderr.writeAll("usage: jmcl-core server install ID --accept-eula [--servers-directory PATH] [--store-directory PATH] [--workers N] [--retries N] [--java-path PATH ...] [--json]\n");
        return 2;
    };
    defer options.deinit(service.allocator);
    if (!options.accept_eula) {
        try stderr.writeAll("Server installation requires explicit Minecraft EULA acceptance\n");
        return 1;
    }
    const root = jmcl.server_store.openRoot(service.io, options.servers_directory) catch |err| {
        try stderr.print("failed to open servers directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    var manifest_document = jmcl.server_store.load(service.allocator, service.io, root, options.id) catch |err| switch (err) {
        error.FileNotFound, error.InvalidServerId => {
            try stderr.writeAll("no server matches the given id\n");
            return 1;
        },
        else => {
            try stderr.print("failed to load server: {s}\n", .{@errorName(err)});
            return 1;
        },
    };
    defer manifest_document.deinit();
    const manifest = &manifest_document.value;
    const server_dir = jmcl.server_store.openServer(service.io, root, manifest.id) catch |err| {
        try stderr.print("failed to open server directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer server_dir.close(service.io);
    var stopped_guard = jmcl.server_lifecycle.lockStopped(service.allocator, service.io, server_dir) catch |err| switch (err) {
        error.ServerAlreadyRunning => {
            try stderr.writeAll("stop the server before installing it\n");
            return 1;
        },
        else => {
            try stderr.print("failed to validate the server lifecycle status: {s}\n", .{@errorName(err)});
            return 1;
        },
    };
    defer stopped_guard.deinit();
    if (!manifest.accept_eula) {
        manifest.accept_eula = true;
        jmcl.server_store.update(service.io, root, manifest.*) catch |err| {
            try stderr.print("failed to record Minecraft EULA acceptance: {s}\n", .{@errorName(err)});
            return 1;
        };
    }
    const target = jmcl.install_plan.Target.host() catch |err| {
        try stderr.print("unsupported host platform: {s}\n", .{@errorName(err)});
        return 1;
    };
    var plan = service.planServerInstall(manifest.*, target) catch |err| switch (err) {
        error.ServerDownloadMissing => {
            try stderr.writeAll("the selected Minecraft version has no dedicated server download\n");
            return 1;
        },
        error.UnsupportedServerLoader => {
            try stderr.writeAll("This server loader is not supported yet\n");
            return 1;
        },
        else => {
            try stderr.print("failed to create the server installation plan: {s}\n", .{@errorName(err)});
            return 1;
        },
    };
    defer plan.deinit();
    const runtime = service.selectServerJava(manifest.*, plan, options.java_paths.items) catch |err| switch (err) {
        error.EnvironmentUnavailable => {
            try stderr.writeAll("process environment is unavailable\n");
            return 1;
        },
        error.NoCompatibleJava => {
            try stderr.writeAll("no Java runtime matches the server requirement\n");
            return 1;
        },
        else => {
            try stderr.print("no Java executable candidates were found: {s}\n", .{@errorName(err)});
            return 1;
        },
    };
    const store = jmcl.artifact_store.openRoot(service.io, options.store_directory) catch |err| {
        try stderr.print("failed to open artifact store: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer store.close(service.io);
    var reporter_context: install_cli.CliInstallReporter = .{
        .io = service.io,
        .writer = stderr,
    };
    const summary = service.executeServerInstall(
        manifest.*,
        server_dir,
        plan,
        store,
        .{ .workers = options.workers, .retries = options.retries },
        runtime,
        .{ .context = &reporter_context, .callback = install_cli.CliInstallReporter.callback },
    ) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        switch (err) {
            error.UnsupportedPlatform => try stderr.writeAll(unsupported_platform_message ++ "\n"),
            error.ServerEulaFailed => try stderr.writeAll("failed to write eula.txt\n"),
            error.ServerPropertiesFailed => try stderr.writeAll("failed to initialize server.properties\n"),
            error.ServerStateError => try stderr.writeAll("failed to record completed server installation\n"),
            else => try stderr.print("failed to download and materialize the server jar: {s}\n", .{@errorName(err)}),
        }
        return 1;
    };
    if (reporter_context.output_failed) return error.WriteFailed;
    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("stage");
        try json.write("server.install");
        try json.objectField("id");
        try json.write(manifest.id);
        try json.objectField("version_id");
        try json.write(manifest.version_id);
        try json.objectField("source");
        try json.write(@tagName(manifest.source));
        try json.objectField("java");
        try json.write(summary.java);
        try json.objectField("properties_created");
        try json.write(summary.properties_created);
        try json.objectField("files_total");
        try json.write(summary.files_total);
        try json.objectField("files_downloaded");
        try json.write(summary.files_downloaded);
        try json.objectField("files_cached");
        try json.write(summary.files_cached);
        try json.objectField("files_hard_linked");
        try json.write(summary.files_hard_linked);
        try json.objectField("files_copied");
        try json.write(summary.files_copied);
        try json.objectField("bytes_verified");
        try json.write(summary.bytes_verified);
        try json.objectField("bytes_transferred");
        try json.write(summary.bytes_transferred);
        try json.endObject();
        try stdout.writeByte('\n');
    } else {
        try common.writeSourceNotice(stdout, manifest.source);
        try stdout.print("server\t{s}\nversion\t{s}\nsource\t{s}\njava\t{s}\npropertiesCreated\t{s}\n", .{
            manifest.id,
            manifest.version_id,
            @tagName(manifest.source),
            summary.java.executable,
            if (summary.properties_created) "true" else "false",
        });
        try stdout.print("files\t{d}\ndownloaded\t{d}\ncached\t{d}\nhardLinked\t{d}\ncopied\t{d}\nverifiedBytes\t{d}\ntransferredBytes\t{d}\n", .{
            summary.files_total,
            summary.files_downloaded,
            summary.files_cached,
            summary.files_hard_linked,
            summary.files_copied,
            summary.bytes_verified,
            summary.bytes_transferred,
        });
    }
    return 0;
}

fn runServerProperties(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "get")) return runServerPropertiesGet(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "set")) return runServerPropertiesSet(service, args[1..], stdout, stderr);
    try stderr.writeAll("usage: jmcl-core server properties <get|set> ID [options]\n");
    return 2;
}

fn loadServerPropertiesDocument(
    service: *jmcl.Service,
    servers_directory: []const u8,
    id: []const u8,
    stderr: *std.Io.Writer,
) !?jmcl.server_properties.Document {
    const root = jmcl.server_store.openRoot(service.io, servers_directory) catch |err| {
        try stderr.print("failed to open servers directory: {s}\n", .{@errorName(err)});
        return null;
    };
    defer root.close(service.io);
    var manifest = jmcl.server_store.load(service.allocator, service.io, root, id) catch |err| switch (err) {
        error.FileNotFound, error.InvalidServerId => {
            try stderr.writeAll("no server matches the given id\n");
            return null;
        },
        else => {
            try stderr.print("failed to load server: {s}\n", .{@errorName(err)});
            return null;
        },
    };
    defer manifest.deinit();
    const server_dir = jmcl.server_store.openServer(service.io, root, id) catch |err| {
        try stderr.print("failed to open server directory: {s}\n", .{@errorName(err)});
        return null;
    };
    defer server_dir.close(service.io);
    return jmcl.server_properties.load(service.allocator, service.io, server_dir) catch |err| switch (err) {
        error.FileNotFound => {
            try stderr.writeAll("server.properties has not been created\n");
            return null;
        },
        error.InvalidServerProperties,
        error.InvalidServerPropertyKey,
        error.InvalidServerPropertyValue,
        error.TooManyServerProperties,
        error.ServerPropertiesTooLarge,
        => {
            try stderr.writeAll("server.properties is malformed or out of range\n");
            return null;
        },
        else => {
            try stderr.print("failed to read server.properties: {s}\n", .{@errorName(err)});
            return null;
        },
    };
}

fn runServerPropertiesGet(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    const options = parseServerTargetOptions(args, true) catch {
        try stderr.writeAll("usage: jmcl-core server properties get ID [--servers-directory PATH] [--json]\n");
        return 2;
    };
    var properties = (try loadServerPropertiesDocument(service, options.servers_directory, options.id, stderr)) orelse return 1;
    defer properties.deinit();
    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("id");
        try json.write(options.id);
        try json.objectField("properties");
        try json.beginObject();
        for (properties.entries.items) |entry| {
            try json.objectField(entry.key);
            try json.write(entry.value);
        }
        try json.endObject();
        try json.endObject();
        try stdout.writeByte('\n');
    } else {
        for (properties.entries.items) |entry| {
            try stdout.print("{s}={s}\n", .{ entry.key, entry.value });
        }
    }
    return 0;
}

const ServerPropertiesSetOptions = struct {
    id: []const u8,
    servers_directory: []const u8 = jmcl.server_store.default_root,
    pairs: std.ArrayList(jmcl.server_properties.Entry) = .empty,

    fn deinit(options: *ServerPropertiesSetOptions, allocator: std.mem.Allocator) void {
        options.pairs.deinit(allocator);
    }
};

fn parseServerPropertiesSetOptions(allocator: std.mem.Allocator, args: []const []const u8) !ServerPropertiesSetOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;
    try jmcl.server_store.validateId(args[0]);
    var options: ServerPropertiesSetOptions = .{ .id = args[0] };
    errdefer options.deinit(allocator);
    var index: usize = 1;
    while (index < args.len) {
        const argument = args[index];
        if (std.mem.eql(u8, argument, "--servers-directory")) {
            if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
            options.servers_directory = args[index + 1];
            index += 2;
            continue;
        }
        if (std.mem.startsWith(u8, argument, "--")) return error.InvalidArguments;
        const separator = std.mem.indexOfScalar(u8, argument, '=') orelse return error.InvalidArguments;
        if (separator == 0) return error.InvalidArguments;
        if (options.pairs.items.len == jmcl.server_properties.max_entries) return error.InvalidArguments;
        try options.pairs.append(allocator, .{
            .key = argument[0..separator],
            .value = argument[separator + 1 ..],
        });
        index += 1;
    }
    if (options.pairs.items.len == 0) return error.InvalidArguments;
    return options;
}

fn runServerPropertiesSet(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    var options = parseServerPropertiesSetOptions(service.allocator, args) catch {
        try stderr.writeAll("usage: jmcl-core server properties set ID key=value ... [--servers-directory PATH]\n");
        return 2;
    };
    defer options.deinit(service.allocator);
    const root = jmcl.server_store.openRoot(service.io, options.servers_directory) catch |err| {
        try stderr.print("failed to open servers directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    var manifest = jmcl.server_store.load(service.allocator, service.io, root, options.id) catch |err| switch (err) {
        error.FileNotFound, error.InvalidServerId => {
            try stderr.writeAll("no server matches the given id\n");
            return 1;
        },
        else => {
            try stderr.print("failed to load server: {s}\n", .{@errorName(err)});
            return 1;
        },
    };
    defer manifest.deinit();
    const server_dir = jmcl.server_store.openServer(service.io, root, options.id) catch |err| {
        try stderr.print("failed to open server directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer server_dir.close(service.io);
    var properties = jmcl.server_properties.load(service.allocator, service.io, server_dir) catch |err| switch (err) {
        error.FileNotFound => {
            try stderr.writeAll("server.properties has not been created\n");
            return 1;
        },
        error.InvalidServerProperties,
        error.InvalidServerPropertyKey,
        error.InvalidServerPropertyValue,
        error.TooManyServerProperties,
        error.ServerPropertiesTooLarge,
        => {
            try stderr.writeAll("server.properties is malformed or out of range\n");
            return 1;
        },
        else => {
            try stderr.print("failed to read server.properties: {s}\n", .{@errorName(err)});
            return 1;
        },
    };
    defer properties.deinit();
    for (options.pairs.items) |pair| {
        properties.set(pair.key, pair.value) catch {
            try stderr.writeAll("a server property key or value is out of range\n");
            return 1;
        };
    }
    jmcl.server_properties.writeAtomic(service.io, server_dir, properties) catch |err| {
        try stderr.print("failed to atomically update server.properties: {s}\n", .{@errorName(err)});
        return 1;
    };
    for (properties.entries.items) |entry| {
        try stdout.print("{s}={s}\n", .{ entry.key, entry.value });
    }
    return 0;
}

const OpenedServer = struct {
    root: std.Io.Dir,
    manifest: jmcl.server_store.Document,
    server_dir: std.Io.Dir,

    fn deinit(opened: *OpenedServer, service: *jmcl.Service) void {
        opened.server_dir.close(service.io);
        opened.manifest.deinit();
        opened.root.close(service.io);
        opened.* = undefined;
    }
};

fn openServerTarget(
    service: *jmcl.Service,
    servers_directory: []const u8,
    id: []const u8,
    stderr: *std.Io.Writer,
) !?OpenedServer {
    const root = jmcl.server_store.openRoot(service.io, servers_directory) catch |err| {
        try stderr.print("failed to open servers directory: {s}\n", .{@errorName(err)});
        return null;
    };
    var manifest = jmcl.server_store.load(service.allocator, service.io, root, id) catch |err| switch (err) {
        error.FileNotFound, error.InvalidServerId => {
            root.close(service.io);
            try stderr.writeAll("no server matches the given id\n");
            return null;
        },
        else => {
            root.close(service.io);
            try stderr.print("failed to load server: {s}\n", .{@errorName(err)});
            return null;
        },
    };
    const server_dir = jmcl.server_store.openServer(service.io, root, id) catch |err| {
        manifest.deinit();
        root.close(service.io);
        try stderr.print("failed to open server directory: {s}\n", .{@errorName(err)});
        return null;
    };
    return .{ .root = root, .manifest = manifest, .server_dir = server_dir };
}

fn writeServerStartError(stderr: *std.Io.Writer, err: jmcl.Service.ServerStartError) std.Io.Writer.Error!void {
    switch (err) {
        error.UnsupportedPlatform => try stderr.writeAll(unsupported_platform_message ++ "\n"),
        error.EnvironmentUnavailable => try stderr.writeAll("process environment is unavailable\n"),
        error.ServerStatusFailed => try stderr.writeAll("failed to validate the server lifecycle status\n"),
        error.ServerAlreadyRunning => try stderr.writeAll("the server is already running\n"),
        error.ServerNotInstalled => try stderr.writeAll("the server installation is incomplete or does not match its manifest\n"),
        error.ServerInstallationUnreadable => try stderr.writeAll("failed to read the completed server installation\n"),
        error.JavaNotFound => try stderr.writeAll("no Java executable candidates were found\n"),
        error.JavaResolutionFailed => try stderr.writeAll("the selected Java executable could not be resolved\n"),
        error.NoCompatibleJava => try stderr.writeAll("no Java runtime matches the installed server requirement\n"),
        error.InvalidGracePeriod, error.InvalidJavaPath => try stderr.writeAll("server lifecycle parameters are invalid\n"),
        error.ServerStartFailed => try stderr.writeAll("failed to start the server\n"),
        error.ServerRestartFailed => try stderr.writeAll("failed to restart the server\n"),
        error.OutOfMemory => try stderr.writeAll("out of memory\n"),
    }
}

const ServerStartOptions = struct {
    id: []const u8,
    servers_directory: []const u8 = jmcl.server_store.default_root,
    java_paths: std.ArrayList([]const u8) = .empty,

    fn deinit(options: *ServerStartOptions, allocator: std.mem.Allocator) void {
        options.java_paths.deinit(allocator);
    }
};

fn parseServerStartOptions(allocator: std.mem.Allocator, args: []const []const u8) !ServerStartOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;
    try jmcl.server_store.validateId(args[0]);
    var options: ServerStartOptions = .{ .id = args[0] };
    errdefer options.deinit(allocator);
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const value = args[index + 1];
        if (std.mem.eql(u8, args[index], "--servers-directory")) {
            options.servers_directory = value;
        } else if (std.mem.eql(u8, args[index], "--java-path")) {
            try options.java_paths.append(allocator, value);
        } else return error.InvalidArguments;
        index += 2;
    }
    return options;
}

fn runServerStart(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    var options = parseServerStartOptions(service.allocator, args) catch {
        try stderr.writeAll("usage: jmcl-core server start ID [--servers-directory PATH] [--java-path PATH ...]\n");
        return 2;
    };
    defer options.deinit(service.allocator);
    var opened = (try openServerTarget(service, options.servers_directory, options.id, stderr)) orelse return 1;
    defer opened.deinit(service);
    var summary = service.startServer(
        opened.manifest.value,
        opened.server_dir,
        options.java_paths.items,
    ) catch |err| {
        try writeServerStartError(stderr, err);
        return 1;
    };
    defer summary.deinit();
    try stdout.print("server\t{s}\nrunning\ttrue\npid\t{d}\nsupervisorPid\t{d}\nstartedAtMs\t{d}\njava\t{s}\n", .{
        options.id,
        summary.pid,
        summary.supervisor_pid,
        summary.started_at_ms,
        summary.java_path,
    });
    return 0;
}

const ServerStopOptions = struct {
    id: []const u8,
    grace_ms: u32 = jmcl.server_lifecycle.default_grace_ms,
    servers_directory: []const u8 = jmcl.server_store.default_root,
};

fn parseServerStopOptions(args: []const []const u8) !ServerStopOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;
    try jmcl.server_store.validateId(args[0]);
    var options: ServerStopOptions = .{ .id = args[0] };
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const value = args[index + 1];
        if (std.mem.eql(u8, args[index], "--servers-directory")) {
            options.servers_directory = value;
        } else if (std.mem.eql(u8, args[index], "--grace-ms")) {
            options.grace_ms = std.fmt.parseInt(u32, value, 10) catch return error.InvalidArguments;
            if (options.grace_ms > jmcl.server_lifecycle.max_grace_ms) return error.InvalidArguments;
        } else return error.InvalidArguments;
        index += 2;
    }
    return options;
}

fn runServerStop(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    const options = parseServerStopOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core server stop ID [--grace-ms N] [--servers-directory PATH]\n");
        return 2;
    };
    var opened = (try openServerTarget(service, options.servers_directory, options.id, stderr)) orelse return 1;
    defer opened.deinit(service);
    const result = jmcl.server_lifecycle.stop(
        service.allocator,
        service.io,
        opened.server_dir,
        options.grace_ms,
    ) catch |err| switch (err) {
        error.InvalidGracePeriod => {
            try stderr.writeAll("server grace_ms is out of range\n");
            return 1;
        },
        else => {
            try stderr.print("failed to stop the server: {s}\n", .{@errorName(err)});
            return 1;
        },
    };
    if (result.termination == .not_running) {
        try stderr.writeAll("the server is not running\n");
        return 1;
    }
    try stdout.print("server\t{s}\nrunning\tfalse\ntermination\t{s}\n", .{
        options.id,
        @tagName(result.termination),
    });
    if (result.pid) |pid| try stdout.print("pid\t{d}\n", .{pid});
    return 0;
}

const ServerRestartOptions = struct {
    id: []const u8,
    grace_ms: u32 = jmcl.server_lifecycle.default_grace_ms,
    servers_directory: []const u8 = jmcl.server_store.default_root,
    java_paths: std.ArrayList([]const u8) = .empty,

    fn deinit(options: *ServerRestartOptions, allocator: std.mem.Allocator) void {
        options.java_paths.deinit(allocator);
    }
};

fn parseServerRestartOptions(allocator: std.mem.Allocator, args: []const []const u8) !ServerRestartOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;
    try jmcl.server_store.validateId(args[0]);
    var options: ServerRestartOptions = .{ .id = args[0] };
    errdefer options.deinit(allocator);
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const value = args[index + 1];
        if (std.mem.eql(u8, args[index], "--servers-directory")) {
            options.servers_directory = value;
        } else if (std.mem.eql(u8, args[index], "--grace-ms")) {
            options.grace_ms = std.fmt.parseInt(u32, value, 10) catch return error.InvalidArguments;
            if (options.grace_ms > jmcl.server_lifecycle.max_grace_ms) return error.InvalidArguments;
        } else if (std.mem.eql(u8, args[index], "--java-path")) {
            try options.java_paths.append(allocator, value);
        } else return error.InvalidArguments;
        index += 2;
    }
    return options;
}

fn runServerRestart(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    var options = parseServerRestartOptions(service.allocator, args) catch {
        try stderr.writeAll("usage: jmcl-core server restart ID [--grace-ms N] [--servers-directory PATH] [--java-path PATH ...]\n");
        return 2;
    };
    defer options.deinit(service.allocator);
    var opened = (try openServerTarget(service, options.servers_directory, options.id, stderr)) orelse return 1;
    defer opened.deinit(service);
    var summary = service.restartServer(
        opened.manifest.value,
        opened.server_dir,
        options.java_paths.items,
        options.grace_ms,
    ) catch |err| {
        try writeServerStartError(stderr, err);
        return 1;
    };
    defer summary.deinit();
    try stdout.print("server\t{s}\nrunning\ttrue\npid\t{d}\nsupervisorPid\t{d}\nstartedAtMs\t{d}\njava\t{s}\n", .{
        options.id,
        summary.pid,
        summary.supervisor_pid,
        summary.started_at_ms,
        summary.java_path,
    });
    return 0;
}

fn runServerStatus(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    const options = parseServerTargetOptions(args, true) catch {
        try stderr.writeAll("usage: jmcl-core server status ID [--servers-directory PATH] [--json]\n");
        return 2;
    };
    var opened = (try openServerTarget(service, options.servers_directory, options.id, stderr)) orelse return 1;
    defer opened.deinit(service);
    var status_result = jmcl.server_lifecycle.status(
        service.allocator,
        service.io,
        opened.server_dir,
    ) catch |err| {
        try stderr.print("failed to read the server lifecycle status: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer status_result.deinit();
    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("stage");
        try json.write("server.status");
        try json.objectField("id");
        try json.write(options.id);
        try json.objectField("running");
        try json.write(status_result.running);
        try json.objectField("stale_state");
        try json.write(status_result.stale_state);
        try json.objectField("pid");
        try json.write(status_result.pid);
        try json.objectField("supervisor_pid");
        try json.write(status_result.supervisor_pid);
        try json.objectField("java_path");
        try json.write(status_result.java_path);
        try json.objectField("started_at_ms");
        try json.write(status_result.started_at_ms);
        try json.objectField("uptime_ms");
        try json.write(status_result.uptime_ms);
        try json.endObject();
        try stdout.writeByte('\n');
    } else {
        try stdout.print("server\t{s}\nrunning\t{s}\nstaleState\t{s}\n", .{
            options.id,
            if (status_result.running) "true" else "false",
            if (status_result.stale_state) "true" else "false",
        });
        if (status_result.pid) |pid| try stdout.print("pid\t{d}\n", .{pid});
        if (status_result.supervisor_pid) |pid| try stdout.print("supervisorPid\t{d}\n", .{pid});
        if (status_result.java_path) |path| try stdout.print("java\t{s}\n", .{path});
        if (status_result.started_at_ms) |ms| try stdout.print("startedAtMs\t{d}\n", .{ms});
        if (status_result.uptime_ms) |ms| try stdout.print("uptimeMs\t{d}\n", .{ms});
    }
    return 0;
}

const ServerLogsOptions = struct {
    id: []const u8,
    cursor: ?u64 = null,
    file_id: ?u64 = null,
    max_bytes: usize = jmcl.server_lifecycle.default_log_read_bytes,
    servers_directory: []const u8 = jmcl.server_store.default_root,
    json: bool = false,
};

fn parseServerLogsOptions(args: []const []const u8) !ServerLogsOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;
    try jmcl.server_store.validateId(args[0]);
    var options: ServerLogsOptions = .{ .id = args[0] };
    var index: usize = 1;
    while (index < args.len) {
        const argument = args[index];
        if (std.mem.eql(u8, argument, "--json")) {
            options.json = true;
            index += 1;
            continue;
        }
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--servers-directory")) {
            options.servers_directory = value;
        } else if (std.mem.eql(u8, argument, "--cursor")) {
            options.cursor = std.fmt.parseInt(u64, value, 10) catch return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--file-id")) {
            options.file_id = std.fmt.parseInt(u64, value, 10) catch return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--max-bytes")) {
            options.max_bytes = std.fmt.parseInt(usize, value, 10) catch return error.InvalidArguments;
            if (options.max_bytes == 0 or options.max_bytes > jmcl.server_lifecycle.max_log_read_bytes) return error.InvalidArguments;
        } else return error.InvalidArguments;
        index += 2;
    }
    return options;
}

fn runServerLogs(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    const options = parseServerLogsOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core server logs ID [--cursor N] [--file-id N] [--max-bytes N] [--servers-directory PATH] [--json]\n");
        return 2;
    };
    var opened = (try openServerTarget(service, options.servers_directory, options.id, stderr)) orelse return 1;
    defer opened.deinit(service);
    var result = jmcl.server_lifecycle.readLogs(
        service.allocator,
        service.io,
        opened.server_dir,
        options.cursor,
        options.file_id,
        options.max_bytes,
    ) catch |err| switch (err) {
        error.ServerLogsNotFound => {
            try stderr.writeAll("no server console log exists\n");
            return 1;
        },
        error.InvalidLogReadSize => {
            try stderr.writeAll("server max_bytes is out of range\n");
            return 1;
        },
        else => {
            try stderr.print("failed to read the server console log: {s}\n", .{@errorName(err)});
            return 1;
        },
    };
    defer result.deinit();
    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("stage");
        try json.write("server.logs");
        try json.objectField("id");
        try json.write(options.id);
        try json.objectField("text");
        try json.write(result.text);
        try json.objectField("file_id");
        try json.write(result.file_id);
        try json.objectField("start_cursor");
        try json.write(result.start_cursor);
        try json.objectField("next_cursor");
        try json.write(result.next_cursor);
        try json.objectField("eof");
        try json.write(result.eof);
        try json.objectField("reset");
        try json.write(result.reset);
        try json.objectField("truncated");
        try json.write(result.truncated);
        try json.endObject();
        try stdout.writeByte('\n');
    } else {
        try stdout.writeAll(result.text);
    }
    return 0;
}

const ServerCommandOptions = struct {
    id: []const u8,
    command: []const u8,
    servers_directory: []const u8 = jmcl.server_store.default_root,
};

fn parseServerCommandOptions(args: []const []const u8) !ServerCommandOptions {
    if (args.len < 2 or args[0].len == 0) return error.InvalidArguments;
    try jmcl.server_store.validateId(args[0]);
    var options: ServerCommandOptions = .{ .id = args[0], .command = args[1] };
    jmcl.server_lifecycle.validateCommand(options.command) catch return error.InvalidArguments;
    var index: usize = 2;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const value = args[index + 1];
        if (std.mem.eql(u8, args[index], "--servers-directory")) {
            options.servers_directory = value;
        } else return error.InvalidArguments;
        index += 2;
    }
    return options;
}

fn runServerCommand(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (!try requireLinux(stderr)) return 1;
    const options = parseServerCommandOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core server command ID COMMAND [--servers-directory PATH]\n");
        return 2;
    };
    var opened = (try openServerTarget(service, options.servers_directory, options.id, stderr)) orelse return 1;
    defer opened.deinit(service);
    const pid = jmcl.server_lifecycle.command(
        service.allocator,
        service.io,
        opened.server_dir,
        options.command,
    ) catch |err| switch (err) {
        error.ServerNotRunning => {
            try stderr.writeAll("the server is not running\n");
            return 1;
        },
        error.InvalidServerCommand => {
            try stderr.writeAll("server command is invalid\n");
            return 1;
        },
        else => {
            try stderr.print("failed to send the server command: {s}\n", .{@errorName(err)});
            return 1;
        },
    };
    try stdout.print("server\t{s}\naccepted\ttrue\npid\t{d}\n", .{ options.id, pid });
    return 0;
}

test "create parser maps flags to fields" {
    const options = try parseServerCreateOptions(&.{
        "vanilla-1.21.4",
        "--version",
        "1.21.4",
        "--name",
        "Vanilla Server",
        "--source",
        "bmclapi",
        "--java-path",
        "/usr/bin/java",
        "--servers-directory",
        "/tmp/servers",
        "--json",
    });
    try std.testing.expectEqualStrings("vanilla-1.21.4", options.id);
    try std.testing.expectEqualStrings("1.21.4", options.version_id.?);
    try std.testing.expectEqualStrings("Vanilla Server", options.name.?);
    try std.testing.expectEqual(jmcl.DownloadSource.bmclapi, options.source);
    try std.testing.expectEqualStrings("/usr/bin/java", options.java_path.?);
    try std.testing.expectEqualStrings("/tmp/servers", options.servers_directory);
    try std.testing.expect(options.json);
}

test "create parser defaults" {
    const options = try parseServerCreateOptions(&.{ "srv", "--version", "1.21.4" });
    try std.testing.expectEqualStrings("srv", options.id);
    try std.testing.expectEqualStrings("1.21.4", options.version_id.?);
    try std.testing.expect(options.fabric_loader == null);
    try std.testing.expect(options.neoforge_version == null);
    try std.testing.expect(options.forge_version == null);
    try std.testing.expect(options.name == null);
    try std.testing.expect(options.java_path == null);
    try std.testing.expectEqual(jmcl.DownloadSource.official, options.source);
    try std.testing.expectEqualStrings(jmcl.server_store.default_root, options.servers_directory);
    try std.testing.expect(!options.json);
}

test "create parser maps each loader flag" {
    const fabric = try parseServerCreateOptions(&.{ "srv", "--version", "1.21.4", "--fabric-loader", "0.16.10" });
    try std.testing.expectEqualStrings("0.16.10", fabric.fabric_loader.?);
    const neoforge = try parseServerCreateOptions(&.{ "srv", "--version", "1.21.1", "--neoforge-version", "21.1.133" });
    try std.testing.expectEqualStrings("21.1.133", neoforge.neoforge_version.?);
    const forge = try parseServerCreateOptions(&.{ "srv", "--version", "1.20.1", "--forge-version", "47.3.12" });
    try std.testing.expectEqualStrings("47.3.12", forge.forge_version.?);
}

test "create parser rejects loader conflicts" {
    try std.testing.expectError(error.InvalidArguments, parseServerCreateOptions(&.{
        "srv",
        "--version",
        "1.21.4",
        "--fabric-loader",
        "0.16.10",
        "--neoforge-version",
        "21.1.0",
    }));
    try std.testing.expectError(error.InvalidArguments, parseServerCreateOptions(&.{
        "srv",
        "--version",
        "1.20.1",
        "--neoforge-version",
        "21.1.0",
        "--forge-version",
        "47.3.12",
    }));
}

test "create parser rejects missing id version unknown flags and empty values" {
    try std.testing.expectError(error.InvalidArguments, parseServerCreateOptions(&.{}));
    try std.testing.expectError(error.InvalidArguments, parseServerCreateOptions(&.{"srv"}));
    try std.testing.expectError(error.InvalidArguments, parseServerCreateOptions(&.{ "srv", "--version", "1.21.4", "--bogus", "x" }));
    try std.testing.expectError(error.InvalidArguments, parseServerCreateOptions(&.{ "srv", "--version" }));
    try std.testing.expectError(error.InvalidArguments, parseServerCreateOptions(&.{ "srv", "--version", "" }));
    try std.testing.expectError(error.InvalidArguments, parseServerCreateOptions(&.{ "srv", "--version", "1.21.4", "--source", "mirror" }));
    try std.testing.expectError(error.InvalidServerId, parseServerCreateOptions(&.{ "Bad Id", "--version", "1.21.4" }));
}

test "list parser accepts json and servers directory" {
    const defaults = try parseServerListOptions(&.{});
    try std.testing.expect(!defaults.json);
    try std.testing.expectEqualStrings(jmcl.server_store.default_root, defaults.servers_directory);
    const options = try parseServerListOptions(&.{ "--json", "--servers-directory", "/tmp/servers" });
    try std.testing.expect(options.json);
    try std.testing.expectEqualStrings("/tmp/servers", options.servers_directory);
    try std.testing.expectError(error.InvalidArguments, parseServerListOptions(&.{"--bogus"}));
    try std.testing.expectError(error.InvalidArguments, parseServerListOptions(&.{"--servers-directory"}));
}

test "target parser accepts id json and servers directory" {
    const options = try parseServerTargetOptions(&.{ "srv", "--json", "--servers-directory", "/tmp/servers" }, true);
    try std.testing.expectEqualStrings("srv", options.id);
    try std.testing.expect(options.json);
    try std.testing.expectEqualStrings("/tmp/servers", options.servers_directory);
    const plain = try parseServerTargetOptions(&.{"srv"}, false);
    try std.testing.expect(!plain.json);
    try std.testing.expectError(error.InvalidArguments, parseServerTargetOptions(&.{ "srv", "--json" }, false));
    try std.testing.expectError(error.InvalidArguments, parseServerTargetOptions(&.{}, false));
    try std.testing.expectError(error.InvalidArguments, parseServerTargetOptions(&.{ "srv", "--bogus" }, true));
    try std.testing.expectError(error.InvalidServerId, parseServerTargetOptions(&.{"Bad Id"}, false));
}

test "install parser maps accept-eula java paths and tuning flags" {
    var options = try parseServerInstallOptions(std.testing.allocator, &.{
        "srv",
        "--accept-eula",
        "--servers-directory",
        "/tmp/servers",
        "--store-directory",
        "/tmp/store",
        "--workers",
        "16",
        "--retries",
        "3",
        "--java-path",
        "/usr/lib/jvm/java-21/bin/java",
        "--java-path",
        "/usr/bin/java",
        "--json",
    });
    defer options.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings("srv", options.id);
    try std.testing.expect(options.accept_eula);
    try std.testing.expectEqualStrings("/tmp/servers", options.servers_directory);
    try std.testing.expectEqualStrings("/tmp/store", options.store_directory);
    try std.testing.expectEqual(@as(u8, 16), options.workers);
    try std.testing.expectEqual(@as(u8, 3), options.retries);
    try std.testing.expectEqual(@as(usize, 2), options.java_paths.items.len);
    try std.testing.expectEqualStrings("/usr/lib/jvm/java-21/bin/java", options.java_paths.items[0]);
    try std.testing.expectEqualStrings("/usr/bin/java", options.java_paths.items[1]);
    try std.testing.expect(options.json);
}

test "install parser defaults" {
    var options = try parseServerInstallOptions(std.testing.allocator, &.{ "srv", "--accept-eula" });
    defer options.deinit(std.testing.allocator);
    try std.testing.expect(options.accept_eula);
    try std.testing.expectEqual(@as(u8, 8), options.workers);
    try std.testing.expectEqual(@as(u8, 2), options.retries);
    try std.testing.expectEqual(@as(usize, 0), options.java_paths.items.len);
    try std.testing.expectEqualStrings(jmcl.server_store.default_root, options.servers_directory);
    try std.testing.expectEqualStrings(jmcl.artifact_store.default_directory, options.store_directory);
    try std.testing.expect(!options.json);
}

test "install parser rejects missing id invalid bounds and unknown flags" {
    try std.testing.expectError(error.InvalidArguments, parseServerInstallOptions(std.testing.allocator, &.{}));
    try std.testing.expectError(error.InvalidArguments, parseServerInstallOptions(std.testing.allocator, &.{ "srv", "--workers", "0" }));
    try std.testing.expectError(error.InvalidArguments, parseServerInstallOptions(std.testing.allocator, &.{ "srv", "--workers", "33" }));
    try std.testing.expectError(error.InvalidArguments, parseServerInstallOptions(std.testing.allocator, &.{ "srv", "--retries", "6" }));
    try std.testing.expectError(error.InvalidArguments, parseServerInstallOptions(std.testing.allocator, &.{ "srv", "--workers", "x" }));
    try std.testing.expectError(error.InvalidArguments, parseServerInstallOptions(std.testing.allocator, &.{ "srv", "--bogus" }));
    try std.testing.expectError(error.InvalidArguments, parseServerInstallOptions(std.testing.allocator, &.{ "srv", "--java-path" }));
    try std.testing.expectError(error.InvalidArguments, parseServerInstallOptions(std.testing.allocator, &.{ "srv", "--java-path", "" }));
    try std.testing.expectError(error.InvalidServerId, parseServerInstallOptions(std.testing.allocator, &.{ "Bad Id", "--accept-eula" }));
}

test "properties set parser collects key value pairs" {
    var options = try parseServerPropertiesSetOptions(std.testing.allocator, &.{
        "srv",
        "max-players=20",
        "--servers-directory",
        "/tmp/servers",
        "motd=hello=world",
        "server-ip=",
    });
    defer options.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings("srv", options.id);
    try std.testing.expectEqualStrings("/tmp/servers", options.servers_directory);
    try std.testing.expectEqual(@as(usize, 3), options.pairs.items.len);
    try std.testing.expectEqualStrings("max-players", options.pairs.items[0].key);
    try std.testing.expectEqualStrings("20", options.pairs.items[0].value);
    try std.testing.expectEqualStrings("motd", options.pairs.items[1].key);
    try std.testing.expectEqualStrings("hello=world", options.pairs.items[1].value);
    try std.testing.expectEqualStrings("server-ip", options.pairs.items[2].key);
    try std.testing.expectEqualStrings("", options.pairs.items[2].value);
}

test "properties set parser rejects bare tokens and missing input" {
    try std.testing.expectError(error.InvalidArguments, parseServerPropertiesSetOptions(std.testing.allocator, &.{}));
    try std.testing.expectError(error.InvalidArguments, parseServerPropertiesSetOptions(std.testing.allocator, &.{"srv"}));
    try std.testing.expectError(error.InvalidArguments, parseServerPropertiesSetOptions(std.testing.allocator, &.{ "srv", "max-players" }));
    try std.testing.expectError(error.InvalidArguments, parseServerPropertiesSetOptions(std.testing.allocator, &.{ "srv", "=20" }));
    try std.testing.expectError(error.InvalidArguments, parseServerPropertiesSetOptions(std.testing.allocator, &.{ "srv", "--json" }));
    try std.testing.expectError(error.InvalidArguments, parseServerPropertiesSetOptions(std.testing.allocator, &.{ "srv", "a=b", "--servers-directory" }));
    try std.testing.expectError(error.InvalidArguments, parseServerPropertiesSetOptions(std.testing.allocator, &.{ "srv", "a=b", "--servers-directory", "" }));
    try std.testing.expectError(error.InvalidServerId, parseServerPropertiesSetOptions(std.testing.allocator, &.{ "Bad Id", "a=b" }));
}

test "start parser maps java paths and servers directory" {
    var options = try parseServerStartOptions(std.testing.allocator, &.{
        "srv",
        "--java-path",
        "/usr/lib/jvm/java-21/bin/java",
        "--java-path",
        "/usr/bin/java",
        "--servers-directory",
        "/tmp/servers",
    });
    defer options.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings("srv", options.id);
    try std.testing.expectEqual(@as(usize, 2), options.java_paths.items.len);
    try std.testing.expectEqualStrings("/usr/lib/jvm/java-21/bin/java", options.java_paths.items[0]);
    try std.testing.expectEqualStrings("/usr/bin/java", options.java_paths.items[1]);
    try std.testing.expectEqualStrings("/tmp/servers", options.servers_directory);
}

test "start parser defaults and rejections" {
    var defaults = try parseServerStartOptions(std.testing.allocator, &.{"srv"});
    defer defaults.deinit(std.testing.allocator);
    try std.testing.expectEqual(@as(usize, 0), defaults.java_paths.items.len);
    try std.testing.expectEqualStrings(jmcl.server_store.default_root, defaults.servers_directory);
    try std.testing.expectError(error.InvalidArguments, parseServerStartOptions(std.testing.allocator, &.{}));
    try std.testing.expectError(error.InvalidArguments, parseServerStartOptions(std.testing.allocator, &.{ "srv", "--bogus", "x" }));
    try std.testing.expectError(error.InvalidArguments, parseServerStartOptions(std.testing.allocator, &.{ "srv", "--java-path" }));
    try std.testing.expectError(error.InvalidArguments, parseServerStartOptions(std.testing.allocator, &.{ "srv", "--java-path", "" }));
    try std.testing.expectError(error.InvalidArguments, parseServerStartOptions(std.testing.allocator, &.{ "srv", "--json" }));
    try std.testing.expectError(error.InvalidServerId, parseServerStartOptions(std.testing.allocator, &.{"Bad Id"}));
}

test "stop parser maps grace flag and rejects out of range" {
    const options = try parseServerStopOptions(&.{ "srv", "--grace-ms", "5000", "--servers-directory", "/tmp/servers" });
    try std.testing.expectEqualStrings("srv", options.id);
    try std.testing.expectEqual(@as(u32, 5000), options.grace_ms);
    try std.testing.expectEqualStrings("/tmp/servers", options.servers_directory);
    const defaults = try parseServerStopOptions(&.{"srv"});
    try std.testing.expectEqual(jmcl.server_lifecycle.default_grace_ms, defaults.grace_ms);
    try std.testing.expectError(error.InvalidArguments, parseServerStopOptions(&.{}));
    try std.testing.expectError(error.InvalidArguments, parseServerStopOptions(&.{ "srv", "--grace-ms", "120001" }));
    try std.testing.expectError(error.InvalidArguments, parseServerStopOptions(&.{ "srv", "--grace-ms", "x" }));
    try std.testing.expectError(error.InvalidArguments, parseServerStopOptions(&.{ "srv", "--grace-ms" }));
    try std.testing.expectError(error.InvalidArguments, parseServerStopOptions(&.{ "srv", "--json" }));
    try std.testing.expectError(error.InvalidServerId, parseServerStopOptions(&.{"Bad Id"}));
}

test "restart parser maps grace java paths and servers directory" {
    var options = try parseServerRestartOptions(std.testing.allocator, &.{
        "srv",
        "--grace-ms",
        "20000",
        "--java-path",
        "/usr/bin/java",
        "--servers-directory",
        "/tmp/servers",
    });
    defer options.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings("srv", options.id);
    try std.testing.expectEqual(@as(u32, 20000), options.grace_ms);
    try std.testing.expectEqual(@as(usize, 1), options.java_paths.items.len);
    try std.testing.expectEqualStrings("/usr/bin/java", options.java_paths.items[0]);
    try std.testing.expectEqualStrings("/tmp/servers", options.servers_directory);
    var defaults = try parseServerRestartOptions(std.testing.allocator, &.{"srv"});
    defer defaults.deinit(std.testing.allocator);
    try std.testing.expectEqual(jmcl.server_lifecycle.default_grace_ms, defaults.grace_ms);
    try std.testing.expectEqual(@as(usize, 0), defaults.java_paths.items.len);
    try std.testing.expectError(error.InvalidArguments, parseServerRestartOptions(std.testing.allocator, &.{}));
    try std.testing.expectError(error.InvalidArguments, parseServerRestartOptions(std.testing.allocator, &.{ "srv", "--grace-ms", "120001" }));
    try std.testing.expectError(error.InvalidArguments, parseServerRestartOptions(std.testing.allocator, &.{ "srv", "--grace-ms", "x" }));
    try std.testing.expectError(error.InvalidArguments, parseServerRestartOptions(std.testing.allocator, &.{ "srv", "--bogus", "x" }));
    try std.testing.expectError(error.InvalidServerId, parseServerRestartOptions(std.testing.allocator, &.{"Bad Id"}));
}

test "logs parser maps cursor file id max bytes and json" {
    const options = try parseServerLogsOptions(&.{
        "srv",
        "--cursor",
        "1024",
        "--file-id",
        "99",
        "--max-bytes",
        "4096",
        "--servers-directory",
        "/tmp/servers",
        "--json",
    });
    try std.testing.expectEqualStrings("srv", options.id);
    try std.testing.expectEqual(@as(?u64, 1024), options.cursor);
    try std.testing.expectEqual(@as(?u64, 99), options.file_id);
    try std.testing.expectEqual(@as(usize, 4096), options.max_bytes);
    try std.testing.expectEqualStrings("/tmp/servers", options.servers_directory);
    try std.testing.expect(options.json);
}

test "logs parser defaults and rejections" {
    const defaults = try parseServerLogsOptions(&.{"srv"});
    try std.testing.expect(defaults.cursor == null);
    try std.testing.expect(defaults.file_id == null);
    try std.testing.expectEqual(jmcl.server_lifecycle.default_log_read_bytes, defaults.max_bytes);
    try std.testing.expect(!defaults.json);
    try std.testing.expectError(error.InvalidArguments, parseServerLogsOptions(&.{}));
    try std.testing.expectError(error.InvalidArguments, parseServerLogsOptions(&.{ "srv", "--cursor", "x" }));
    try std.testing.expectError(error.InvalidArguments, parseServerLogsOptions(&.{ "srv", "--cursor" }));
    try std.testing.expectError(error.InvalidArguments, parseServerLogsOptions(&.{ "srv", "--file-id", "" }));
    try std.testing.expectError(error.InvalidArguments, parseServerLogsOptions(&.{ "srv", "--max-bytes", "0" }));
    try std.testing.expectError(error.InvalidArguments, parseServerLogsOptions(&.{ "srv", "--max-bytes", "1048577" }));
    try std.testing.expectError(error.InvalidArguments, parseServerLogsOptions(&.{ "srv", "--bogus", "x" }));
    try std.testing.expectError(error.InvalidServerId, parseServerLogsOptions(&.{"Bad Id"}));
}

test "command parser maps command and servers directory" {
    const options = try parseServerCommandOptions(&.{ "srv", "say hello", "--servers-directory", "/tmp/servers" });
    try std.testing.expectEqualStrings("srv", options.id);
    try std.testing.expectEqualStrings("say hello", options.command);
    try std.testing.expectEqualStrings("/tmp/servers", options.servers_directory);
}

test "command parser rejects missing invalid and multiline commands" {
    try std.testing.expectError(error.InvalidArguments, parseServerCommandOptions(&.{}));
    try std.testing.expectError(error.InvalidArguments, parseServerCommandOptions(&.{"srv"}));
    try std.testing.expectError(error.InvalidArguments, parseServerCommandOptions(&.{ "srv", "" }));
    try std.testing.expectError(error.InvalidArguments, parseServerCommandOptions(&.{ "srv", "say hi\nsay bye" }));
    try std.testing.expectError(error.InvalidArguments, parseServerCommandOptions(&.{ "srv", "say hi", "--bogus", "x" }));
    try std.testing.expectError(error.InvalidArguments, parseServerCommandOptions(&.{ "srv", "say hi", "--servers-directory" }));
    try std.testing.expectError(error.InvalidServerId, parseServerCommandOptions(&.{ "Bad Id", "say hi" }));
}
