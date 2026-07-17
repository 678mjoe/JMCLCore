const std = @import("std");
const core = @import("root.zig");
const version_manifest = @import("version_manifest.zig");

const Request = struct {
    protocol: u32,
    id: []const u8,
    method: []const u8,
    params: ?std.json.Value = null,
};

const VersionListParams = struct {
    type: ?version_manifest.VersionType = null,
    limit: usize = 20,
    source: core.DownloadSource = .official,
};

const VersionGetParams = struct {
    id: []const u8,
    source: core.DownloadSource = .official,
};

const InstallPlanParams = struct {
    id: []const u8,
    source: core.DownloadSource = .official,
    os: ?core.install_plan.OperatingSystem = null,
    arch: ?core.install_plan.Architecture = null,
};

const InstallExecuteParams = struct {
    id: []const u8,
    directory: []const u8 = ".minecraft",
    store_directory: []const u8 = core.artifact_store.default_directory,
    source: core.DownloadSource = .official,
    os: ?core.install_plan.OperatingSystem = null,
    arch: ?core.install_plan.Architecture = null,
    workers: u8 = 8,
    retries: u8 = 2,
};

const InstallPrepareParams = struct {
    id: []const u8,
    directory: []const u8 = ".minecraft",
    source: core.DownloadSource = .official,
    os: ?core.install_plan.OperatingSystem = null,
    arch: ?core.install_plan.Architecture = null,
};

const JavaDetectParams = struct {
    paths: []const []const u8 = &.{},
};

const JavaSelectParams = struct {
    id: []const u8,
    source: core.DownloadSource = .official,
    arch: ?core.install_plan.Architecture = null,
    paths: []const []const u8 = &.{},
};

const LaunchPlanParams = struct {
    id: []const u8,
    directory: []const u8 = ".minecraft",
    natives_directory: []const u8,
    auth: core.launch_plan.Auth,
    options: core.launch_plan.Options = .{},
    source: core.DownloadSource = .official,
    arch: ?core.install_plan.Architecture = null,
    java_paths: []const []const u8 = &.{},
};

const LaunchExecuteParams = struct {
    id: []const u8,
    directory: []const u8 = ".minecraft",
    auth: core.launch_plan.Auth,
    options: core.launch_plan.Options = .{},
    source: core.DownloadSource = .official,
    arch: ?core.install_plan.Architecture = null,
    java_paths: []const []const u8 = &.{},
};

pub fn handleLine(
    service: *core.Service,
    line: []const u8,
    writer: *std.Io.Writer,
) std.Io.Writer.Error!void {
    var parsed = std.json.parseFromSlice(Request, service.allocator, line, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, null, "INVALID_REQUEST", "Request must be valid JSON with protocol, id, and method fields");
        return;
    };
    defer parsed.deinit();

    const request = parsed.value;
    if (request.id.len == 0) {
        try writeError(writer, null, "INVALID_REQUEST", "Request id must not be empty");
        return;
    }
    if (request.protocol != core.protocol_version) {
        try writeError(writer, request.id, "UNSUPPORTED_PROTOCOL", "Unsupported protocol version");
        return;
    }

    if (std.mem.eql(u8, request.method, "ping")) {
        try writePingResult(writer, request.id);
    } else if (std.mem.eql(u8, request.method, "core.version")) {
        try writeVersionResult(writer, request.id);
    } else if (std.mem.eql(u8, request.method, "version.list")) {
        try writeVersionListResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "version.get")) {
        try writeVersionGetResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "install.plan")) {
        try writeInstallPlanResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "install.execute")) {
        try writeInstallExecuteResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "install.prepare")) {
        try writeInstallPrepareResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "java.detect")) {
        try writeJavaDetectResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "java.select")) {
        try writeJavaSelectResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "launch.plan")) {
        try writeLaunchPlanResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "launch.execute")) {
        try writeLaunchExecuteResult(service, writer, request.id, request.params);
    } else {
        try writeError(writer, request.id, "METHOD_NOT_FOUND", "Unknown RPC method");
    }
}

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
    }
    try json.endObject();
    try context.writer.writeByte('\n');
}

