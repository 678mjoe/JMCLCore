const std = @import("std");
const jmcl = @import("jmcl_core");

const max_request_bytes = 64 * 1024;

pub fn main(init: std.process.Init) !void {
    const args = try init.minimal.args.toSlice(init.arena.allocator());

    var stdout_buffer: [4096]u8 = undefined;
    var stdout_file: std.Io.File.Writer = .init(.stdout(), init.io, &stdout_buffer);
    const stdout = &stdout_file.interface;

    var stderr_buffer: [4096]u8 = undefined;
    var stderr_file: std.Io.File.Writer = .init(.stderr(), init.io, &stderr_buffer);
    const stderr = &stderr_file.interface;

    const exit_code = try run(init, args, stdout, stderr);
    try stdout.flush();
    try stderr.flush();
    if (exit_code != 0) std.process.exit(exit_code);
}

fn run(
    init: std.process.Init,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    var service = jmcl.Service.init(init.gpa, init.io, init.environ_map);

    if (args.len < 2 or std.mem.eql(u8, args[1], "help") or std.mem.eql(u8, args[1], "--help")) {
        try writeHelp(stdout);
        return 0;
    }

    if (std.mem.eql(u8, args[1], "version")) {
        if (args.len >= 3 and std.mem.eql(u8, args[2], "list")) {
            return runVersionList(&service, args[3..], stdout, stderr);
        }
        if (args.len >= 3 and std.mem.eql(u8, args[2], "show")) {
            return runVersionShow(&service, args[3..], stdout, stderr);
        }
        if (args.len == 2) {
            try stdout.print("{s}\n", .{jmcl.version});
            return 0;
        }
        if (args.len == 3 and std.mem.eql(u8, args[2], "--json")) {
            try jmcl.protocol.writeVersionDocument(stdout);
            return 0;
        }
        try stderr.writeAll("usage: jmcl-core version [--json]\n");
        return 2;
    }

    if (std.mem.eql(u8, args[1], "install")) {
        if (args.len >= 3 and std.mem.eql(u8, args[2], "plan")) {
            return runInstallPlan(&service, args[3..], stdout, stderr);
        }
        if (args.len >= 3 and std.mem.eql(u8, args[2], "execute")) {
            return runInstallExecute(&service, args[3..], stdout, stderr);
        }
        if (args.len >= 3 and std.mem.eql(u8, args[2], "prepare")) {
            return runInstallPrepare(&service, args[3..], stdout, stderr);
        }
        try stderr.writeAll("usage: jmcl-core install <plan|execute|prepare> ID [options]\n");
        return 2;
    }

    if (std.mem.eql(u8, args[1], "java")) {
        if (args.len >= 3 and std.mem.eql(u8, args[2], "detect")) {
            return runJavaDetect(&service, args[3..], stdout, stderr);
        }
        if (args.len >= 3 and std.mem.eql(u8, args[2], "select")) {
            return runJavaSelect(&service, args[3..], stdout, stderr);
        }
        try stderr.writeAll("usage: jmcl-core java <detect|select> [options]\n");
        return 2;
    }

    if (std.mem.eql(u8, args[1], "instance")) {
        if (args.len >= 3 and std.mem.eql(u8, args[2], "create")) return runInstanceCreate(&service, args[3..], stdout, stderr);
        if (args.len >= 3 and std.mem.eql(u8, args[2], "list")) return runInstanceList(&service, args[3..], stdout, stderr);
        if (args.len >= 3 and std.mem.eql(u8, args[2], "show")) return runInstanceShow(&service, args[3..], stdout, stderr);
        if (args.len >= 3 and std.mem.eql(u8, args[2], "delete")) return runInstanceDelete(&service, args[3..], stdout, stderr);
        if (args.len >= 3 and std.mem.eql(u8, args[2], "install")) return runInstanceInstall(&service, args[3..], stdout, stderr);
        if (args.len >= 3 and std.mem.eql(u8, args[2], "launch")) return runInstanceLaunch(&service, args[3..], stdout, stderr);
        try stderr.writeAll("usage: jmcl-core instance <create|list|show|delete|install|launch> [options]\n");
        return 2;
    }

    if (std.mem.eql(u8, args[1], "launch")) {
        if (args.len >= 3 and std.mem.eql(u8, args[2], "execute")) {
            return runLaunchExecute(&service, args[3..], stdout, stderr);
        }
        try stderr.writeAll("usage: jmcl-core launch execute ID [--directory PATH] --offline-name NAME [options]\n");
        return 2;
    }

    if (std.mem.eql(u8, args[1], "rpc")) {
        if (args.len != 2) {
            try stderr.writeAll("usage: jmcl-core rpc\n");
            return 2;
        }
        try runRpc(init, &service, stdout);
        return 0;
    }

    try stderr.print("unknown command: {s}\n", .{args[1]});
    try stderr.writeAll("run `jmcl-core --help` for usage\n");
    return 2;
}

const LaunchExecuteCliOptions = struct {
    id: []const u8,
    directory: []const u8 = ".minecraft",
    offline_name: ?[]const u8 = null,
    source: jmcl.DownloadSource = .official,
    arch: ?jmcl.install_plan.Architecture = null,
    java_paths: std.ArrayList([]const u8) = .empty,

    fn deinit(options: *LaunchExecuteCliOptions, allocator: std.mem.Allocator) void {
        options.java_paths.deinit(allocator);
    }
};

const CliGameReporter = struct {
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
    output_failed: bool = false,

    fn callback(raw_context: *anyopaque, event: jmcl.launch_executor.Event) void {
        const context: *CliGameReporter = @ptrCast(@alignCast(raw_context));
        if (context.output_failed) return;
        switch (event) {
            .started => |started| {
                context.stderr.print("game started: pid {d}\n", .{started.pid}) catch {
                    context.output_failed = true;
                    return;
                };
                context.stderr.flush() catch {
                    context.output_failed = true;
                };
            },
            .log => |log| {
                const writer = if (log.stream == .stdout) context.stdout else context.stderr;
                writer.writeAll(log.bytes) catch {
                    context.output_failed = true;
                    return;
                };
                if (log.end == .newline) writer.writeByte('\n') catch {
                    context.output_failed = true;
                    return;
                };
                writer.flush() catch {
                    context.output_failed = true;
                };
            },
        }
    }
};

