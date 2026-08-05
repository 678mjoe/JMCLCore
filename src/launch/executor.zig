const std = @import("std");
const DownloadSource = @import("../install/download_source.zig").DownloadSource;
const Service = @import("../service.zig").Service;
const asset_index = @import("../install/asset_index.zig");
const game_process = @import("process.zig");
const hard_link = @import("../install/hard_link.zig");
const install_plan = @import("../install/plan.zig");
const installer = @import("../install/installer.zig");
const java_runtime = @import("../java/java_runtime.zig");
const runtime_installer = @import("../java/runtime_installer.zig");
const java_managed = @import("../java/managed.zig");
const launch_plan = @import("plan.zig");
const preparer = @import("../install/preparer.zig");

pub const Input = struct {
    id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    directory: []const u8 = ".minecraft",
    auth: launch_plan.Auth,
    options: launch_plan.Options = .{},
    source: DownloadSource = .official,
    arch: ?install_plan.Architecture = null,
    java_paths: []const []const u8 = &.{},
    java_policy: java_managed.Policy = .auto,
    java_override: ?[]const u8 = null,
    store_directory: []const u8 = @import("../install/artifact_store.zig").default_directory,
};

pub const StartedEvent = struct {
    pid: u64,
    redacted_argv: []const []const u8,
};

pub const Event = union(enum) {
    started: StartedEvent,
    log: game_process.LogEvent,
    runtime: runtime_installer.Event,
};

pub const Reporter = struct {
    context: *anyopaque,
    callback: *const fn (context: *anyopaque, event: Event) void,

    fn emit(reporter: Reporter, event: Event) void {
        reporter.callback(reporter.context, event);
    }
};

pub const NativeCleanup = enum {
    removed,
    failed,
};

pub const Result = struct {
    process: game_process.Result,
    native_cleanup: NativeCleanup,
};

pub const ExecuteError = error{
    InvalidInput,
    UnsupportedPlatform,
    MetadataFailed,
    AssetIndexFailed,
    InstallPlanFailed,
    DirectoryFailed,
    PrepareFailed,
    InvalidMetadata,
    EnvironmentUnavailable,
    JavaNotFound,
    JavaIncompatible,
    JavaDownloadFailed,
    LaunchPlanFailed,
    ProcessFailed,
    OutOfMemory,
    Canceled,
};

const GameReporter = struct {
    reporter: Reporter,
    redacted_argv: []const []const u8,

    fn callback(raw_context: *anyopaque, event: game_process.Event) void {
        const context: *GameReporter = @ptrCast(@alignCast(raw_context));
        switch (event) {
            .started => |started| context.reporter.emit(.{ .started = .{
                .pid = started.pid,
                .redacted_argv = context.redacted_argv,
            } }),
            .log => |log| context.reporter.emit(.{ .log = log }),
        }
    }
};

const RuntimeReporter = struct {
    reporter: Reporter,

    fn callback(raw_context: *anyopaque, event: runtime_installer.Event) void {
        const context: *RuntimeReporter = @ptrCast(@alignCast(raw_context));
        context.reporter.emit(.{ .runtime = event });
    }
};