fn writeLaunchExecuteResult(
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
            .directory = input.directory,
            .auth = input.auth,
            .options = input.options,
            .source = input.source,
            .arch = input.arch,
            .java_paths = input.java_paths,
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

fn writeLaunchPlanResult(
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

    const host = core.install_plan.Target.host() catch {
        try writeError(writer, request_id, "UNSUPPORTED_PLATFORM", "The host platform is not supported");
        return;
    };
    const target: core.install_plan.Target = .{
        .os = host.os,
        .arch = input.arch orelse host.arch,
    };

    var metadata = service.fetchVersionMetadata(input.id, input.source) catch |err| {
        switch (err) {
            error.VersionNotFound => try writeError(writer, request_id, "VERSION_NOT_FOUND", "The requested version is not in the selected manifest"),
            else => try writeError(writer, request_id, "METADATA_ERROR", "Failed to resolve version metadata"),
        }
        return;
    };
    defer metadata.deinit();

    const asset_document: ?std.json.Parsed(core.asset_index.Index) = if (metadata.value.assetIndex) |descriptor|
        service.fetchAssetIndex(descriptor, input.source) catch {
            try writeError(writer, request_id, "ASSET_INDEX_ERROR", "Failed to resolve the asset index");
            return;
        }
    else
        null;
    defer if (asset_document) |document| document.deinit();
    var install = core.install_plan.build(
        service.allocator,
        metadata.value,
        if (asset_document) |document| document.value else null,
        input.source,
        target,
    ) catch {
        try writeError(writer, request_id, "INSTALL_PLAN_ERROR", "Failed to build the classpath and asset layout");
        return;
    };
    defer install.deinit();

    const required_major = core.java_runtime.requiredMajor(metadata.value) catch {
        try writeError(writer, request_id, "INVALID_METADATA", "Version metadata contains an invalid Java requirement");
        return;
    };
    var detection = core.java_runtime.detect(
        service.allocator,
        service.io,
        service.environ_map orelse {
            try writeError(writer, request_id, "ENVIRONMENT_UNAVAILABLE", "Process environment is unavailable");
            return;
        },
        input.java_paths,
    ) catch {
        try writeError(writer, request_id, "JAVA_NOT_FOUND", "No Java executable candidates were found");
        return;
    };
    defer detection.deinit();
    const runtime = core.java_runtime.selectCompatible(detection, required_major, target.arch) catch {
        try writeError(writer, request_id, "JAVA_INCOMPATIBLE", "No Java runtime matches the required version and architecture");
        return;
    };

    var launch = core.launch_plan.build(service.allocator, .{
        .metadata = metadata.value,
        .install = install.value,
        .runtime = runtime,
        .root_directory = input.directory,
        .game_directory = input.directory,
        .natives_directory = input.natives_directory,
        .auth = input.auth,
        .options = input.options,
    }) catch |err| {
        switch (err) {
            error.InvalidAuthSession => try writeError(writer, request_id, "INVALID_AUTH", "Authentication fields must not be empty"),
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

fn writeJavaDetectResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params: JavaDetectParams = .{};
    var parsed_params: ?std.json.Parsed(JavaDetectParams) = null;
    defer if (parsed_params) |parsed| parsed.deinit();
    if (params_value) |value| {
        parsed_params = std.json.parseFromValue(JavaDetectParams, service.allocator, value, .{
            .ignore_unknown_fields = true,
        }) catch {
            try writeError(writer, request_id, "INVALID_PARAMS", "java.detect paths must be strings");
            return;
        };
        params = parsed_params.?.value;
    }

    var detection = core.java_runtime.detect(
        service.allocator,
        service.io,
        service.environ_map orelse {
            try writeError(writer, request_id, "ENVIRONMENT_UNAVAILABLE", "Process environment is unavailable");
            return;
        },
        params.paths,
    ) catch |err| {
        switch (err) {
            error.JavaNotFound => try writeError(writer, request_id, "JAVA_NOT_FOUND", "No Java executable candidates were found"),
            else => try writeError(writer, request_id, "JAVA_DETECTION_FAILED", "Failed to discover local Java runtimes"),
        }
        return;
    };
    defer detection.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("runtimes");
    try json.write(detection.runtimes);
    try json.objectField("failures");
    try json.write(detection.failures);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeJavaSelectResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "java.select requires a version id");
        return;
    };
    var params = std.json.parseFromValue(JavaSelectParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "java.select parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "version id must not be empty");
        return;
    }

    var metadata = service.fetchVersionMetadata(params.value.id, params.value.source) catch |err| {
        switch (err) {
            error.VersionNotFound => try writeError(writer, request_id, "VERSION_NOT_FOUND", "The requested version is not in the selected manifest"),
            else => try writeError(writer, request_id, "METADATA_ERROR", "Failed to resolve the Java requirement"),
        }
        return;
    };
    defer metadata.deinit();
    const required_major = core.java_runtime.requiredMajor(metadata.value) catch {
        try writeError(writer, request_id, "INVALID_METADATA", "Version metadata contains an invalid Java requirement");
        return;
    };
    const host = core.install_plan.Target.host() catch {
        try writeError(writer, request_id, "UNSUPPORTED_PLATFORM", "The host platform is not supported");
        return;
    };
    const required_arch = params.value.arch orelse host.arch;

    var detection = core.java_runtime.detect(
        service.allocator,
        service.io,
        service.environ_map orelse {
            try writeError(writer, request_id, "ENVIRONMENT_UNAVAILABLE", "Process environment is unavailable");
            return;
        },
        params.value.paths,
    ) catch {
        try writeError(writer, request_id, "JAVA_NOT_FOUND", "No Java executable candidates were found");
        return;
    };
    defer detection.deinit();
    const runtime = core.java_runtime.selectCompatible(detection, required_major, required_arch) catch {
        try writeError(writer, request_id, "JAVA_INCOMPATIBLE", "No Java runtime matches the required major version and architecture");
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try writeSourceField(&json, params.value.source);
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("version_id");
    try json.write(params.value.id);
    try json.objectField("required_major");
    try json.write(required_major);
    try json.objectField("required_arch");
    try json.write(required_arch);
    try json.objectField("runtime");
    try json.write(runtime);
    try json.objectField("probe_failures");
    try json.write(detection.failures);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeInstallPrepareResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "install.prepare requires an id");
        return;
    };
    var params = std.json.parseFromValue(InstallPrepareParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "install.prepare parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.id.len == 0 or params.value.directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "install.prepare id and directory must not be empty");
        return;
    }

    const host = core.install_plan.Target.host() catch {
        try writeError(writer, request_id, "UNSUPPORTED_PLATFORM", "The host platform is not supported");
        return;
    };
    const target: core.install_plan.Target = .{
        .os = params.value.os orelse host.os,
        .arch = params.value.arch orelse host.arch,
    };
    var plan = service.createInstallPlan(params.value.id, params.value.source, target) catch |err| {
        try writeInstallPlanningError(writer, request_id, err);
        return;
    };
    defer plan.deinit();
    const root = core.installer.openRoot(service.io, params.value.directory) catch {
        try writeError(writer, request_id, "INSTALL_ROOT_ERROR", "Failed to open the install root");
        return;
    };
    defer root.close(service.io);

    var result = core.preparer.prepare(service.allocator, service.io, root, plan.value) catch |err| {
        switch (err) {
            error.CacheFileInvalid, error.FileNotFound => try writeError(writer, request_id, "CACHE_INVALID", "A required downloaded file is missing or invalid"),
            error.UnsafePath, error.UnsafeArchivePath, error.UnsupportedArchiveEntryType, error.ArchiveEntryTypeMismatch => try writeError(writer, request_id, "UNSAFE_ARCHIVE", "A native archive contains an unsafe entry"),
            error.ConflictingNativeEntry => try writeError(writer, request_id, "PREPARE_CONFLICT", "Runtime preparation found conflicting output paths"),
            else => try writeError(writer, request_id, "PREPARE_FAILED", "Failed to prepare runtime files"),
        }
        return;
    };
    defer result.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try writeSourceField(&json, params.value.source);
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("prepare");
    try json.objectField("version_id");
    try json.write(params.value.id);
    try json.objectField("native_directory");
    try json.write(result.native_directory);
    try json.objectField("native_files");
    try json.write(result.native_files);
    try json.objectField("projections_created");
    try json.write(result.projections_created);
    try json.objectField("projections_cached");
    try json.write(result.projections_cached);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

const ProtocolInstallReporter = struct {
    mutex: std.Io.Mutex = .init,
    io: std.Io,
    writer: *std.Io.Writer,
    request_id: []const u8,
    source: core.DownloadSource,
    output_failed: bool = false,

    fn callback(raw_context: *anyopaque, event: core.installer.Event) void {
        const context: *ProtocolInstallReporter = @ptrCast(@alignCast(raw_context));
        context.mutex.lockUncancelable(context.io);
        defer context.mutex.unlock(context.io);
        if (context.output_failed) return;

        writeInstallProgressEvent(
            context.writer,
            context.request_id,
            context.source,
            event,
        ) catch {
            context.output_failed = true;
            return;
        };
        context.writer.flush() catch {
            context.output_failed = true;
        };
    }
};

fn writeInstallExecuteResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "install.execute requires an id");
        return;
    };
    var params = std.json.parseFromValue(InstallExecuteParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "install.execute parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.id.len == 0 or params.value.directory.len == 0 or params.value.store_directory.len == 0 or
        params.value.workers == 0 or params.value.workers > 32 or params.value.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "install.execute parameters are out of range");
        return;
    }

    const host = core.install_plan.Target.host() catch {
        try writeError(writer, request_id, "UNSUPPORTED_PLATFORM", "The host platform is not supported");
        return;
    };
    const target: core.install_plan.Target = .{
        .os = params.value.os orelse host.os,
        .arch = params.value.arch orelse host.arch,
    };

    var plan = service.createInstallPlan(params.value.id, params.value.source, target) catch |err| {
        try writeInstallPlanningError(writer, request_id, err);
        return;
    };
    defer plan.deinit();

    const root = core.installer.openRoot(service.io, params.value.directory) catch {
        try writeError(writer, request_id, "INSTALL_ROOT_ERROR", "Failed to open the install root");
        return;
    };
    defer root.close(service.io);
    const store = core.artifact_store.openRoot(service.io, params.value.store_directory) catch {
        try writeError(writer, request_id, "STORE_ROOT_ERROR", "Failed to open the artifact store");
        return;
    };
    defer store.close(service.io);

    try writeInstallStarted(writer, request_id, params.value.source, plan.value);
    try writer.flush();

    var reporter_context: ProtocolInstallReporter = .{
        .io = service.io,
        .writer = writer,
        .request_id = request_id,
        .source = params.value.source,
    };
    const summary = core.installer.execute(
        service.allocator,
        service.io,
        service.environ_map,
        root,
        store,
        plan.value,
        .{ .workers = params.value.workers, .retries = params.value.retries },
        .{ .context = &reporter_context, .callback = ProtocolInstallReporter.callback },
    ) catch {
        if (reporter_context.output_failed) return error.WriteFailed;
        try writeError(writer, request_id, "DOWNLOAD_FAILED", "A planned file could not be downloaded and verified");
        return;
    };
    if (reporter_context.output_failed) return error.WriteFailed;

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try writeSourceField(&json, params.value.source);
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("download");
    try json.objectField("version_id");
    try json.write(params.value.id);
    try json.objectField("files_total");
    try json.write(summary.files_total);
    try json.objectField("files_downloaded");
    try json.write(summary.files_downloaded);
    try json.objectField("files_cached");
    try json.write(summary.files_cached);
    try json.objectField("files_hard_linked");
    try json.write(summary.files_hard_linked);
    try json.objectField("files_copied");
    try json.write(summary.files_copied);
    try json.objectField("bytes_verified");
    try json.write(summary.bytes_verified);
    try json.objectField("bytes_transferred");
    try json.write(summary.bytes_transferred);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeInstallStarted(
    writer: *std.Io.Writer,
    request_id: []const u8,
    source: core.DownloadSource,
    plan: core.install_plan.Data,
) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "started");
    try writeSourceField(&json, source);
    try json.objectField("started");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("download");
    try json.objectField("version_id");
    try json.write(plan.version_id);
    try json.objectField("files_total");
    try json.write(plan.files.len);
    try json.objectField("bytes_total");
    try json.write(plan.total_download_size);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeInstallProgressEvent(
    writer: *std.Io.Writer,
    request_id: []const u8,
    source: core.DownloadSource,
    event: core.installer.Event,
) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = writer };
    switch (event) {
        .file => |file| {
            try writeEventHeader(&json, request_id, "file");
            try writeSourceField(&json, source);
            try json.objectField("file");
            try json.beginObject();
            try json.objectField("path");
            try json.write(file.path);
            try json.objectField("status");
            try json.write(@tagName(file.status));
            try json.objectField("placement");
            try json.write(@tagName(file.placement));
            try json.objectField("size");
            try json.write(file.size);
            try json.objectField("attempts");
            try json.write(file.attempts);
            try json.endObject();
        },
        .retry => |retry| {
            try writeEventHeader(&json, request_id, "retry");
            try writeSourceField(&json, source);
            try json.objectField("retry");
            try json.beginObject();
            try json.objectField("path");
            try json.write(retry.path);
            try json.objectField("attempt");
            try json.write(retry.attempt);
            try json.objectField("max_attempts");
            try json.write(retry.max_attempts);
            try json.objectField("error");
            try json.write(retry.error_name);
            try json.endObject();
        },
        .progress => |progress| {
            try writeEventHeader(&json, request_id, "progress");
            try writeSourceField(&json, source);
            try json.objectField("progress");
            try json.write(progress);
        },
    }
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeInstallPlanningError(
    writer: *std.Io.Writer,
    request_id: []const u8,
    failure: anyerror,
) std.Io.Writer.Error!void {
    switch (failure) {
        error.VersionNotFound => try writeError(writer, request_id, "VERSION_NOT_FOUND", "The requested version is not in the selected manifest"),
        error.MetadataHashMismatch, error.AssetIndexHashMismatch => try writeError(writer, request_id, "INTEGRITY_ERROR", "Downloaded metadata failed SHA-1 verification"),
        error.UnsuccessfulHttpStatus => try writeError(writer, request_id, "HTTP_STATUS_ERROR", "The selected download source returned an unsuccessful HTTP status"),
        error.UnsafePath, error.UnsafeArchivePath, error.InsecureDownloadUrl => try writeError(writer, request_id, "UNSAFE_METADATA", "Metadata contains an unsafe download destination"),
        else => try writeError(writer, request_id, "INSTALL_PLAN_ERROR", "Failed to create the install plan"),
    }
}