fn runLaunchExecute(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    var options = parseLaunchExecuteOptions(service.allocator, args) catch {
        try stderr.writeAll("usage: jmcl-core launch execute ID [--directory PATH] --offline-name NAME [--source SOURCE] [--arch ARCH] [--java PATH]\n");
        return 2;
    };
    defer options.deinit(service.allocator);
    return executeLaunch(
        service,
        options.id,
        options.directory,
        options.offline_name.?,
        options.source,
        options.arch,
        options.java_paths.items,
        stdout,
        stderr,
    );
}

fn executeLaunch(
    service: *jmcl.Service,
    id: []const u8,
    directory: []const u8,
    offline_name: []const u8,
    source: jmcl.DownloadSource,
    arch: ?jmcl.install_plan.Architecture,
    java_paths: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const uuid = offlineUuid(offline_name);
    var reporter_context: CliGameReporter = .{ .stdout = stdout, .stderr = stderr };
    const result = jmcl.launch_executor.execute(
        service,
        .{
            .id = id,
            .directory = directory,
            .auth = .{
                .player_name = offline_name,
                .uuid = &uuid,
                .access_token = "offline",
                .user_type = "legacy",
            },
            .source = source,
            .arch = arch,
            .java_paths = java_paths,
        },
        .{ .context = &reporter_context, .callback = CliGameReporter.callback },
    ) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        try stderr.print("game launch failed: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (reporter_context.output_failed) return error.WriteFailed;
    try stderr.print("game stopped: {s}", .{@tagName(result.process.termination)});
    if (result.process.exit_code) |code| try stderr.print(" ({d})", .{code});
    try stderr.print(", native cleanup: {s}\n", .{@tagName(result.native_cleanup)});
    return if (result.process.termination == .exited) result.process.exit_code.? else 1;
}

fn parseLaunchExecuteOptions(allocator: std.mem.Allocator, args: []const []const u8) !LaunchExecuteCliOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;
    var options: LaunchExecuteCliOptions = .{ .id = args[0] };
    errdefer options.deinit(allocator);
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (value.len == 0) return error.InvalidArguments;
        if (std.mem.eql(u8, argument, "--directory")) {
            options.directory = value;
        } else if (std.mem.eql(u8, argument, "--offline-name")) {
            options.offline_name = value;
        } else if (std.mem.eql(u8, argument, "--source")) {
            options.source = jmcl.DownloadSource.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--arch")) {
            options.arch = jmcl.install_plan.Architecture.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--java")) {
            try options.java_paths.append(allocator, value);
        } else {
            return error.InvalidArguments;
        }
        index += 2;
    }
    if (options.offline_name == null) return error.InvalidArguments;
    return options;
}

fn offlineUuid(name: []const u8) [32]u8 {
    var digest: [std.crypto.hash.Md5.digest_length]u8 = undefined;
    var hasher = std.crypto.hash.Md5.init(.{});
    hasher.update("OfflinePlayer:");
    hasher.update(name);
    hasher.final(&digest);
    digest[6] = (digest[6] & 0x0f) | 0x30;
    digest[8] = (digest[8] & 0x3f) | 0x80;
    return std.fmt.bytesToHex(digest, .lower);
}

const InstanceCreateOptions = struct {
    id: []const u8,
    version_id: ?[]const u8 = null,
    name: ?[]const u8 = null,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    source: jmcl.DownloadSource = .official,
};

fn runInstanceCreate(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseInstanceCreateOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core instance create ID --version VERSION [--name NAME] [--instances-directory PATH] [--source SOURCE]\n");
        return 2;
    };
    var metadata = service.fetchVersionMetadataPreferred(options.version_id.?, options.source) catch |err| {
        try stderr.print("failed to resolve Minecraft version: {s}\n", .{@errorName(err)});
        return 1;
    };
    metadata.deinit();
    const root = jmcl.instance_store.openRoot(service.io, options.instances_directory) catch |err| {
        try stderr.print("failed to open instances directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    jmcl.instance_store.create(service.io, root, .{
        .id = options.id,
        .name = options.name orelse options.id,
        .version_id = options.version_id.?,
        .source = options.source,
    }) catch |err| {
        try stderr.print("failed to create instance: {s}\n", .{@errorName(err)});
        return 1;
    };
    const path = try jmcl.instance_store.minecraftPathAlloc(service.allocator, options.instances_directory, options.id);
    defer service.allocator.free(path);
    try stdout.print("instance\t{s}\nversion\t{s}\ndirectory\t{s}\n", .{ options.id, options.version_id.?, path });
    return 0;
}

fn parseInstanceCreateOptions(args: []const []const u8) !InstanceCreateOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;
    try jmcl.instance_store.validateId(args[0]);
    var options: InstanceCreateOptions = .{ .id = args[0] };
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--version")) {
            options.version_id = value;
        } else if (std.mem.eql(u8, argument, "--name")) {
            options.name = value;
        } else if (std.mem.eql(u8, argument, "--instances-directory")) {
            options.instances_directory = value;
        } else if (std.mem.eql(u8, argument, "--source")) {
            options.source = jmcl.DownloadSource.parse(value) orelse return error.InvalidArguments;
        } else return error.InvalidArguments;
        index += 2;
    }
    if (options.version_id == null) return error.InvalidArguments;
    return options;
}

const InstanceListOptions = struct {
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    json: bool = false,
};

