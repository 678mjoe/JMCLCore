const std = @import("std");
const jmcl = @import("jmcl_core");

pub fn run(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "execute")) {
        return runLaunchExecute(service, args[1..], stdout, stderr);
    }
    try stderr.writeAll("usage: jmcl-core launch execute ID [--directory PATH] --offline-name NAME [options]\n");
    return 2;
}

const LaunchExecuteCliOptions = struct {
    id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    directory: []const u8 = ".minecraft",
    offline_name: ?[]const u8 = null,
    source: jmcl.DownloadSource = .official,
    arch: ?jmcl.install_plan.Architecture = null,
    java_paths: std.ArrayList([]const u8) = .empty,
    java_policy: jmcl.java_managed.Policy = .auto,
    java_override: ?[]const u8 = null,
    store_directory: []const u8 = jmcl.artifact_store.default_directory,

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
            .runtime => |runtime_event| {
                switch (runtime_event) {
                    .stage => |stage| context.stderr.print("java runtime stage: {s}\n", .{@tagName(stage)}) catch {
                        context.output_failed = true;
                        return;
                    },
                    .retry => |retry| context.stderr.print(
                        "java runtime retry {d}/{d}: {s} ({s})\n",
                        .{ retry.attempt, retry.max_attempts, retry.path, retry.error_name },
                    ) catch {
                        context.output_failed = true;
                        return;
                    },
                    .progress => |progress| context.stderr.print(
                        "java runtime download: {d}/{d} bytes\n",
                        .{ progress.bytes_processed, progress.bytes_total },
                    ) catch {
                        context.output_failed = true;
                        return;
                    },
                    .file => {},
                }
                context.stderr.flush() catch {
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
        try stderr.writeAll("usage: jmcl-core launch execute ID [--fabric-loader VERSION | --neoforge-version VERSION | --forge-version VERSION] [--directory PATH] --offline-name NAME [--source SOURCE] [--arch ARCH] [--java PATH]\n");
        return 2;
    };
    defer options.deinit(service.allocator);
    return executeLaunch(
        service,
        options.id,
        options.fabric_loader,
        options.neoforge_version,
        options.forge_version,
        options.directory,
        options.offline_name.?,
        options.source,
        options.arch,
        options.java_paths.items,
        options.java_policy,
        options.java_override,
        options.store_directory,
        stdout,
        stderr,
    );
}

pub fn executeLaunch(
    service: *jmcl.Service,
    id: []const u8,
    fabric_loader: ?[]const u8,
    neoforge_version: ?[]const u8,
    forge_version: ?[]const u8,
    directory: []const u8,
    offline_name: []const u8,
    source: jmcl.DownloadSource,
    arch: ?jmcl.install_plan.Architecture,
    java_paths: []const []const u8,
    java_policy: jmcl.java_managed.Policy,
    java_override: ?[]const u8,
    store_directory: []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const uuid = offlineUuid(offline_name);
    var reporter_context: CliGameReporter = .{ .stdout = stdout, .stderr = stderr };
    const result = jmcl.launch_executor.execute(
        service,
        .{
            .id = id,
            .fabric_loader = fabric_loader,
            .neoforge_version = neoforge_version,
            .forge_version = forge_version,
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
            .java_policy = java_policy,
            .java_override = java_override,
            .store_directory = store_directory,
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
        } else if (std.mem.eql(u8, argument, "--fabric-loader")) {
            options.fabric_loader = value;
        } else if (std.mem.eql(u8, argument, "--neoforge-version")) {
            options.neoforge_version = value;
        } else if (std.mem.eql(u8, argument, "--forge-version")) {
            options.forge_version = value;
        } else if (std.mem.eql(u8, argument, "--offline-name")) {
            options.offline_name = value;
        } else if (std.mem.eql(u8, argument, "--source")) {
            options.source = jmcl.DownloadSource.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--arch")) {
            options.arch = jmcl.install_plan.Architecture.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--java")) {
            try options.java_paths.append(allocator, value);
        } else if (std.mem.eql(u8, argument, "--java-policy")) {
            options.java_policy = jmcl.java_managed.Policy.parse(value) orelse return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--java-override")) {
            options.java_override = value;
        } else if (std.mem.eql(u8, argument, "--store")) {
            options.store_directory = value;
        } else {
            return error.InvalidArguments;
        }
        index += 2;
    }
    if (options.offline_name == null) return error.InvalidArguments;
    if (options.java_override != null and options.java_paths.items.len != 0) return error.InvalidArguments;
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
