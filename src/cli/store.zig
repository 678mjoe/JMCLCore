const std = @import("std");
const jmcl = @import("jmcl_core");

pub fn run(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "gc")) {
        return runStoreGc(service, args[1..], stdout, stderr);
    }
    try stderr.writeAll("usage: jmcl-core store gc [--store-directory PATH] [--min-age-seconds N] [--execute] [--json]\n");
    return 2;
}

const StoreGcCliOptions = struct {
    store_directory: []const u8 = jmcl.artifact_store.default_directory,
    min_age_seconds: u64 = 24 * 60 * 60,
    dry_run: bool = true,
    json: bool = false,
};

fn runStoreGc(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseStoreGcOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core store gc [--store-directory PATH] [--min-age-seconds N] [--execute] [--json]\n");
        return 2;
    };
    const root = jmcl.artifact_store.openExistingRoot(service.io, options.store_directory) catch |err| {
        try stderr.print("failed to open artifact store: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    const summary = jmcl.artifact_store.collect(service.allocator, service.io, root, .{
        .dry_run = options.dry_run,
        .min_age_seconds = options.min_age_seconds,
    }) catch |err| {
        try stderr.print("artifact collection failed: {s}\n", .{@errorName(err)});
        return 1;
    };

    if (options.json) {
        var json: std.json.Stringify = .{ .writer = stdout };
        try json.beginObject();
        try json.objectField("protocol");
        try json.write(jmcl.protocol_version);
        try json.objectField("result");
        try json.beginObject();
        try json.objectField("dry_run");
        try json.write(options.dry_run);
        try json.objectField("min_age_seconds");
        try json.write(options.min_age_seconds);
        try json.objectField("summary");
        try json.write(summary);
        try json.endObject();
        try json.endObject();
        try stdout.writeByte('\n');
        return if (summary.errors == 0) 0 else 1;
    }

    try stdout.print("mode\t{s}\n", .{if (options.dry_run) "dry-run" else "execute"});
    try stdout.print("minAgeSeconds\t{d}\n", .{options.min_age_seconds});
    try stdout.print("scanned\t{d}\n", .{summary.objects_scanned});
    try stdout.print("eligible\t{d}\n", .{summary.objects_eligible});
    try stdout.print("deleted\t{d}\n", .{summary.objects_deleted});
    try stdout.print("eligibleBytes\t{d}\n", .{summary.bytes_eligible});
    try stdout.print("deletedBytes\t{d}\n", .{summary.bytes_deleted});
    try stdout.print("linked\t{d}\n", .{summary.objects_linked});
    try stdout.print("young\t{d}\n", .{summary.objects_young});
    try stdout.print("ignored\t{d}\n", .{summary.objects_ignored});
    try stdout.print("busy\t{d}\n", .{summary.objects_busy});
    try stdout.print("errors\t{d}\n", .{summary.errors});
    return if (summary.errors == 0) 0 else 1;
}

fn parseStoreGcOptions(args: []const []const u8) !StoreGcCliOptions {
    var options: StoreGcCliOptions = .{};
    var index: usize = 0;
    while (index < args.len) {
        const argument = args[index];
        if (std.mem.eql(u8, argument, "--execute")) {
            options.dry_run = false;
            index += 1;
        } else if (std.mem.eql(u8, argument, "--json")) {
            options.json = true;
            index += 1;
        } else if (std.mem.eql(u8, argument, "--store-directory") or std.mem.eql(u8, argument, "--min-age-seconds")) {
            if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
            const value = args[index + 1];
            if (std.mem.eql(u8, argument, "--store-directory")) {
                options.store_directory = value;
            } else {
                options.min_age_seconds = std.fmt.parseInt(u64, value, 10) catch return error.InvalidArguments;
            }
            index += 2;
        } else return error.InvalidArguments;
    }
    return options;
}

test "store GC options default to dry run" {
    const defaults = try parseStoreGcOptions(&.{});
    try std.testing.expect(defaults.dry_run);
    try std.testing.expectEqual(@as(u64, 24 * 60 * 60), defaults.min_age_seconds);
    const execute = try parseStoreGcOptions(&.{
        "--store-directory",
        "cache",
        "--min-age-seconds",
        "60",
        "--execute",
        "--json",
    });
    try std.testing.expect(!execute.dry_run);
    try std.testing.expectEqualStrings("cache", execute.store_directory);
    try std.testing.expectEqual(@as(u64, 60), execute.min_age_seconds);
    try std.testing.expect(execute.json);
}