fn runInstanceList(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseInstanceListOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core instance list [--instances-directory PATH] [--json]\n");
        return 2;
    };
    const root = jmcl.instance_store.openRoot(service.io, options.instances_directory) catch |err| {
        try stderr.print("failed to open instances directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    var instances = jmcl.instance_store.list(service.allocator, service.io, root) catch |err| {
        try stderr.print("failed to list instances: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer instances.deinit();
    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("instances");
        try json.beginArray();
        for (instances.items) |document| try json.write(document.value);
        try json.endArray();
        try json.endObject();
        try stdout.writeByte('\n');
    } else {
        for (instances.items) |document| {
            try stdout.print("{s}\t{s}\t{s}\t{s}\n", .{
                document.value.id,
                document.value.version_id,
                @tagName(document.value.source),
                document.value.name,
            });
        }
    }
    return 0;
}

fn parseInstanceListOptions(args: []const []const u8) !InstanceListOptions {
    var options: InstanceListOptions = .{};
    var index: usize = 0;
    while (index < args.len) {
        if (std.mem.eql(u8, args[index], "--json")) {
            options.json = true;
            index += 1;
        } else if (std.mem.eql(u8, args[index], "--instances-directory")) {
            if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
            options.instances_directory = args[index + 1];
            index += 2;
        } else return error.InvalidArguments;
    }
    return options;
}

const InstanceTargetOptions = struct {
    id: []const u8,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    json: bool = false,
    force: bool = false,
};

fn parseInstanceTargetOptions(args: []const []const u8, allow_json: bool, allow_force: bool) !InstanceTargetOptions {
    if (args.len == 0) return error.InvalidArguments;
    try jmcl.instance_store.validateId(args[0]);
    var options: InstanceTargetOptions = .{ .id = args[0] };
    var index: usize = 1;
    while (index < args.len) {
        if (allow_json and std.mem.eql(u8, args[index], "--json")) {
            options.json = true;
            index += 1;
        } else if (allow_force and std.mem.eql(u8, args[index], "--force")) {
            options.force = true;
            index += 1;
        } else if (std.mem.eql(u8, args[index], "--instances-directory")) {
            if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
            options.instances_directory = args[index + 1];
            index += 2;
        } else return error.InvalidArguments;
    }
    return options;
}

fn runInstanceShow(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseInstanceTargetOptions(args, true, false) catch {
        try stderr.writeAll("usage: jmcl-core instance show ID [--instances-directory PATH] [--json]\n");
        return 2;
    };
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
    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("instance");
        try json.write(document.value);
        try json.objectField("directory");
        try json.write(path);
        try json.endObject();
        try stdout.writeByte('\n');
    } else {
        try stdout.print("instance\t{s}\nname\t{s}\nversion\t{s}\nsource\t{s}\ndirectory\t{s}\n", .{
            document.value.id,
            document.value.name,
            document.value.version_id,
            @tagName(document.value.source),
            path,
        });
    }
    return 0;
}

fn runInstanceDelete(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseInstanceTargetOptions(args, false, true) catch {
        try stderr.writeAll("usage: jmcl-core instance delete ID --force [--instances-directory PATH]\n");
        return 2;
    };
    if (!options.force) {
        try stderr.writeAll("instance deletion requires --force\n");
        return 2;
    }
    const root = jmcl.instance_store.openRoot(service.io, options.instances_directory) catch |err| {
        try stderr.print("failed to open instances directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    jmcl.instance_store.delete(service.allocator, service.io, root, options.id) catch |err| {
        try stderr.print("failed to delete instance: {s}\n", .{@errorName(err)});
        return 1;
    };
    try stdout.print("deleted\t{s}\n", .{options.id});
    return 0;
}

const InstanceInstallOptions = struct {
    id: []const u8,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    store_directory: []const u8 = jmcl.artifact_store.default_directory,
    source: ?jmcl.DownloadSource = null,
    os: ?jmcl.install_plan.OperatingSystem = null,
    arch: ?jmcl.install_plan.Architecture = null,
    workers: u8 = 8,
    retries: u8 = 2,
};

fn runInstanceInstall(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseInstanceInstallOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core instance install ID [--instances-directory PATH] [--store-directory PATH] [--source SOURCE] [--os OS] [--arch ARCH] [--workers N] [--retries N]\n");
        return 2;
    };
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
    return executeInstall(
        service,
        document.value.version_id,
        path,
        options.source orelse document.value.source,
        options.store_directory,
        options.os,
        options.arch,
        options.workers,
        options.retries,
        stdout,
        stderr,
    );
}

fn parseInstanceInstallOptions(args: []const []const u8) !InstanceInstallOptions {
    if (args.len == 0) return error.InvalidArguments;
    try jmcl.instance_store.validateId(args[0]);
    var options: InstanceInstallOptions = .{ .id = args[0] };
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--instances-directory")) {
            options.instances_directory = value;
        } else if (std.mem.eql(u8, argument, "--store-directory")) {
            options.store_directory = value;
        } else if (std.mem.eql(u8, argument, "--source")) {
            options.source = jmcl.DownloadSource.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--os")) {
            options.os = jmcl.install_plan.OperatingSystem.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--arch")) {
            options.arch = jmcl.install_plan.Architecture.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--workers")) {
            options.workers = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
            if (options.workers == 0 or options.workers > 32) return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--retries")) {
            options.retries = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
            if (options.retries > 5) return error.InvalidArguments;
        } else return error.InvalidArguments;
        index += 2;
    }
    return options;
}

const InstanceLaunchOptions = struct {
    id: []const u8,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    offline_name: ?[]const u8 = null,
    arch: ?jmcl.install_plan.Architecture = null,
    java_paths: std.ArrayList([]const u8) = .empty,

    fn deinit(options: *InstanceLaunchOptions, allocator: std.mem.Allocator) void {
        options.java_paths.deinit(allocator);
    }
};

fn runInstanceLaunch(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    var options = parseInstanceLaunchOptions(service.allocator, args) catch {
        try stderr.writeAll("usage: jmcl-core instance launch ID --offline-name NAME [--instances-directory PATH] [--arch ARCH] [--java PATH]\n");
        return 2;
    };
    defer options.deinit(service.allocator);
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
    return executeLaunch(
        service,
        document.value.version_id,
        path,
        options.offline_name.?,
        document.value.source,
        options.arch,
        options.java_paths.items,
        stdout,
        stderr,
    );
}

fn parseInstanceLaunchOptions(allocator: std.mem.Allocator, args: []const []const u8) !InstanceLaunchOptions {
    if (args.len == 0) return error.InvalidArguments;
    try jmcl.instance_store.validateId(args[0]);
    var options: InstanceLaunchOptions = .{ .id = args[0] };
    errdefer options.deinit(allocator);
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--instances-directory")) {
            options.instances_directory = value;
        } else if (std.mem.eql(u8, argument, "--offline-name")) {
            options.offline_name = value;
        } else if (std.mem.eql(u8, argument, "--arch")) {
            options.arch = jmcl.install_plan.Architecture.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--java")) {
            try options.java_paths.append(allocator, value);
        } else return error.InvalidArguments;
        index += 2;
    }
    if (options.offline_name == null) return error.InvalidArguments;
    return options;
}

