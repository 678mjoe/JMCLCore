const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;
const writeSourceField = protocol.writeSourceField;
const java = @import("java.zig");

const LaunchPlanParams = struct {
    id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    directory: []const u8 = ".minecraft",
    natives_directory: []const u8,
    auth: core.launch_plan.Auth,
    options: core.launch_plan.Options = .{},
    source: core.DownloadSource = .official,
    arch: ?core.install_plan.Architecture = null,
    java_paths: []const []const u8 = &.{},
    java_policy: []const u8 = "auto",
    java_override: ?[]const u8 = null,
    store_directory: []const u8 = core.artifact_store.default_directory,
};

const LaunchExecuteParams = struct {
    id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    directory: []const u8 = ".minecraft",
    auth: core.launch_plan.Auth,
    options: core.launch_plan.Options = .{},
    source: core.DownloadSource = .official,
    arch: ?core.install_plan.Architecture = null,
    java_paths: []const []const u8 = &.{},
    java_policy: []const u8 = "auto",
    java_override: ?[]const u8 = null,
    store_directory: []const u8 = core.artifact_store.default_directory,
};

const ProtocolGameReporter = struct {
    writer: *std.Io.Writer,
    request_id: []const u8,
    source: core.DownloadSource,
    version_id: []const u8,
    sequence: u64 = 0,
    output_failed: bool = false,

    fn callback(raw_context: *anyopaque, event: core.launch_executor.Event) void {
        const context: *ProtocolGameReporter = @ptrCast(@alignCast(raw_context));
        if (context.output_failed) return;
        writeGameEvent(context, event) catch {
            context.output_failed = true;
            return;
        };
        context.writer.flush() catch {
            context.output_failed = true;
        };
    }
};

fn writeGameEvent(context: *ProtocolGameReporter, event: core.launch_executor.Event) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = context.writer };
    switch (event) {
        .started => |started| {
            try writeEventHeader(&json, context.request_id, "started");
            try writeSourceField(&json, context.source);
            try json.objectField("started");
            try json.beginObject();
            try json.objectField("stage");
            try json.write("launch");
            try json.objectField("version_id");
            try json.write(context.version_id);
            try json.objectField("pid");
            try json.write(started.pid);
            try json.objectField("redacted_argv");
            try json.write(started.redacted_argv);
            try json.endObject();
        },
        .log => |log| {
            context.sequence += 1;
            try writeEventHeader(&json, context.request_id, @tagName(log.stream));
            try writeSourceField(&json, context.source);
            try json.objectField(@tagName(log.stream));
            try json.beginObject();
            try json.objectField("sequence");
            try json.write(context.sequence);
            try json.objectField("encoding");
            try json.write("base64");
            try json.objectField("end");
            try json.write(@tagName(log.end));
            try json.objectField("data");
            var encoded: [std.base64.standard.Encoder.calcSize(16 * 1024)]u8 = undefined;
            try json.write(std.base64.standard.Encoder.encode(&encoded, log.bytes));
            try json.endObject();
        },
        .runtime => |runtime_event| {
            try java.writeRuntimeEvent(context.writer, context.request_id, runtime_event);
            return;
        },
    }
    try json.endObject();
    try context.writer.writeByte('\n');
}

fn parseJavaPolicy(
    writer: *std.Io.Writer,
    request_id: []const u8,
    value: []const u8,
) std.Io.Writer.Error!?core.Service.JavaPolicy {
    const policy = core.Service.JavaPolicy.parse(value) orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "java_policy must be auto, local, or managed");
        return null;
    };
    return policy;
}