pub fn execute(service: *Service, input: Input, reporter: Reporter) ExecuteError!Result {
    if (input.id.len == 0 or input.directory.len == 0 or input.auth.player_name.len == 0 or
        input.auth.uuid.len == 0 or input.auth.access_token.len == 0 or
        input.auth.access_token.len > 16 * 1024)
    {
        return error.InvalidInput;
    }

    const host = install_plan.Target.host() catch return error.UnsupportedPlatform;
    const target: install_plan.Target = .{ .os = host.os, .arch = input.arch orelse host.arch };
    var metadata = service.resolveMetadata(.{
        .id = input.id,
        .fabric_loader = input.fabric_loader,
        .neoforge_version = input.neoforge_version,
        .forge_version = input.forge_version,
    }, input.source) catch return error.MetadataFailed;
    defer metadata.deinit();
    const metadata_value = metadata.value();
    const asset_document: ?std.json.Parsed(asset_index.Index) = if (metadata_value.assetIndex) |descriptor|
        service.fetchAssetIndex(descriptor, input.source) catch return error.AssetIndexFailed
    else
        null;
    defer if (asset_document) |document| document.deinit();
    var install = install_plan.build(
        service.allocator,
        metadata_value.*,
        if (asset_document) |document| document.value else null,
        input.source,
        target,
    ) catch return error.InstallPlanFailed;
    defer install.deinit();

    const root = installer.openRoot(service.io, input.directory) catch return error.DirectoryFailed;
    defer root.close(service.io);
    const directory_absolute = hard_link.absolutePathAlloc(service.allocator, service.io, root, ".") catch return error.DirectoryFailed;
    defer service.allocator.free(directory_absolute);

    var prepared = preparer.prepare(service.allocator, service.io, root, install.value) catch return error.PrepareFailed;
    defer prepared.deinit();
    const native_relative = prepared.native_directory orelse return error.PrepareFailed;
    var cleanup_needed = true;
    defer if (cleanup_needed) root.deleteTree(service.io, native_relative) catch {};
    const native_absolute = hard_link.absolutePathAlloc(service.allocator, service.io, root, native_relative) catch return error.PrepareFailed;
    defer service.allocator.free(native_absolute);

    const required_major = java_runtime.requiredMajor(metadata_value.*) catch return error.InvalidMetadata;
    const environment = service.environ_map orelse return error.EnvironmentUnavailable;
    var runtime_reporter_context: RuntimeReporter = .{ .reporter = reporter };
    const runtime_reporter: ?runtime_installer.Reporter = .{
        .context = &runtime_reporter_context,
        .callback = RuntimeReporter.callback,
    };
    var java = service.resolveGameJava(.{
        .required_major = required_major,
        .component = if (metadata_value.javaVersion) |requirement| requirement.component else null,
        .arch = target.arch,
        .policy = input.java_policy,
        .java_paths = input.java_paths,
        .java_override = input.java_override,
        .store_directory = input.store_directory,
        .allow_download = true,
    }, runtime_reporter) catch |err| switch (err) {
        error.InvalidInput => return error.InvalidInput,
        error.EnvironmentUnavailable => return error.EnvironmentUnavailable,
        error.JavaNotFound => return error.JavaNotFound,
        error.JavaIncompatible => return error.JavaIncompatible,
        error.JavaDownloadFailed => return error.JavaDownloadFailed,
        error.OutOfMemory => return error.OutOfMemory,
        error.Canceled => return error.Canceled,
    };
    defer java.deinit();
    const runtime = java.runtime;
    var launch = launch_plan.build(service.allocator, .{
        .metadata = metadata_value.*,
        .install = install.value,
        .runtime = runtime,
        .root_directory = directory_absolute,
        .game_directory = directory_absolute,
        .natives_directory = native_absolute,
        .auth = input.auth,
        .options = input.options,
        .force_java = java.forced(),
    }) catch return error.LaunchPlanFailed;
    defer launch.deinit();

    var game_reporter: GameReporter = .{
        .reporter = reporter,
        .redacted_argv = launch.value.redacted_argv,
    };
    const secrets = [_][]const u8{
        input.auth.access_token,
        input.auth.session orelse "",
    };
    const process_result = game_process.run(
        service.allocator,
        service.io,
        environment,
        launch.value.argv,
        launch.value.working_directory,
        &secrets,
        .{ .context = &game_reporter, .callback = GameReporter.callback },
    ) catch return error.ProcessFailed;

    const cleanup_status: NativeCleanup = cleanup: {
        root.deleteTree(service.io, native_relative) catch break :cleanup .failed;
        break :cleanup .removed;
    };
    cleanup_needed = false;
    return .{ .process = process_result, .native_cleanup = cleanup_status };
}

test "started adapter includes only redacted arguments" {
    const Collector = struct {
        seen: bool = false,

        fn callback(raw_context: *anyopaque, event: Event) void {
            const context: *@This() = @ptrCast(@alignCast(raw_context));
            switch (event) {
                .started => |started| {
                    context.seen = std.mem.eql(u8, started.redacted_argv[1], "<redacted>");
                },
                .log => {},
                .runtime => {},
            }
        }
    };
    var collector: Collector = .{};
    var adapter: GameReporter = .{
        .reporter = .{ .context = &collector, .callback = Collector.callback },
        .redacted_argv = &.{ "java", "<redacted>" },
    };
    GameReporter.callback(&adapter, .{ .started = .{ .pid = 1 } });
    try std.testing.expect(collector.seen);
}