fn runJavaDetect(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    var paths: std.ArrayList([]const u8) = .empty;
    defer paths.deinit(service.allocator);
    var json_output = false;
    var index: usize = 0;
    while (index < args.len) {
        if (std.mem.eql(u8, args[index], "--json")) {
            json_output = true;
            index += 1;
        } else if (std.mem.eql(u8, args[index], "--path")) {
            if (index + 1 >= args.len or args[index + 1].len == 0) {
                try stderr.writeAll("usage: jmcl-core java detect [--path PATH] [--json]\n");
                return 2;
            }
            try paths.append(service.allocator, args[index + 1]);
            index += 2;
        } else {
            try stderr.writeAll("usage: jmcl-core java detect [--path PATH] [--json]\n");
            return 2;
        }
    }

    var detection = jmcl.java_runtime.detect(
        service.allocator,
        service.io,
        service.environ_map.?,
        paths.items,
    ) catch |err| {
        try stderr.print("Java detection failed: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer detection.deinit();

    if (json_output) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("protocol");
        try json.write(jmcl.protocol_version);
        try json.objectField("result");
        try json.beginObject();
        try json.objectField("runtimes");
        try json.write(detection.runtimes);
        try json.objectField("failures");
        try json.write(detection.failures);
        try json.endObject();
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }

    for (detection.runtimes) |runtime| {
        try stdout.print("{s}\t{d}\t{s}\t{s}\n", .{
            runtime.executable,
            runtime.major_version,
            runtime.os_arch,
            runtime.vendor orelse "unknown",
        });
    }
    for (detection.failures) |failure| {
        try stderr.print("Java probe failed: {s} ({s})\n", .{ failure.executable, failure.error_name });
    }
    return if (detection.runtimes.len == 0) 1 else 0;
}

const JavaSelectCliOptions = struct {
    id: []const u8,
    source: jmcl.DownloadSource = .official,
    arch: ?jmcl.install_plan.Architecture = null,
    paths: std.ArrayList([]const u8) = .empty,
    json: bool = false,

    fn deinit(options: *JavaSelectCliOptions, allocator: std.mem.Allocator) void {
        options.paths.deinit(allocator);
    }
};

fn runJavaSelect(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    var options = parseJavaSelectOptions(service.allocator, args) catch {
        try stderr.writeAll("usage: jmcl-core java select ID [--path PATH] [--source SOURCE] [--arch ARCH] [--json]\n");
        return 2;
    };
    defer options.deinit(service.allocator);

    var metadata = service.fetchVersionMetadata(options.id, options.source) catch |err| {
        try stderr.print("failed to fetch version metadata: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer metadata.deinit();
    const required_major = jmcl.java_runtime.requiredMajor(metadata.value) catch {
        try stderr.writeAll("version metadata contains an invalid Java requirement\n");
        return 1;
    };
    const host = jmcl.install_plan.Target.host() catch |err| {
        try stderr.print("unsupported host platform: {s}\n", .{@errorName(err)});
        return 1;
    };
    const required_arch = options.arch orelse host.arch;

    var detection = jmcl.java_runtime.detect(
        service.allocator,
        service.io,
        service.environ_map.?,
        options.paths.items,
    ) catch |err| {
        try stderr.print("Java detection failed: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer detection.deinit();
    const selected = jmcl.java_runtime.selectCompatible(detection, required_major, required_arch) catch {
        try stderr.print("no compatible Java found; required Java {d} for {s}\n", .{
            required_major,
            @tagName(required_arch),
        });
        return 1;
    };

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("protocol");
        try json.write(jmcl.protocol_version);
        try json.objectField("source");
        try json.write(@tagName(options.source));
        try json.objectField("result");
        try json.beginObject();
        try json.objectField("version_id");
        try json.write(options.id);
        try json.objectField("required_major");
        try json.write(required_major);
        try json.objectField("required_arch");
        try json.write(required_arch);
        try json.objectField("runtime");
        try json.write(selected);
        try json.endObject();
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }

    try stdout.print("version\t{s}\n", .{options.id});
    try stdout.print("requiredJava\t{d}\n", .{required_major});
    try stdout.print("executable\t{s}\n", .{selected.executable});
    try stdout.print("javaVersion\t{s}\n", .{selected.version orelse selected.specification_version});
    try stdout.print("architecture\t{s}\n", .{selected.os_arch});
    try stdout.print("vendor\t{s}\n", .{selected.vendor orelse "unknown"});
    return 0;
}

fn parseJavaSelectOptions(allocator: std.mem.Allocator, args: []const []const u8) !JavaSelectCliOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;
    var options: JavaSelectCliOptions = .{ .id = args[0] };
    errdefer options.deinit(allocator);
    var index: usize = 1;
    while (index < args.len) {
        const argument = args[index];
        if (std.mem.eql(u8, argument, "--json")) {
            options.json = true;
            index += 1;
            continue;
        }
        if (index + 1 >= args.len) return error.InvalidArguments;
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--path")) {
            if (value.len == 0) return error.InvalidArguments;
            try options.paths.append(allocator, value);
        } else if (std.mem.eql(u8, argument, "--source")) {
            options.source = jmcl.DownloadSource.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--arch")) {
            options.arch = jmcl.install_plan.Architecture.parse(value) orelse return error.InvalidArguments;
        } else {
            return error.InvalidArguments;
        }
        index += 2;
    }
    return options;
}

const InstallPrepareCliOptions = struct {
    id: []const u8,
    directory: []const u8 = ".minecraft",
    source: jmcl.DownloadSource = .official,
    os: ?jmcl.install_plan.OperatingSystem = null,
    arch: ?jmcl.install_plan.Architecture = null,
};

fn runInstallPrepare(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseInstallPrepareOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core install prepare ID [--directory PATH] [--source SOURCE] [--os OS] [--arch ARCH]\n");
        return 2;
    };
    const host = jmcl.install_plan.Target.host() catch |err| {
        try stderr.print("unsupported host platform: {s}\n", .{@errorName(err)});
        return 1;
    };
    const target: jmcl.install_plan.Target = .{
        .os = options.os orelse host.os,
        .arch = options.arch orelse host.arch,
    };

    var plan = service.createInstallPlan(options.id, options.source, target) catch |err| {
        try stderr.print("failed to create install plan from {s}: {s}\n", .{
            options.source.displayName(),
            @errorName(err),
        });
        return 1;
    };
    defer plan.deinit();
    const root = jmcl.installer.openRoot(service.io, options.directory) catch |err| {
        try stderr.print("failed to open install root: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);

    var result = jmcl.preparer.prepare(service.allocator, service.io, root, plan.value) catch |err| {
        try stderr.print("runtime preparation failed: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer result.deinit();

    try writeSourceNotice(stdout, options.source);
    try stdout.print("version\t{s}\n", .{options.id});
    if (result.native_directory) |path| try stdout.print("nativeDirectory\t{s}\n", .{path});
    try stdout.print("nativeFiles\t{d}\n", .{result.native_files});
    try stdout.print("projectionsCreated\t{d}\n", .{result.projections_created});
    try stdout.print("projectionsCached\t{d}\n", .{result.projections_cached});
    return 0;
}

fn parseInstallPrepareOptions(args: []const []const u8) !InstallPrepareCliOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;
    var options: InstallPrepareCliOptions = .{ .id = args[0] };
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--directory")) {
            if (value.len == 0) return error.InvalidArguments;
            options.directory = value;
        } else if (std.mem.eql(u8, argument, "--source")) {
            options.source = jmcl.DownloadSource.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--os")) {
            options.os = jmcl.install_plan.OperatingSystem.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--arch")) {
            options.arch = jmcl.install_plan.Architecture.parse(value) orelse return error.InvalidArguments;
        } else {
            return error.InvalidArguments;
        }
        index += 2;
    }
    return options;
}

const InstallExecuteCliOptions = struct {
    id: []const u8,
    directory: []const u8 = ".minecraft",
    store_directory: []const u8 = jmcl.artifact_store.default_directory,
    source: jmcl.DownloadSource = .official,
    os: ?jmcl.install_plan.OperatingSystem = null,
    arch: ?jmcl.install_plan.Architecture = null,
    workers: u8 = 8,
    retries: u8 = 2,
};

const CliInstallReporter = struct {
    mutex: std.Io.Mutex = .init,
    io: std.Io,
    writer: *std.Io.Writer,
    output_failed: bool = false,
    last_completed: usize = 0,

    fn callback(raw_context: *anyopaque, event: jmcl.installer.Event) void {
        const context: *CliInstallReporter = @ptrCast(@alignCast(raw_context));
        context.mutex.lockUncancelable(context.io);
        defer context.mutex.unlock(context.io);
        if (context.output_failed) return;

        switch (event) {
            .retry => |retry| context.writer.print(
                "retry {d}/{d}: {s} ({s})\n",
                .{ retry.attempt, retry.max_attempts, retry.path, retry.error_name },
            ) catch {
                context.output_failed = true;
                return;
            },
            .progress => |progress| {
                if (progress.files_completed <= context.last_completed) return;
                if (progress.files_completed % 100 != 0 and progress.files_completed != progress.files_total) return;
                context.last_completed = progress.files_completed;
                context.writer.print(
                    "download progress: {d}/{d} files, {d}/{d} bytes verified\n",
                    .{
                        progress.files_completed,
                        progress.files_total,
                        progress.bytes_verified,
                        progress.bytes_total,
                    },
                ) catch {
                    context.output_failed = true;
                    return;
                };
            },
            .file => {},
        }
        context.writer.flush() catch {
            context.output_failed = true;
        };
    }
};

fn runInstallExecute(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseInstallExecuteOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core install execute ID [--directory PATH] [--store-directory PATH] [--source SOURCE] [--os OS] [--arch ARCH] [--workers N] [--retries N]\n");
        return 2;
    };
    return executeInstall(
        service,
        options.id,
        options.directory,
        options.source,
        options.store_directory,
        options.os,
        options.arch,
        options.workers,
        options.retries,
        stdout,
        stderr,
    );
}

fn executeInstall(
    service: *jmcl.Service,
    id: []const u8,
    directory: []const u8,
    source: jmcl.DownloadSource,
    store_directory: []const u8,
    os: ?jmcl.install_plan.OperatingSystem,
    arch: ?jmcl.install_plan.Architecture,
    workers: u8,
    retries: u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const host = jmcl.install_plan.Target.host() catch |err| {
        try stderr.print("unsupported host platform: {s}\n", .{@errorName(err)});
        return 1;
    };
    const target: jmcl.install_plan.Target = .{
        .os = os orelse host.os,
        .arch = arch orelse host.arch,
    };

    var plan = service.createInstallPlan(id, source, target) catch |err| {
        try stderr.print("failed to create install plan from {s}: {s}\n", .{
            source.displayName(),
            @errorName(err),
        });
        return 1;
    };
    defer plan.deinit();

    const root = jmcl.installer.openRoot(service.io, directory) catch |err| {
        try stderr.print("failed to open install root: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    const store = jmcl.artifact_store.openRoot(service.io, store_directory) catch |err| {
        try stderr.print("failed to open artifact store: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer store.close(service.io);

    var reporter_context: CliInstallReporter = .{
        .io = service.io,
        .writer = stderr,
    };
    const summary = jmcl.installer.execute(
        service.allocator,
        service.io,
        service.environ_map,
        root,
        store,
        plan.value,
        .{ .workers = workers, .retries = retries },
        .{ .context = &reporter_context, .callback = CliInstallReporter.callback },
    ) catch |err| {
        try stderr.print("download failed: {s}\n", .{@errorName(err)});
        return 1;
    };
    if (reporter_context.output_failed) return error.WriteFailed;

    try writeSourceNotice(stdout, source);
    try stdout.print("version\t{s}\n", .{id});
    try stdout.print("downloaded\t{d}\n", .{summary.files_downloaded});
    try stdout.print("cached\t{d}\n", .{summary.files_cached});
    try stdout.print("hardLinked\t{d}\n", .{summary.files_hard_linked});
    try stdout.print("copied\t{d}\n", .{summary.files_copied});
    try stdout.print("verifiedBytes\t{d}\n", .{summary.bytes_verified});
    try stdout.print("transferredBytes\t{d}\n", .{summary.bytes_transferred});
    return 0;
}

fn parseInstallExecuteOptions(args: []const []const u8) !InstallExecuteCliOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;

    var options: InstallExecuteCliOptions = .{ .id = args[0] };
    var index: usize = 1;
    while (index < args.len) {
        const argument = args[index];
        if (index + 1 >= args.len) return error.InvalidArguments;
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--directory")) {
            if (value.len == 0) return error.InvalidArguments;
            options.directory = value;
        } else if (std.mem.eql(u8, argument, "--store-directory")) {
            if (value.len == 0) return error.InvalidArguments;
            options.store_directory = value;
        } else if (std.mem.eql(u8, argument, "--source")) {
            options.source = jmcl.DownloadSource.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--os")) {
            options.os = jmcl.install_plan.OperatingSystem.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--arch")) {
            options.arch = jmcl.install_plan.Architecture.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--workers")) {
            options.workers = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
            if (options.workers == 0 or options.workers > 32) return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--retries")) {
            options.retries = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
            if (options.retries > 5) return error.InvalidArguments;
        } else {
            return error.InvalidArguments;
        }
        index += 2;
    }
    return options;
}

const InstallPlanCliOptions = struct {
    id: []const u8,
    source: jmcl.DownloadSource = .official,
    os: ?jmcl.install_plan.OperatingSystem = null,
    arch: ?jmcl.install_plan.Architecture = null,
    json: bool = false,
};

fn runInstallPlan(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseInstallPlanOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core install plan ID [--source SOURCE] [--os OS] [--arch ARCH] [--json]\n");
        return 2;
    };
    const host = jmcl.install_plan.Target.host() catch |err| {
        try stderr.print("unsupported host platform: {s}\n", .{@errorName(err)});
        return 1;
    };
    const target: jmcl.install_plan.Target = .{
        .os = options.os orelse host.os,
        .arch = options.arch orelse host.arch,
    };

    var plan = service.createInstallPlan(options.id, options.source, target) catch |err| {
        try stderr.print("failed to create install plan from {s}: {s}\n", .{
            options.source.displayName(),
            @errorName(err),
        });
        return 1;
    };
    defer plan.deinit();

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("protocol");
        try json.write(jmcl.protocol_version);
        try json.objectField("source");
        try json.write(@tagName(options.source));
        try json.objectField("result");
        try json.write(plan.value);
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }

    try writeSourceNotice(stdout, options.source);
    try stdout.print("version\t{s}\n", .{plan.value.version_id});
    try stdout.print("target\t{s}\t{s}\n", .{ @tagName(target.os), @tagName(target.arch) });
    try stdout.print("files\t{d}\n", .{plan.value.files.len});
    try stdout.print("classpath\t{d}\n", .{plan.value.classpath.len});
    try stdout.print("nativeExtractions\t{d}\n", .{plan.value.native_extractions.len});
    try stdout.print("assetProjections\t{d}\n", .{plan.value.asset_projections.len});
    try stdout.print("downloadBytes\t{d}\n", .{plan.value.total_download_size});
    return 0;
}

