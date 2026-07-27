const std = @import("std");
const builtin = @import("builtin");
const jmcl = @import("jmcl_core");

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
    try stderr.writeAll("usage: jmcl-core server <create|list|show|delete> [options]\n");
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