fn writeInstallPlanResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "install.plan requires a version id");
        return;
    };
    var params = std.json.parseFromValue(InstallPlanParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "install.plan parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "version id must not be empty");
        return;
    }

    const host = core.install_plan.Target.host() catch {
        try writeError(writer, request_id, "UNSUPPORTED_PLATFORM", "The host platform is not supported");
        return;
    };
    const target: core.install_plan.Target = .{
        .os = params.value.os orelse host.os,
        .arch = params.value.arch orelse host.arch,
    };

    var plan = service.createInstallPlan(params.value.id, params.value.source, target) catch |err| {
        switch (err) {
            error.VersionNotFound => try writeError(writer, request_id, "VERSION_NOT_FOUND", "The requested version is not in the selected manifest"),
            error.MetadataHashMismatch, error.AssetIndexHashMismatch => try writeError(writer, request_id, "INTEGRITY_ERROR", "Downloaded metadata failed SHA-1 verification"),
            error.ResponseBodyTooLarge => try writeError(writer, request_id, "RESPONSE_TOO_LARGE", "Version metadata exceeds the size limit"),
            error.UnsuccessfulHttpStatus => try writeError(writer, request_id, "HTTP_STATUS_ERROR", "The selected download source returned an unsuccessful HTTP status"),
            error.UnsafePath, error.UnsafeArchivePath, error.InsecureDownloadUrl => try writeError(writer, request_id, "UNSAFE_METADATA", "Metadata contains an unsafe download destination"),
            error.InvalidSha1, error.InvalidAssetHash, error.ConflictingDestination, error.ConflictingProjection, error.MissingLibraryDownloads, error.MissingLibraryArtifact, error.MissingNativeClassifiers, error.MissingNativeClassifier => try writeError(writer, request_id, "INVALID_METADATA", "Metadata cannot produce a consistent install plan"),
            else => try writeError(writer, request_id, "INSTALL_PLAN_ERROR", "Failed to create the install plan"),
        }
        return;
    };
    defer plan.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try writeSourceField(&json, params.value.source);
    try json.objectField("result");
    try json.write(plan.value);
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeVersionGetResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "version.get requires an id");
        return;
    };
    var params = std.json.parseFromValue(VersionGetParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "version.get requires a string id");
        return;
    };
    defer params.deinit();

    if (params.value.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "version id must not be empty");
        return;
    }

    var metadata = service.fetchVersionMetadata(params.value.id, params.value.source) catch |err| {
        switch (err) {
            error.VersionNotFound => try writeError(writer, request_id, "VERSION_NOT_FOUND", "The requested version is not in the Mojang manifest"),
            error.MetadataHashMismatch => try writeError(writer, request_id, "INTEGRITY_ERROR", "Version metadata failed SHA-1 verification"),
            error.MetadataIdMismatch => try writeError(writer, request_id, "INVALID_METADATA", "Version metadata contains an unexpected id"),
            error.ResponseBodyTooLarge => try writeError(writer, request_id, "RESPONSE_TOO_LARGE", "Mojang metadata exceeds the size limit"),
            error.UnsuccessfulHttpStatus => try writeError(writer, request_id, "HTTP_STATUS_ERROR", "The selected download source returned an unsuccessful HTTP status"),
            error.SyntaxError, error.UnexpectedToken, error.UnknownField, error.InvalidEnumTag, error.MissingField => try writeError(writer, request_id, "INVALID_METADATA", "Mojang returned invalid version metadata"),
            else => try writeError(writer, request_id, "NETWORK_ERROR", "Failed to fetch version metadata from the selected source"),
        }
        return;
    };
    defer metadata.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try writeSourceField(&json, params.value.source);
    try json.objectField("result");
    try json.write(metadata.value);
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeVersionDocument(writer: *std.Io.Writer) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = writer };
    try json.beginObject();
    try json.objectField("protocol");
    try json.write(core.protocol_version);
    try json.objectField("name");
    try json.write(core.name);
    try json.objectField("version");
    try json.write(core.version);
    try json.endObject();
    try writer.writeByte('\n');
}