fn parseInstallPlanOptions(args: []const []const u8) !InstallPlanCliOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;

    var options: InstallPlanCliOptions = .{ .id = args[0] };
    var index: usize = 1;
    while (index < args.len) {
        const argument = args[index];
        if (std.mem.eql(u8, argument, "--json")) {
            options.json = true;
            index += 1;
        } else if (std.mem.eql(u8, argument, "--source")) {
            if (index + 1 >= args.len) return error.InvalidArguments;
            options.source = jmcl.DownloadSource.parse(args[index + 1]) orelse return error.InvalidArguments;
            index += 2;
        } else if (std.mem.eql(u8, argument, "--os")) {
            if (index + 1 >= args.len) return error.InvalidArguments;
            options.os = jmcl.install_plan.OperatingSystem.parse(args[index + 1]) orelse return error.InvalidArguments;
            index += 2;
        } else if (std.mem.eql(u8, argument, "--arch")) {
            if (index + 1 >= args.len) return error.InvalidArguments;
            options.arch = jmcl.install_plan.Architecture.parse(args[index + 1]) orelse return error.InvalidArguments;
            index += 2;
        } else {
            return error.InvalidArguments;
        }
    }
    return options;
}

fn runVersionShow(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseVersionShowOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core version show ID [--source SOURCE] [--json]\n");
        return 2;
    };

    var metadata = service.fetchVersionMetadata(options.id, options.source) catch |err| {
        if (err == error.VersionNotFound) {
            try stderr.print("Minecraft version not found: {s}\n", .{options.id});
        } else {
            try stderr.print("failed to fetch Minecraft version metadata from {s}: {s}\n", .{
                options.source.displayName(),
                @errorName(err),
            });
        }
        return 1;
    };
    defer metadata.deinit();

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("protocol");
        try json.write(jmcl.protocol_version);
        try json.objectField("source");
        try json.write(@tagName(options.source));
        try json.objectField("result");
        try json.write(metadata.value);
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }

    try writeSourceNotice(stdout, options.source);
    try stdout.print("id\t{s}\n", .{metadata.value.id});
    try stdout.print("type\t{s}\n", .{metadata.value.type});
    try stdout.print("mainClass\t{s}\n", .{metadata.value.mainClass});
    try stdout.print("libraries\t{d}\n", .{metadata.value.libraries.len});
    try stdout.print("client\t{s}\n", .{metadata.value.downloads.client.url});
    if (metadata.value.assetIndex) |asset_index| {
        try stdout.print("assetIndex\t{s}\n", .{asset_index.id});
    }
    if (metadata.value.javaVersion) |java_version| {
        try stdout.print("java\t{d}\t{s}\n", .{ java_version.majorVersion, java_version.component });
    }
    try stdout.print("arguments\t{s}\n", .{if (metadata.value.arguments != null) "modern" else "legacy"});
    return 0;
}