pub fn writeExecuteResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "launch.execute requires version and authentication");
        return;
    };
    var params = std.json.parseFromValue(LaunchExecuteParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "launch.execute parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.id.len == 0 or input.directory.len == 0 or
        input.auth.access_token.len == 0 or input.auth.access_token.len > 16 * 1024)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "launch.execute directory, version, and access token are invalid");
        return;
    }
    const policy = (try parseJavaPolicy(writer, request_id, input.java_policy)) orelse return;
    if (input.java_override != null and input.java_paths.len != 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "java_override is mutually exclusive with java_paths");
        return;
    }

    var reporter_context: ProtocolGameReporter = .{
        .writer = writer,
        .request_id = request_id,
        .source = input.source,
        .version_id = input.id,
    };
    const result = core.launch_executor.execute(
        service,
        .{
            .id = input.id,
            .fabric_loader = input.fabric_loader,
            .neoforge_version = input.neoforge_version,
            .forge_version = input.forge_version,
            .directory = input.directory,
            .auth = input.auth,
            .options = input.options,
            .source = input.source,
            .arch = input.arch,
            .java_paths = input.java_paths,
            .java_policy = policy,
            .java_override = input.java_override,
            .store_directory = input.store_directory,
        },
        .{ .context = &reporter_context, .callback = ProtocolGameReporter.callback },
    ) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        switch (err) {
            error.UnsupportedPlatform => try writeError(writer, request_id, "UNSUPPORTED_PLATFORM", "The host platform is not supported"),
            error.MetadataFailed => try writeError(writer, request_id, "METADATA_ERROR", "Failed to resolve version metadata"),
            error.AssetIndexFailed => try writeError(writer, request_id, "ASSET_INDEX_ERROR", "Failed to resolve the asset index"),
            error.InstallPlanFailed => try writeError(writer, request_id, "INSTALL_PLAN_ERROR", "Failed to build the install plan"),
            error.DirectoryFailed => try writeError(writer, request_id, "GAME_DIRECTORY_ERROR", "Failed to create or resolve the .minecraft directory"),
            error.PrepareFailed => try writeError(writer, request_id, "PREPARE_FAILED", "Downloaded files are missing, invalid, or could not be prepared"),
            error.InvalidMetadata => try writeError(writer, request_id, "INVALID_METADATA", "Version metadata contains an invalid Java requirement"),
            error.EnvironmentUnavailable => try writeError(writer, request_id, "ENVIRONMENT_UNAVAILABLE", "Process environment is unavailable"),
            error.JavaNotFound => try writeError(writer, request_id, "JAVA_NOT_FOUND", "No Java executable candidates were found"),
            error.JavaIncompatible => try writeError(writer, request_id, "JAVA_INCOMPATIBLE", "No Java runtime matches the required version and architecture"),
            error.JavaDownloadFailed => try writeError(writer, request_id, "JAVA_DOWNLOAD_FAILED", "The managed Java runtime download failed"),
            error.LaunchPlanFailed => try writeError(writer, request_id, "LAUNCH_PLAN_ERROR", "Failed to generate launch arguments"),
            error.ProcessFailed => try writeError(writer, request_id, "PROCESS_ERROR", "Failed to start or supervise the game process"),
            else => try writeError(writer, request_id, "LAUNCH_ERROR", "Failed to execute the game launch"),
        }
        return;
    };
    if (reporter_context.output_failed) return error.WriteFailed;

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try writeSourceField(&json, input.source);
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("launch");
    try json.objectField("version_id");
    try json.write(input.id);
    try json.objectField("process");
    try json.write(result.process);
    try json.objectField("native_cleanup");
    try json.write(result.native_cleanup);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writePlanResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "launch.plan requires version, paths, and authentication");
        return;
    };
    var params = std.json.parseFromValue(LaunchPlanParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "launch.plan parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.id.len == 0 or input.directory.len == 0 or input.natives_directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "launch.plan paths and version id must not be empty");
        return;
    }
    const policy = (try parseJavaPolicy(writer, request_id, input.java_policy)) orelse return;
    if (input.java_override != null and input.java_paths.len != 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "java_override is mutually exclusive with java_paths");
        return;
    }

    const host = core.install_plan.Target.host() catch {
        try writeError(writer, request_id, "UNSUPPORTED_PLATFORM", "The host platform is not supported");
        return;
    };
    const target: core.install_plan.Target = .{
        .os = host.os,
        .arch = input.arch orelse host.arch,
    };

    var metadata = service.resolveMetadata(.{
        .id = input.id,
        .fabric_loader = input.fabric_loader,
        .neoforge_version = input.neoforge_version,
        .forge_version = input.forge_version,
    }, input.source) catch |err| {
        switch (err) {
            error.VersionNotFound => try writeError(writer, request_id, "VERSION_NOT_FOUND", "The requested version is not in the selected manifest"),
            else => try writeError(writer, request_id, "METADATA_ERROR", "Failed to resolve version metadata"),
        }
        return;
    };
    defer metadata.deinit();

    const metadata_value = metadata.value();
    const asset_document: ?std.json.Parsed(core.asset_index.Index) = if (metadata_value.assetIndex) |descriptor|
        service.fetchAssetIndex(descriptor, input.source) catch {
            try writeError(writer, request_id, "ASSET_INDEX_ERROR", "Failed to resolve the asset index");
            return;
        }
    else
        null;
    defer if (asset_document) |document| document.deinit();
    var install = core.install_plan.build(
        service.allocator,
        metadata_value.*,
        if (asset_document) |document| document.value else null,
        input.source,
        target,
    ) catch {
        try writeError(writer, request_id, "INSTALL_PLAN_ERROR", "Failed to build the classpath and asset layout");
        return;
    };
    defer install.deinit();

    const required_major = core.java_runtime.requiredMajor(metadata_value.*) catch {
        try writeError(writer, request_id, "INVALID_METADATA", "Version metadata contains an invalid Java requirement");
        return;
    };
    var resolved_java = service.resolveGameJava(.{
        .required_major = required_major,
        .component = if (metadata_value.javaVersion) |requirement| requirement.component else null,
        .arch = target.arch,
        .policy = policy,
        .java_paths = input.java_paths,
        .java_override = input.java_override,
        .store_directory = input.store_directory,
        .allow_download = false,
    }, null) catch |err| {
        switch (err) {
            error.InvalidInput => try writeError(writer, request_id, "INVALID_PARAMS", "launch.plan Java parameters are invalid"),
            error.EnvironmentUnavailable => try writeError(writer, request_id, "ENVIRONMENT_UNAVAILABLE", "Process environment is unavailable"),
            error.JavaNotFound => try writeError(writer, request_id, "JAVA_NOT_FOUND", "No Java executable candidates were found"),
            error.JavaIncompatible => try writeError(writer, request_id, "JAVA_INCOMPATIBLE", "No Java runtime matches the required version and architecture"),
            else => try writeError(writer, request_id, "JAVA_INCOMPATIBLE", "No Java runtime matches the required version and architecture"),
        }
        return;
    };
    defer resolved_java.deinit();
    const runtime = resolved_java.runtime;

    var launch = core.launch_plan.build(service.allocator, .{
        .metadata = metadata_value.*,
        .install = install.value,
        .runtime = runtime,
        .root_directory = input.directory,
        .game_directory = input.directory,
        .natives_directory = input.natives_directory,
        .auth = input.auth,
        .options = input.options,
        .force_java = resolved_java.forced(),
    }) catch |err| {
        switch (err) {
            error.InvalidAuthSession => try writeError(writer, request_id, "INVALID_AUTH", "Authentication fields must not be empty"),
            error.JavaVersionIncompatible, error.JavaArchitectureIncompatible => try writeError(writer, request_id, "JAVA_INCOMPATIBLE", "The selected Java runtime does not match the required version and architecture"),
            error.UnknownPlaceholder, error.UnsupportedRulePattern => try writeError(writer, request_id, "UNSUPPORTED_METADATA", "Version metadata contains an unsupported launch expression"),
            else => try writeError(writer, request_id, "LAUNCH_PLAN_ERROR", "Failed to generate launch arguments"),
        }
        return;
    };
    defer launch.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try writeSourceField(&json, input.source);
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("version_id");
    try json.write(input.id);
    try json.objectField("required_java");
    try json.write(required_major);
    try json.objectField("runtime");
    try json.write(runtime);
    try json.objectField("java_source");
    try json.write(@tagName(resolved_java.source));
    try json.objectField("forced");
    try json.write(resolved_java.forced());
    try json.objectField("launch");
    try writeLaunchSummary(&json, launch.value);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeLaunchSummary(json: *std.json.Stringify, launch: core.launch_plan.Data) std.Io.Writer.Error!void {
    try json.beginObject();
    try json.objectField("java_executable");
    try json.write(launch.java_executable);
    try json.objectField("working_directory");
    try json.write(launch.working_directory);
    try json.objectField("redacted_argv");
    try json.write(launch.redacted_argv);
    try json.objectField("sensitive_argv_indices");
    try json.write(launch.sensitive_argv_indices);
    try json.endObject();
}

test "launch summary never serializes raw arguments" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var json: std.json.Stringify = .{ .writer = &output.writer };
    try writeLaunchSummary(&json, .{
        .java_executable = "java",
        .working_directory = "game",
        .jvm_args = &.{},
        .main_class = "Main",
        .game_args = &.{ "--accessToken", "secret-token" },
        .argv = &.{ "java", "Main", "--accessToken", "secret-token" },
        .redacted_argv = &.{ "java", "Main", "--accessToken", "<redacted>" },
        .sensitive_argv_indices = &.{3},
    });

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "secret-token") == null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "<redacted>") != null);
}