fn writePingResult(writer: *std.Io.Writer, id: []const u8) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("pong");
    try json.write(true);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeVersionResult(writer: *std.Io.Writer, id: []const u8) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("name");
    try json.write(core.name);
    try json.objectField("version");
    try json.write(core.version);
    try json.objectField("protocol");
    try json.write(core.protocol_version);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeVersionListResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params: VersionListParams = .{};
    var parsed_params: ?std.json.Parsed(VersionListParams) = null;
    defer if (parsed_params) |parsed| parsed.deinit();

    if (params_value) |value| {
        parsed_params = std.json.parseFromValue(VersionListParams, service.allocator, value, .{
            .ignore_unknown_fields = true,
        }) catch {
            try writeError(writer, id, "INVALID_PARAMS", "version.list params must contain a valid type and limit");
            return;
        };
        params = parsed_params.?.value;
    }

    if (params.limit == 0 or params.limit > 1000) {
        try writeError(writer, id, "INVALID_PARAMS", "limit must be between 1 and 1000");
        return;
    }

    var manifest = service.fetchVersionManifest(params.source) catch |err| {
        switch (err) {
            error.ResponseBodyTooLarge => try writeError(writer, id, "RESPONSE_TOO_LARGE", "Version manifest exceeds the size limit"),
            error.UnsuccessfulHttpStatus => try writeError(writer, id, "HTTP_STATUS_ERROR", "The selected download source returned an unsuccessful HTTP status"),
            error.SyntaxError, error.UnexpectedToken, error.UnknownField, error.InvalidEnumTag, error.MissingField => try writeError(writer, id, "INVALID_MANIFEST", "Mojang returned an invalid version manifest"),
            else => try writeError(writer, id, "NETWORK_ERROR", "Failed to fetch the version manifest from the selected source"),
        }
        return;
    };
    defer manifest.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, id, "result");
    try writeSourceField(&json, params.source);
    try json.objectField("result");
    try version_manifest.writeSelection(&json, manifest.value, .{
        .kind = params.type,
        .limit = params.limit,
    });
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeError(
    writer: *std.Io.Writer,
    id: ?[]const u8,
    code: []const u8,
    message: []const u8,
) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = writer };
    try json.beginObject();
    try json.objectField("protocol");
    try json.write(core.protocol_version);
    try json.objectField("id");
    try json.write(id);
    try json.objectField("event");
    try json.write("error");
    try json.objectField("error");
    try json.beginObject();
    try json.objectField("code");
    try json.write(code);
    try json.objectField("message");
    try json.write(message);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeEventHeader(json: *std.json.Stringify, id: []const u8, event: []const u8) std.Io.Writer.Error!void {
    try json.beginObject();
    try json.objectField("protocol");
    try json.write(core.protocol_version);
    try json.objectField("id");
    try json.write(id);
    try json.objectField("event");
    try json.write(event);
}