const VersionShowCliOptions = struct {
    id: []const u8,
    source: jmcl.DownloadSource = .official,
    json: bool = false,
};

fn parseVersionShowOptions(args: []const []const u8) !VersionShowCliOptions {
    if (args.len == 0 or args[0].len == 0) return error.InvalidArguments;

    var options: VersionShowCliOptions = .{ .id = args[0] };
    var index: usize = 1;
    while (index < args.len) {
        if (std.mem.eql(u8, args[index], "--json")) {
            options.json = true;
            index += 1;
        } else if (std.mem.eql(u8, args[index], "--source")) {
            if (index + 1 >= args.len) return error.InvalidArguments;
            options.source = jmcl.DownloadSource.parse(args[index + 1]) orelse return error.InvalidArguments;
            index += 2;
        } else {
            return error.InvalidArguments;
        }
    }
    return options;
}

const VersionListCliOptions = struct {
    list: jmcl.version_manifest.ListOptions = .{},
    source: jmcl.DownloadSource = .official,
    json: bool = false,
};

fn runVersionList(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseVersionListOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core version list [--type TYPE] [--limit N] [--source SOURCE] [--json]\n");
        return 2;
    };

    var manifest = service.fetchVersionManifest(options.source) catch |err| {
        try stderr.print("failed to fetch version manifest from {s}: {s}\n", .{
            options.source.displayName(),
            @errorName(err),
        });
        return 1;
    };
    defer manifest.deinit();

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("protocol");
        try json.write(jmcl.protocol_version);
        try json.objectField("source");
        try json.write(@tagName(options.source));
        try json.objectField("result");
        try jmcl.version_manifest.writeSelection(&json, manifest.value, options.list);
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }

    try writeSourceNotice(stdout, options.source);
    var written: usize = 0;
    for (manifest.value.versions) |version| {
        if (!options.list.includes(version)) continue;
        if (written == options.list.limit) break;
        try stdout.print("{s}\t{s}\t{s}\n", .{
            version.id,
            @tagName(version.type),
            version.releaseTime,
        });
        written += 1;
    }
    return 0;
}

