const std = @import("std");
const jmcl = @import("jmcl_core");
const common = @import("common.zig");
const cli_java = @import("java.zig");

pub fn run(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "plan")) {
        return runInstallPlan(service, args[1..], stdout, stderr);
    }
    if (args.len >= 1 and std.mem.eql(u8, args[0], "execute")) {
        return runInstallExecute(service, args[1..], stdout, stderr);
    }
    if (args.len >= 1 and std.mem.eql(u8, args[0], "prepare")) {
        return runInstallPrepare(service, args[1..], stdout, stderr);
    }
    try stderr.writeAll("usage: jmcl-core install <plan|execute|prepare> ID [options]\n");
    return 2;
}

const InstallPrepareCliOptions = struct {
    id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
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
        try stderr.writeAll("usage: jmcl-core install prepare ID [--fabric-loader VERSION | --neoforge-version VERSION | --forge-version VERSION] [--directory PATH] [--source SOURCE] [--os OS] [--arch ARCH]\n");
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

    var plan = service.createInstallPlanForSelection(.{
        .id = options.id,
        .fabric_loader = options.fabric_loader,
        .neoforge_version = options.neoforge_version,
    }, options.source, target) catch |err| {
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

    try common.writeSourceNotice(stdout, options.source);
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
        } else if (std.mem.eql(u8, argument, "--fabric-loader")) {
            if (value.len == 0) return error.InvalidArguments;
            options.fabric_loader = value;
        } else if (std.mem.eql(u8, argument, "--neoforge-version")) {
            if (value.len == 0) return error.InvalidArguments;
            options.neoforge_version = value;
        } else if (std.mem.eql(u8, argument, "--forge-version")) {
            if (value.len == 0) return error.InvalidArguments;
            options.forge_version = value;
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
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    directory: []const u8 = ".minecraft",
    store_directory: []const u8 = jmcl.artifact_store.default_directory,
    source: jmcl.DownloadSource = .official,
    os: ?jmcl.install_plan.OperatingSystem = null,
    arch: ?jmcl.install_plan.Architecture = null,
    workers: u8 = 8,
    retries: u8 = 2,
    java_policy: jmcl.java_managed.Policy = .auto,
};

pub const CliInstallReporter = struct {
    mutex: std.Io.Mutex = .init,
    io: std.Io,
    writer: *std.Io.Writer,
    output_failed: bool = false,
    last_completed: usize = 0,

    pub fn callback(raw_context: *anyopaque, event: jmcl.installer.Event) void {
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

const CliProcessorReporter = struct {
    writer: *std.Io.Writer,
    output_failed: bool = false,

    fn callback(raw_context: *anyopaque, event: jmcl.neoforge_executor.Event) void {
        const context: *CliProcessorReporter = @ptrCast(@alignCast(raw_context));
        if (context.output_failed) return;
        switch (event) {
            .processor_started => |started| {
                context.writer.print("processor {d}/{d}: {s}\n", .{
                    started.index,
                    started.total,
                    started.name,
                }) catch {
                    context.output_failed = true;
                    return;
                };
                context.writer.flush() catch {
                    context.output_failed = true;
                };
            },
        }
    }
};

fn runInstallExecute(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseInstallExecuteOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core install execute ID [--fabric-loader VERSION | --neoforge-version VERSION | --forge-version VERSION] [--directory PATH] [--store-directory PATH] [--source SOURCE] [--os OS] [--arch ARCH] [--workers N] [--retries N]\n");
        return 2;
    };
    return executeInstall(
        service,
        options.id,
        options.fabric_loader,
        options.neoforge_version,
        options.forge_version,
        options.directory,
        options.source,
        options.store_directory,
        options.os,
        options.arch,
        options.workers,
        options.retries,
        &.{},
        options.java_policy,
        stdout,
        stderr,
    );
}

pub fn executeInstall(
    service: *jmcl.Service,
    id: []const u8,
    fabric_loader: ?[]const u8,
    neoforge_version: ?[]const u8,
    forge_version: ?[]const u8,
    directory: []const u8,
    source: jmcl.DownloadSource,
    store_directory: []const u8,
    os: ?jmcl.install_plan.OperatingSystem,
    arch: ?jmcl.install_plan.Architecture,
    workers: u8,
    retries: u8,
    java_paths: []const []const u8,
    java_policy: jmcl.java_managed.Policy,
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

    var plan = service.createInstallPlanForSelection(.{
        .id = id,
        .fabric_loader = fabric_loader,
        .neoforge_version = neoforge_version,
        .forge_version = forge_version,
    }, source, target) catch |err| {
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

    var processor_java_paths: []const []const u8 = java_paths;
    var processor_java: ?jmcl.java_managed.Result = null;
    defer if (processor_java) |*resolved| resolved.deinit();
    if (plan.value.loader == .neoforge or plan.value.loader == .forge) {
        var runtime_reporter_context: cli_java.CliRuntimeReporter = .{
            .io = service.io,
            .writer = stderr,
        };
        processor_java = service.resolveGameJava(.{
            .required_major = plan.value.java_major_version,
            .arch = target.arch,
            .policy = java_policy,
            .java_paths = java_paths,
            .store_directory = store_directory,
            .allow_download = true,
        }, .{
            .context = &runtime_reporter_context,
            .callback = cli_java.CliRuntimeReporter.callback,
        }) catch |err| {
            try stderr.print("processor Java resolution failed: {s}\n", .{@errorName(err)});
            return 1;
        };
        processor_java_paths = &.{processor_java.?.runtime.executable};
    }

    if (plan.value.loader == .neoforge) {
        const loader_version = neoforge_version.?;
        var processor_reporter: CliProcessorReporter = .{ .writer = stderr };
        const patched = jmcl.neoforge_executor.run(
            service.allocator,
            service.io,
            service.environ_map,
            root,
            store,
            loader_version,
            plan.value.version_id,
            .{
                .java_paths = processor_java_paths,
                .required_major = plan.value.java_major_version,
            },
            .{ .context = &processor_reporter, .callback = CliProcessorReporter.callback },
        ) catch |err| {
            try stderr.print("NeoForge processor chain failed: {s}\n", .{@errorName(err)});
            return 1;
        };
        defer service.allocator.free(patched.patched_sha1);
        try stdout.print("processors\t{d}\n", .{patched.processors_run});
        try stdout.print("patchedClient\t{s}\n", .{patched.patched_sha1});
    }
    if (plan.value.loader == .forge) {
        const loader_version = forge_version.?;
        var processor_reporter: CliProcessorReporter = .{ .writer = stderr };
        const patched = jmcl.forge_executor.run(
            service.allocator,
            service.io,
            service.environ_map,
            root,
            store,
            id,
            loader_version,
            plan.value.version_id,
            .{
                .java_paths = processor_java_paths,
                .required_major = plan.value.java_major_version,
            },
            .{ .context = &processor_reporter, .callback = CliProcessorReporter.callback },
        ) catch |err| {
            try stderr.print("Forge processor chain failed: {s}\n", .{@errorName(err)});
            return 1;
        };
        defer service.allocator.free(patched.patched_sha1);
        try stdout.print("processors\t{d}\n", .{patched.processors_run});
        try stdout.print("patchedClient\t{s}\n", .{patched.patched_sha1});
    }

    jmcl.instance_store.recordCompletedInstallation(
        service.io,
        root,
        id,
        fabric_loader,
        neoforge_version,
        forge_version,
        source,
        @tagName(target.os),
        @tagName(target.arch),
    ) catch |err| {
        try stderr.print("failed to record completed installation: {s}\n", .{@errorName(err)});
        return 1;
    };

    try common.writeSourceNotice(stdout, source);
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
        } else if (std.mem.eql(u8, argument, "--fabric-loader")) {
            if (value.len == 0) return error.InvalidArguments;
            options.fabric_loader = value;
        } else if (std.mem.eql(u8, argument, "--neoforge-version")) {
            if (value.len == 0) return error.InvalidArguments;
            options.neoforge_version = value;
        } else if (std.mem.eql(u8, argument, "--forge-version")) {
            if (value.len == 0) return error.InvalidArguments;
            options.forge_version = value;
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
        } else if (std.mem.eql(u8, argument, "--java-policy")) {
            options.java_policy = jmcl.java_managed.Policy.parse(value) orelse return error.InvalidArguments;
        } else {
            return error.InvalidArguments;
        }
        index += 2;
    }
    return options;
}

const InstallPlanCliOptions = struct {
    id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
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
        try stderr.writeAll("usage: jmcl-core install plan ID [--fabric-loader VERSION | --neoforge-version VERSION | --forge-version VERSION] [--source SOURCE] [--os OS] [--arch ARCH] [--json]\n");
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

    var plan = service.createInstallPlanForSelection(.{
        .id = options.id,
        .fabric_loader = options.fabric_loader,
        .neoforge_version = options.neoforge_version,
        .forge_version = options.forge_version,
    }, options.source, target) catch |err| {
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

    try common.writeSourceNotice(stdout, options.source);
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
        } else if (std.mem.eql(u8, argument, "--fabric-loader")) {
            if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
            options.fabric_loader = args[index + 1];
            index += 2;
        } else if (std.mem.eql(u8, argument, "--neoforge-version")) {
            if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
            options.neoforge_version = args[index + 1];
            index += 2;
        } else if (std.mem.eql(u8, argument, "--forge-version")) {
            if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
            options.forge_version = args[index + 1];
            index += 2;
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