fn writeSourceField(json: *std.json.Stringify, source: core.DownloadSource) std.Io.Writer.Error!void {
    try json.objectField("source");
    try json.write(@tagName(source));
}

test "ping request produces a result event" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-1\",\"method\":\"ping\"}",
        &output.writer,
    );

    try std.testing.expectEqualStrings(
        "{\"protocol\":1,\"id\":\"test-1\",\"event\":\"result\",\"result\":{\"pong\":true}}\n",
        output.written(),
    );
}

test "unknown methods produce a structured error" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-2\",\"method\":\"missing\"}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "METHOD_NOT_FOUND") != null);
}

test "malformed requests do not expose an id" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(&service, "not-json", &output.writer);

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"id\":null") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_REQUEST") != null);
}

test "version list validates its limit before fetching" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-3\",\"method\":\"version.list\",\"params\":{\"limit\":0}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "version get requires a non-empty id before fetching" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-4\",\"method\":\"version.get\",\"params\":{\"id\":\"\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "version list rejects an unknown download source" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-5\",\"method\":\"version.list\",\"params\":{\"source\":\"unknown\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "install plan validates its version id before fetching" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-6\",\"method\":\"install.plan\",\"params\":{\"id\":\"\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "install execute validates its directory before planning" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-7\",\"method\":\"install.execute\",\"params\":{\"id\":\"1.21.11\",\"directory\":\"\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "install prepare validates its directory before planning" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-8\",\"method\":\"install.prepare\",\"params\":{\"id\":\"1.6.4\",\"directory\":\"\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "Java select validates its version id before fetching" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-9\",\"method\":\"java.select\",\"params\":{\"id\":\"\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "launch plan validates required paths before fetching" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-10\",\"method\":\"launch.plan\",\"params\":{\"id\":\"test\",\"directory\":\"\",\"natives_directory\":\"natives\",\"auth\":{\"player_name\":\"p\",\"uuid\":\"u\",\"access_token\":\"t\"}}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "launch execute validates required paths before fetching" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-11\",\"method\":\"launch.execute\",\"params\":{\"id\":\"test\",\"directory\":\"\",\"auth\":{\"player_name\":\"p\",\"uuid\":\"u\",\"access_token\":\"t\"}}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
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