fn parseVersionListOptions(args: []const []const u8) !VersionListCliOptions {
    var options: VersionListCliOptions = .{};
    var index: usize = 0;
    while (index < args.len) {
        const arg = args[index];
        if (std.mem.eql(u8, arg, "--json")) {
            options.json = true;
            index += 1;
        } else if (std.mem.eql(u8, arg, "--type")) {
            if (index + 1 >= args.len) return error.InvalidArguments;
            options.list.kind = jmcl.version_manifest.VersionType.parse(args[index + 1]) orelse
                return error.InvalidArguments;
            index += 2;
        } else if (std.mem.eql(u8, arg, "--limit")) {
            if (index + 1 >= args.len) return error.InvalidArguments;
            options.list.limit = std.fmt.parseInt(usize, args[index + 1], 10) catch
                return error.InvalidArguments;
            if (options.list.limit == 0 or options.list.limit > 1000) return error.InvalidArguments;
            index += 2;
        } else if (std.mem.eql(u8, arg, "--source")) {
            if (index + 1 >= args.len) return error.InvalidArguments;
            options.source = jmcl.DownloadSource.parse(args[index + 1]) orelse
                return error.InvalidArguments;
            index += 2;
        } else {
            return error.InvalidArguments;
        }
    }
    return options;
}

fn writeSourceNotice(writer: *std.Io.Writer, source: jmcl.DownloadSource) std.Io.Writer.Error!void {
    if (source == .bmclapi) {
        try writer.print("# source\t{s}\n", .{source.displayName()});
    }
}

fn runRpc(init: std.process.Init, service: *jmcl.Service, stdout: *std.Io.Writer) !void {
    var stdin_buffer: [max_request_bytes]u8 = undefined;
    defer std.crypto.secureZero(u8, &stdin_buffer);
    var stdin_file: std.Io.File.Reader = .init(.stdin(), init.io, &stdin_buffer);
    const stdin = &stdin_file.interface;

    while (true) {
        const maybe_line = stdin.takeDelimiter('\n') catch |err| switch (err) {
            error.StreamTooLong => {
                _ = try stdin.discardDelimiterInclusive('\n');
                std.crypto.secureZero(u8, &stdin_buffer);
                try jmcl.protocol.handleLine(service, "", stdout);
                try stdout.flush();
                continue;
            },
            else => return err,
        };
        const raw_line = maybe_line orelse break;
        defer std.crypto.secureZero(u8, @constCast(raw_line));
        const line = if (std.mem.endsWith(u8, raw_line, "\r"))
            raw_line[0 .. raw_line.len - 1]
        else
            raw_line;
        if (line.len == 0) continue;

        try jmcl.protocol.handleLine(service, line, stdout);
        try stdout.flush();
    }
}

fn writeHelp(writer: *std.Io.Writer) std.Io.Writer.Error!void {
    try writer.writeAll(
        \\JMCL Core
        \\
        \\Usage:
        \\  jmcl-core version [--json]  Show core and protocol version
        \\  jmcl-core version list      Fetch and list Minecraft versions
        \\  jmcl-core version show ID   Fetch detailed version metadata
        \\  jmcl-core install plan ID   Generate an immutable install plan
        \\  jmcl-core install execute ID Download and verify static files
        \\  jmcl-core install prepare ID Prepare natives and legacy assets
        \\  jmcl-core instance <command>  Manage isolated instances
        \\  jmcl-core java detect         Detect local Java runtimes
        \\  jmcl-core java select ID      Select a compatible Java runtime
        \\  jmcl-core launch execute ID   Launch Minecraft
        \\  jmcl-core rpc               Start a JSON Lines RPC session
        \\  jmcl-core help              Show this help
        \\
    );
}

test "help includes available commands" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();

    try writeHelp(&output.writer);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "jmcl-core rpc") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "jmcl-core launch execute") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "jmcl-core instance") != null);
}

test "version list options parse filters and limits" {
    const options = try parseVersionListOptions(&.{ "--type", "release", "--limit", "5", "--source", "bmclapi", "--json" });
    try std.testing.expectEqual(jmcl.version_manifest.VersionType.release, options.list.kind.?);
    try std.testing.expectEqual(@as(usize, 5), options.list.limit);
    try std.testing.expect(options.json);
    try std.testing.expectEqual(jmcl.DownloadSource.bmclapi, options.source);
}

