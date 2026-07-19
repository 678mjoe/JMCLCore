const std = @import("std");
const jmcl = @import("jmcl_core");
const common = @import("common.zig");

pub fn run(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "list")) {
        return runVersionList(service, args[1..], stdout, stderr);
    }
    if (args.len >= 1 and std.mem.eql(u8, args[0], "show")) {
        return runVersionShow(service, args[1..], stdout, stderr);
    }
    if (args.len == 0) {
        try stdout.print("{s}\n", .{jmcl.version});
        return 0;
    }
    if (args.len == 1 and std.mem.eql(u8, args[0], "--json")) {
        try jmcl.protocol.writeVersionDocument(stdout);
        return 0;
    }
    try stderr.writeAll("usage: jmcl-core version [--json]\n");
    return 2;
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

    try common.writeSourceNotice(stdout, options.source);
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

    try common.writeSourceNotice(stdout, options.source);
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

test "version list options reject invalid limits" {
    try std.testing.expectError(error.InvalidArguments, parseVersionListOptions(&.{ "--limit", "0" }));
    try std.testing.expectError(error.InvalidArguments, parseVersionListOptions(&.{ "--limit", "1001" }));
}
