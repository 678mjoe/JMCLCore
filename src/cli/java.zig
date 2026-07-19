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
    try stderr.writeAll("usage: jmcl-core java <detect|select> [options]\n");
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