test "version show options parse source" {
    const options = try parseVersionShowOptions(&.{ "1.21.11", "--source", "bmclapi", "--json" });
    try std.testing.expectEqualStrings("1.21.11", options.id);
    try std.testing.expectEqual(jmcl.DownloadSource.bmclapi, options.source);
    try std.testing.expect(options.json);
}

test "install plan options parse target platform" {
    const options = try parseInstallPlanOptions(&.{
        "1.21.11",
        "--source",
        "bmclapi",
        "--os",
        "windows",
        "--arch",
        "amd64",
        "--json",
    });
    try std.testing.expectEqual(jmcl.DownloadSource.bmclapi, options.source);
    try std.testing.expectEqual(jmcl.install_plan.OperatingSystem.windows, options.os.?);
    try std.testing.expectEqual(jmcl.install_plan.Architecture.x86_64, options.arch.?);
    try std.testing.expect(options.json);
}

test "install execute options use a Minecraft directory and validate concurrency" {
    const options = try parseInstallExecuteOptions(&.{
        "1.21.11",
        "--directory",
        "game-data",
        "--store-directory",
        "shared-data",
        "--workers",
        "4",
        "--retries",
        "1",
    });
    try std.testing.expectEqualStrings("game-data", options.directory);
    try std.testing.expectEqualStrings("shared-data", options.store_directory);
    try std.testing.expectEqual(@as(u8, 4), options.workers);
    try std.testing.expectEqual(@as(u8, 1), options.retries);
    const defaults = try parseInstallExecuteOptions(&.{"1.21.11"});
    try std.testing.expectEqualStrings(".minecraft", defaults.directory);
    try std.testing.expectError(error.InvalidArguments, parseInstallExecuteOptions(&.{
        "1.21.11",
        "--directory",
        "data",
        "--workers",
        "0",
    }));
}

test "install prepare options use a Minecraft directory" {
    const options = try parseInstallPrepareOptions(&.{
        "1.6.4",
        "--directory",
        "game-data",
        "--os",
        "windows",
    });
    try std.testing.expectEqualStrings("game-data", options.directory);
    try std.testing.expectEqual(jmcl.install_plan.OperatingSystem.windows, options.os.?);
    const defaults = try parseInstallPrepareOptions(&.{"1.6.4"});
    try std.testing.expectEqualStrings(".minecraft", defaults.directory);
}

test "launch execute options default to Minecraft directory" {
    var options = try parseLaunchExecuteOptions(std.testing.allocator, &.{ "26.1", "--offline-name", "Player" });
    defer options.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings(".minecraft", options.directory);
    try std.testing.expectEqualStrings("Player", options.offline_name.?);
}

test "launch execute options accept directory and Java" {
    var options = try parseLaunchExecuteOptions(std.testing.allocator, &.{
        "26.1",
        "--directory",
        "/games/test/.minecraft",
        "--offline-name",
        "Player",
        "--java",
        "/example/java",
        "--source",
        "bmclapi",
    });
    defer options.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings("/games/test/.minecraft", options.directory);
    try std.testing.expectEqualStrings("/example/java", options.java_paths.items[0]);
    try std.testing.expectEqual(jmcl.DownloadSource.bmclapi, options.source);
}

test "launch execute requires an offline name" {
    try std.testing.expectError(
        error.InvalidArguments,
        parseLaunchExecuteOptions(std.testing.allocator, &.{"26.1"}),
    );
}

test "offline UUID matches Java name UUID" {
    try std.testing.expectEqualStrings("cb7ecbf7173e39428f9fb92995eab3d0", &offlineUuid("OfflinePlayer"));
}

test "instance create options pin version and source" {
    const options = try parseInstanceCreateOptions(&.{
        "vanilla-26.1",
        "--version",
        "26.1",
        "--name",
        "Vanilla 26.1",
        "--source",
        "bmclapi",
    });
    try std.testing.expectEqualStrings("26.1", options.version_id.?);
    try std.testing.expectEqualStrings("Vanilla 26.1", options.name.?);
    try std.testing.expectEqual(jmcl.DownloadSource.bmclapi, options.source);
}

test "instance create rejects unsafe ids and missing versions" {
    try std.testing.expectError(error.InvalidInstanceId, parseInstanceCreateOptions(&.{ "../escape", "--version", "26.1" }));
    try std.testing.expectError(error.InvalidArguments, parseInstanceCreateOptions(&.{"vanilla"}));
}

test "instance list parses root and JSON output" {
    const options = try parseInstanceListOptions(&.{ "--instances-directory", "/games/instances", "--json" });
    try std.testing.expectEqualStrings("/games/instances", options.instances_directory);
    try std.testing.expect(options.json);
}

test "instance install validates worker limits" {
    const options = try parseInstanceInstallOptions(&.{ "vanilla", "--workers", "4", "--arch", "arm64", "--store-directory", "cache", "--source", "bmclapi" });
    try std.testing.expectEqual(@as(u8, 4), options.workers);
    try std.testing.expectEqual(jmcl.install_plan.Architecture.arm64, options.arch.?);
    try std.testing.expectEqualStrings("cache", options.store_directory);
    try std.testing.expectEqual(jmcl.DownloadSource.bmclapi, options.source.?);
    try std.testing.expectError(error.InvalidArguments, parseInstanceInstallOptions(&.{ "vanilla", "--workers", "0" }));
}

test "instance launch requires offline name" {
    try std.testing.expectError(
        error.InvalidArguments,
        parseInstanceLaunchOptions(std.testing.allocator, &.{"vanilla"}),
    );
}

test "Java select options collect explicit paths" {
    var options = try parseJavaSelectOptions(std.testing.allocator, &.{
        "1.21.11",
        "--path",
        "/example/java",
        "--arch",
        "arm64",
        "--json",
    });
    defer options.deinit(std.testing.allocator);
    try std.testing.expectEqual(@as(usize, 1), options.paths.items.len);
    try std.testing.expectEqualStrings("/example/java", options.paths.items[0]);
    try std.testing.expectEqual(jmcl.install_plan.Architecture.arm64, options.arch.?);
    try std.testing.expect(options.json);
}

test "version list options reject invalid limits" {
    try std.testing.expectError(error.InvalidArguments, parseVersionListOptions(&.{ "--limit", "0" }));
    try std.testing.expectError(error.InvalidArguments, parseVersionListOptions(&.{ "--limit", "1001" }));
}
