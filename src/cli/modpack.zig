const std = @import("std");
const jmcl = @import("jmcl_core");
const cli_install = @import("install.zig");

pub fn run(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len >= 1 and std.mem.eql(u8, args[0], "install")) return runModpackInstall(service, args[1..], stdout, stderr);
    try stderr.writeAll("usage: jmcl-core instance modpack <install> ID [options]\n");
    return 2;
}

const ModpackInstallOptions = struct {
    id: []const u8,
    file: ?[]const u8 = null,
    api_key: ?[]const u8 = null,
    instances_directory: []const u8 = jmcl.instance_store.default_root,
    store_directory: []const u8 = jmcl.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

fn runModpackInstall(
    service: *jmcl.Service,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    if (args.len == 0) {
        try stderr.writeAll("usage: jmcl-core instance modpack install ID --file PATH [--api-key KEY] [--instances-directory PATH] [--store-directory PATH] [--workers N] [--retries N]\n");
        return 2;
    }
    var options: ModpackInstallOptions = .{ .id = args[0] };
    jmcl.instance_store.validateId(options.id) catch return error.InvalidArguments;
    var index: usize = 1;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return error.InvalidArguments;
        const argument = args[index];
        const value = args[index + 1];
        if (std.mem.eql(u8, argument, "--file")) {
            options.file = value;
        } else if (std.mem.eql(u8, argument, "--api-key")) {
            options.api_key = value;
        } else if (std.mem.eql(u8, argument, "--instances-directory")) {
            options.instances_directory = value;
        } else if (std.mem.eql(u8, argument, "--store-directory")) {
            options.store_directory = value;
        } else if (std.mem.eql(u8, argument, "--workers")) {
            options.workers = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
            if (options.workers == 0 or options.workers > 32) return error.InvalidArguments;
        } else if (std.mem.eql(u8, argument, "--retries")) {
            options.retries = std.fmt.parseInt(u8, value, 10) catch return error.InvalidArguments;
            if (options.retries > 5) return error.InvalidArguments;
        } else return error.InvalidArguments;
        index += 2;
    }
    const file = options.file orelse {
        try stderr.writeAll("instance modpack install requires --file\n");
        return 2;
    };

    const root = jmcl.instance_store.openRoot(service.io, options.instances_directory) catch |err| {
        try stderr.print("failed to open instances directory: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer root.close(service.io);
    const store = jmcl.artifact_store.openRoot(service.io, options.store_directory) catch |err| {
        try stderr.print("failed to open artifact store: {s}\n", .{@errorName(err)});
        return 1;
    };
    defer store.close(service.io);

    var reporter_context: cli_install.CliInstallReporter = .{
        .io = service.io,
        .writer = stderr,
    };
    var summary = service.installModpack(
        root,
        store,
        options.id,
        file,
        options.api_key,
        .{ .workers = options.workers, .retries = options.retries },
        .{ .context = &reporter_context, .callback = cli_install.CliInstallReporter.callback },
    ) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        switch (err) {
            error.InstanceAlreadyExists => try stderr.writeAll("an instance with this id already exists\n"),
            error.UnknownModpackFormat => try stderr.writeAll("the file is not a recognized modpack (no modrinth.index.json or manifest.json)\n"),
            error.UnsupportedModpackFormat => try stderr.writeAll("the modpack format version is not supported\n"),
            error.UnsupportedModpackLoader => try stderr.writeAll("the modpack's mod loader is not supported\n"),
            error.ModpackLoaderConflict => try stderr.writeAll("the modpack declares conflicting mod loaders\n"),
            error.CurseforgeApiKeyRequired => try stderr.writeAll("curseforge modpacks require --api-key (supplied by the embedding application)\n"),
            error.CurseforgeDownloadUnavailable => try stderr.writeAll("a project file is not available for third-party downloads\n"),
            error.UnsupportedProjectType => try stderr.writeAll("a project in the pack is not a mod, resource pack, or shader pack\n"),
            error.FileNotFound => try stderr.writeAll("the modpack file does not exist\n"),
            else => try stderr.print("modpack installation failed: {s}\n", .{@errorName(err)}),
        }
        return 1;
    };
    defer summary.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    try stdout.print("instance\t{s}\t{s}\n", .{ summary.id, summary.name });
    try stdout.print("minecraft\t{s}\n", .{summary.version_id});
    if (summary.loader_name) |loader_name| {
        try stdout.print("loader\t{s}\t{s}\n", .{ loader_name, summary.loader_version orelse "-" });
    } else {
        try stdout.writeAll("loader\tvanilla\n");
    }
    try stdout.print("installed\t{d}\n", .{summary.files_installed});
    try stdout.print("skipped\t{d}\n", .{summary.files_skipped});
    try stdout.print("overrides\t{d}\n", .{summary.overrides_written});
    return 0;
}
