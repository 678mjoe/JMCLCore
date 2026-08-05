const std = @import("std");
const jmcl = @import("jmcl_core");

pub fn run(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "detect")) {
        return runJavaDetect(service, args[1..], stdout, stderr);
    }
    if (args.len >= 1 and std.mem.eql(u8, args[0], "select")) {
        return runJavaSelect(service, args[1..], stdout, stderr);
    }
    if (args.len >= 1 and std.mem.eql(u8, args[0], "runtime")) {
        return runJavaRuntime(service, args[1..], stdout, stderr);
    }
    try stderr.writeAll("usage: jmcl-core java <detect|select|runtime> [options]\n");
    return 2;
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


const JavaRuntimeCliOptions = struct {
    store_directory: []const u8 = jmcl.artifact_store.default_directory,
    component: ?[]const u8 = null,
    major: ?u16 = null,
    provider: jmcl.Service.ManagedProviderChoice = .auto,
    platform: ?[]const u8 = null,
    workers: u8 = 8,
    retries: u8 = 2,
    json: bool = false,
};

const CliRuntimeReporter = struct {
    mutex: std.Io.Mutex = .init,
    io: std.Io,
    writer: *std.Io.Writer,
    output_failed: bool = false,
    last_percent: u64 = 0,

    fn callback(raw_context: *anyopaque, event: jmcl.runtime_installer.Event) void {
        const context: *CliRuntimeReporter = @ptrCast(@alignCast(raw_context));
        context.mutex.lockUncancelable(context.io);
        defer context.mutex.unlock(context.io);
        if (context.output_failed) return;

        switch (event) {
            .stage => |stage| context.writer.print("stage: {s}\n", .{@tagName(stage)}) catch {
                context.output_failed = true;
                return;
            },
            .retry => |retry| context.writer.print(
                "retry {d}/{d}: {s} ({s})\n",
                .{ retry.attempt, retry.max_attempts, retry.path, retry.error_name },
            ) catch {
                context.output_failed = true;
                return;
            },
            .progress => |progress| {
                if (progress.bytes_total == 0) return;
                const percent = progress.bytes_processed * 100 / progress.bytes_total;
                const completed = progress.files_completed == progress.files_total;
                if (!completed and percent < context.last_percent + 10) return;
                context.last_percent = percent;
                context.writer.print(
                    "download progress: {d}/{d} files, {d}/{d} bytes\n",
                    .{ progress.files_completed, progress.files_total, progress.bytes_processed, progress.bytes_total },
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

fn runJavaRuntime(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "list")) {
        return runJavaRuntimeList(service, args[1..], stdout, stderr);
    }
    if (args.len >= 1 and std.mem.eql(u8, args[0], "install")) {
        return runJavaRuntimeInstall(service, args[1..], stdout, stderr);
    }
    if (args.len >= 1 and std.mem.eql(u8, args[0], "remove")) {
        return runJavaRuntimeRemove(service, args[1..], stdout, stderr);
    }
    try stderr.writeAll("usage: jmcl-core java runtime <list|install|remove> [options]\n");
    return 2;
}

fn parseRuntimeStoreArgs(
    allocator: std.mem.Allocator,
    args: []const []const u8,
    allow_json: bool,
) !JavaRuntimeCliOptions {
    _ = allocator;
    var options: JavaRuntimeCliOptions = .{};
    var index: usize = 0;
    while (index < args.len) {
        const argument = args[index];
        if (allow_json and std.mem.eql(u8, argument, "--json")) {
            options.json = true;
            index += 1;
            continue;
        }
        if (index + 1 >= args.len) return error.InvalidArguments;
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--store")) {
            if (value.len == 0) return error.InvalidArguments;
            options.store_directory = value;
        } else return error.InvalidArguments;
        index += 2;
    }
    return options;
}

fn writeReceiptJson(json: *std.json.Stringify, name: []const u8, receipt: jmcl.java_runtime_store.Receipt) !void {
    try json.objectField("name");
    try json.write(name);
    try json.objectField("provider");
    try json.write(@tagName(receipt.provider));
    try json.objectField("id");
    try json.write(receipt.id);
    try json.objectField("platform");
    try json.write(receipt.platform);
    try json.objectField("major_version");
    try json.write(receipt.major_version);
    try json.objectField("executable");
    try json.write(receipt.executable);
    try json.objectField("version");
    try json.write(receipt.version);
    try json.objectField("vendor");
    try json.write(receipt.vendor);
}

fn runJavaRuntimeList(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseRuntimeStoreArgs(service.allocator, args, true) catch {
        try stderr.writeAll("usage: jmcl-core java runtime list [--store DIR] [--json]\n");
        return 2;
    };
    var runtimes = service.listManagedRuntimes(options.store_directory) catch |err| {
        try stderr.print("failed to list managed runtimes: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer runtimes.deinit();

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("protocol");
        try json.write(jmcl.protocol_version);
        try json.objectField("result");
        try json.beginObject();
        try json.objectField("runtimes");
        try json.beginArray();
        for (runtimes.items) |document| {
            try json.beginObject();
            try writeReceiptJson(&json, document.name, document.parsed.value);
            try json.endObject();
        }
        try json.endArray();
        try json.endObject();
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }

    for (runtimes.items) |document| {
        const receipt = document.parsed.value;
        try stdout.print("{s}\t{s}\t{d}\t{s}\t{s}\n", .{
            document.name,
            @tagName(receipt.provider),
            receipt.major_version,
            receipt.version orelse "unknown",
            receipt.executable,
        });
    }
    return 0;
}

fn runJavaRuntimeInstall(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    var options: JavaRuntimeCliOptions = .{};
    var index: usize = 0;
    while (index < args.len) {
        const argument = args[index];
        if (std.mem.eql(u8, argument, "--json")) {
            options.json = true;
            index += 1;
            continue;
        }
        if (index + 1 >= args.len) {
            try stderr.writeAll("usage: jmcl-core java runtime install (--component NAME | --major N) [--provider auto|mojang|zulu] [--platform PLATFORM] [--store DIR] [--workers N] [--retries N] [--json]\n");
            return 2;
        }
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--component")) {
            if (value.len == 0) return error.InvalidArguments;
            options.component = value;
        } else if (std.mem.eql(u8, argument, "--major")) {
            options.major = std.fmt.parseInt(u16, value, 10) catch {
                try stderr.writeAll("invalid --major value\n");
                return 2;
            };
            if (options.major.? == 0) {
                try stderr.writeAll("invalid --major value\n");
                return 2;
            }
        } else if (std.mem.eql(u8, argument, "--provider")) {
            options.provider = jmcl.Service.ManagedProviderChoice.parse(value) orelse {
                try stderr.writeAll("provider must be auto, mojang, or zulu\n");
                return 2;
            };
        } else if (std.mem.eql(u8, argument, "--platform")) {
            if (jmcl.java_runtime_store.parsePlatform(value) == null) {
                try stderr.writeAll("unknown platform\n");
                return 2;
            }
            options.platform = value;
        } else if (std.mem.eql(u8, argument, "--store")) {
            if (value.len == 0) return error.InvalidArguments;
            options.store_directory = value;
        } else if (std.mem.eql(u8, argument, "--workers")) {
            options.workers = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--retries")) {
            options.retries = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
        } else {
            try stderr.writeAll("usage: jmcl-core java runtime install (--component NAME | --major N) [--provider auto|mojang|zulu] [--platform PLATFORM] [--store DIR] [--workers N] [--retries N] [--json]\n");
            return 2;
        }
        index += 2;
    }
    if ((options.component == null) == (options.major == null)) {
        try stderr.writeAll("exactly one of --component or --major is required\n");
        return 2;
    }

    var reporter_context: CliRuntimeReporter = .{
        .io = service.io,
        .writer = stderr,
    };
    var outcome = service.installManagedRuntime(.{
        .component = options.component,
        .major = options.major,
        .provider = options.provider,
        .platform = options.platform,
        .store_directory = options.store_directory,
        .workers = options.workers,
        .retries = options.retries,
    }, .{ .context = &reporter_context, .callback = CliRuntimeReporter.callback }) catch |err| {
        try stderr.print("managed runtime install failed: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer outcome.deinit();

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("protocol");
        try json.write(jmcl.protocol_version);
        try json.objectField("result");
        try json.beginObject();
        try json.objectField("already_installed");
        try json.write(outcome.already_installed);
        try json.objectField("runtime");
        try json.beginObject();
        try writeReceiptJson(&json, outcome.runtime_name, outcome.receipt);
        try json.endObject();
        try json.endObject();
        try json.endObject();
        try stdout.writeByte('\n');
        return 0;
    }

    const receipt = outcome.receipt;
    try stdout.print("runtime\t{s}\n", .{outcome.runtime_name});
    try stdout.print("alreadyInstalled\t{s}\n", .{if (outcome.already_installed) "true" else "false"});
    try stdout.print("provider\t{s}\n", .{@tagName(receipt.provider)});
    try stdout.print("major\t{d}\n", .{receipt.major_version});
    try stdout.print("version\t{s}\n", .{receipt.version orelse "unknown"});
    try stdout.print("vendor\t{s}\n", .{receipt.vendor orelse "unknown"});
    try stdout.print("executable\t{s}\n", .{receipt.executable});
    return 0;
}

fn runJavaRuntimeRemove(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0 or args[0].len == 0) {
        try stderr.writeAll("usage: jmcl-core java runtime remove NAME [--store DIR]\n");
        return 2;
    }
    const name = args[0];
    const options = parseRuntimeStoreArgs(service.allocator, args[1..], false) catch {
        try stderr.writeAll("usage: jmcl-core java runtime remove NAME [--store DIR]\n");
        return 2;
    };
    service.removeManagedRuntime(options.store_directory, name) catch |err| {
        try stderr.print("managed runtime remove failed: {s}\n", .{@errorName(err)});
        return 1;
    };
    try stdout.print("removed\t{s}\n", .{name});
    return 0;
}
