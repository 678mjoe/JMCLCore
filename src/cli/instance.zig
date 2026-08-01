const std = @import("std");
const jmcl = @import("jmcl_core");
const cli_install = @import("install.zig");
const cli_launch = @import("launch.zig");
const cli_content = @import("content.zig");
const cli_modpack = @import("modpack.zig");

pub fn run(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "create")) return runInstanceCreate(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "list")) return runInstanceList(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "show")) return runInstanceShow(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "delete")) return runInstanceDelete(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "install")) return runInstanceInstall(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "launch")) return runInstanceLaunch(service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "mods")) return cli_content.runWithKind(.mod, service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "resourcepacks")) return cli_content.runWithKind(.resourcepack, service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "shaderpacks")) return cli_content.runWithKind(.shaderpack, service, args[1..], stdout, stderr);
    if (args.len >= 1 and std.mem.eql(u8, args[0], "modpack")) return cli_modpack.run(service, args[1..], stdout, stderr);
    try stderr.writeAll("usage: jmcl-core instance <create|list|show|delete|install|launch|mods|resourcepacks|shaderpacks|modpack> [options]\n");
    return 2;
}

const InstanceCreateOptions = struct {
    id: []const u8,
    version_id: ?[]const u8 = null,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    name: ?[]const u8 = null,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    source: jmcl.DownloadSource = .official,
    validate: bool = true,
};

fn runInstanceCreate(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    const options = parseInstanceCreateOptions(args) catch {
        try stderr.writeAll("usage: jmcl-core instance create ID --version VERSION [--fabric-loader VERSION | --neoforge-version VERSION | --forge-version VERSION] [--name NAME] [--instances-directory PATH] [--source SOURCE] [--no-validate]\n");
        return 2;
    };
    const loader_count: u8 = @as(u8, @intFromBool(options.fabric_loader != null)) +
        @intFromBool(options.neoforge_version != null) +
        @intFromBool(options.forge_version != null);
    if (loader_count > 1) {
        try stderr.writeAll("only one of --fabric-loader, --neoforge-version, or --forge-version may be given\n");
        return 2;
    }
    var metadata = if (options.validate) blk: {
        const resolved = service.resolveMetadataPreferred(.{
            .id = options.version_id.?,
            .fabric_loader = options.fabric_loader,
            .neoforge_version = options.neoforge_version,
            .forge_version = options.forge_version,
        }, options.source) catch |err| {
            try stderr.print("failed to resolve Minecraft version: {s}\n", .{@errorName(err)});
            return 1;
        };
        break :blk resolved;
    } else null;
    if (metadata) |*resolved| resolved.deinit();
    const root = jmcl.instance_store.openRoot(service.io, options.instances_directory) catch |err| {
        try stderr.print("failed to open instances directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    jmcl.instance_store.create(service.io, root, .{
        .id = options.id,
        .name = options.name orelse options.id,
        .version_id = options.version_id.?,
        .fabric_loader = options.fabric_loader,
        .neoforge_version = options.neoforge_version,
        .forge_version = options.forge_version,
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
        if (std.mem.eql(u8, args[index], "--no-validate")) {
            options.validate = false;
            index += 1;
            continue;
        }
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
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
    java_paths: std.ArrayList([]const u8) = .empty,

    fn deinit(options: *InstanceInstallOptions, allocator: std.mem.Allocator) void {
        options.java_paths.deinit(allocator);
    }
};

fn runInstanceInstall(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    var options = parseInstanceInstallOptions(service.allocator, args) catch {
        try stderr.writeAll("usage: jmcl-core instance install ID [--instances-directory PATH] [--store-directory PATH] [--source SOURCE] [--os OS] [--arch ARCH] [--workers N] [--retries N] [--java PATH]\n");
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
    return cli_install.executeInstall(
        service,
        document.value.version_id,
        document.value.fabric_loader,
        document.value.neoforge_version,
        document.value.forge_version,
        path,
        options.source orelse document.value.source,
        options.store_directory,
        options.os,
        options.arch,
        options.workers,
        options.retries,
        options.java_paths.items,
        stdout,
        stderr,
    );
}

fn parseInstanceInstallOptions(allocator: std.mem.Allocator, args: []const []const u8) !InstanceInstallOptions {
    if (args.len == 0) return error.InvalidArguments;
    try jmcl.instance_store.validateId(args[0]);
    var options: InstanceInstallOptions = .{ .id = args[0] };
    errdefer options.deinit(allocator);
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
        } else if (std.mem.eql(u8, argument, "--java")) {
            try options.java_paths.append(allocator, value);
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
    return cli_launch.executeLaunch(
        service,
        document.value.version_id,
        document.value.fabric_loader,
        document.value.neoforge_version,
        document.value.forge_version,
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

test "instance create options pin version and source" {
    const options = try parseInstanceCreateOptions(&.{
        "vanilla-26.1",
        "--version",
        "26.1",
        "--name",
        "Vanilla 26.1",
        "--fabric-loader",
        "0.16.10",
        "--source",
        "bmclapi",
    });
    try std.testing.expectEqualStrings("26.1", options.version_id.?);
    try std.testing.expectEqualStrings("Vanilla 26.1", options.name.?);
    try std.testing.expectEqualStrings("0.16.10", options.fabric_loader.?);
    try std.testing.expectEqual(jmcl.DownloadSource.bmclapi, options.source);
    try std.testing.expect(options.validate);
}

test "instance create accepts --no-validate without a value" {
    const options = try parseInstanceCreateOptions(&.{
        "vanilla-26.1",
        "--version",
        "26.1",
        "--no-validate",
    });
    try std.testing.expect(!options.validate);
    const trailing = try parseInstanceCreateOptions(&.{
        "vanilla-26.1",
        "--no-validate",
        "--version",
        "26.1",
    });
    try std.testing.expect(!trailing.validate);
    try std.testing.expectEqualStrings("26.1", trailing.version_id.?);
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
    const allocator = std.testing.allocator;
    var options = try parseInstanceInstallOptions(allocator, &.{ "vanilla", "--workers", "4", "--arch", "arm64", "--store-directory", "cache", "--source", "bmclapi" });
    defer options.deinit(allocator);
    try std.testing.expectEqual(@as(u8, 4), options.workers);
    try std.testing.expectEqual(jmcl.install_plan.Architecture.arm64, options.arch.?);
    try std.testing.expectEqualStrings("cache", options.store_directory);
    try std.testing.expectEqual(jmcl.DownloadSource.bmclapi, options.source.?);
    try std.testing.expectError(error.InvalidArguments, parseInstanceInstallOptions(allocator, &.{ "vanilla", "--workers", "0" }));
}

test "instance launch requires offline name" {
    try std.testing.expectError(
        error.InvalidArguments,
        parseInstanceLaunchOptions(std.testing.allocator, &.{"vanilla"}),
    );
}
