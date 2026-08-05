const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;
const writeSourceField = protocol.writeSourceField;

const JavaDetectParams = struct {
    paths: []const []const u8 = &.{},
};

const JavaSelectParams = struct {
    id: []const u8,
    source: core.DownloadSource = .official,
    arch: ?core.install_plan.Architecture = null,
    paths: []const []const u8 = &.{},
};

pub fn writeDetectResult(
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

pub fn writeSelectResult(
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


const JavaRuntimeStoreParams = struct {
    store_directory: []const u8 = core.artifact_store.default_directory,
};

const JavaRuntimeInstallParams = struct {
    component: ?[]const u8 = null,
    major: ?u16 = null,
    provider: []const u8 = "auto",
    platform: ?[]const u8 = null,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

const JavaRuntimeRemoveParams = struct {
    runtime: []const u8,
    store_directory: []const u8 = core.artifact_store.default_directory,
};

fn writeReceiptFields(
    json: *std.json.Stringify,
    name: []const u8,
    receipt: core.java_runtime_store.Receipt,
) std.Io.Writer.Error!void {
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
    try json.objectField("java_home");
    try json.write(receipt.java_home);
    try json.objectField("executable");
    try json.write(receipt.executable);
    try json.objectField("version");
    try json.write(receipt.version);
    try json.objectField("vendor");
    try json.write(receipt.vendor);
    try json.objectField("component");
    try json.write(receipt.component);
    try json.objectField("package_uuid");
    try json.write(receipt.package_uuid);
    try json.objectField("source_url");
    try json.write(receipt.source_url);
    try json.objectField("files_total");
    try json.write(receipt.files_total);
    try json.objectField("bytes_total");
    try json.write(receipt.bytes_total);
    try json.objectField("installed_at_ms");
    try json.write(receipt.installed_at_ms);
}

pub fn writeRuntimeListResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params: JavaRuntimeStoreParams = .{};
    var parsed_params: ?std.json.Parsed(JavaRuntimeStoreParams) = null;
    defer if (parsed_params) |parsed| parsed.deinit();
    if (params_value) |value| {
        parsed_params = std.json.parseFromValue(JavaRuntimeStoreParams, service.allocator, value, .{
            .ignore_unknown_fields = true,
        }) catch {
            try writeError(writer, request_id, "INVALID_PARAMS", "java.runtime.list parameters are invalid");
            return;
        };
        params = parsed_params.?.value;
    }
    if (params.store_directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "store directory must not be empty");
        return;
    }

    var runtimes = service.listManagedRuntimes(params.store_directory) catch |err| {
        try writeRuntimeError(writer, request_id, err);
        return;
    };
    defer runtimes.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("runtimes");
    try json.beginArray();
    for (runtimes.items) |document| {
        try json.beginObject();
        try writeReceiptFields(&json, document.name, document.parsed.value);
        try json.endObject();
    }
    try json.endArray();
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub const ProtocolRuntimeReporter = struct {
    mutex: std.Io.Mutex = .init,
    io: std.Io,
    writer: *std.Io.Writer,
    request_id: []const u8,
    output_failed: bool = false,

    pub fn callback(raw_context: *anyopaque, event: core.runtime_installer.Event) void {
        const context: *ProtocolRuntimeReporter = @ptrCast(@alignCast(raw_context));
        context.mutex.lockUncancelable(context.io);
        defer context.mutex.unlock(context.io);
        if (context.output_failed) return;
        writeRuntimeEvent(context.writer, context.request_id, event) catch {
            context.output_failed = true;
            return;
        };
        context.writer.flush() catch {
            context.output_failed = true;
        };
    }
};

pub fn writeRuntimeEvent(
    writer: *std.Io.Writer,
    request_id: []const u8,
    event: core.runtime_installer.Event,
) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = writer };
    switch (event) {
        .stage => |stage| {
            try writeEventHeader(&json, request_id, "stage");
            try json.objectField("stage");
            try json.write(@tagName(stage));
        },
        .file => |file| {
            try writeEventHeader(&json, request_id, "file");
            try json.objectField("file");
            try json.beginObject();
            try json.objectField("path");
            try json.write(file.path);
            try json.objectField("size");
            try json.write(file.size);
            try json.endObject();
        },
        .retry => |retry| {
            try writeEventHeader(&json, request_id, "retry");
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
            try json.objectField("progress");
            try json.write(progress);
        },
    }
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeRuntimeInstallResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "java.runtime.install requires a component or major version");
        return;
    };
    var params = std.json.parseFromValue(JavaRuntimeInstallParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "java.runtime.install parameters are invalid");
        return;
    };
    defer params.deinit();
    const provider = core.java_managed.ProviderChoice.parse(params.value.provider) orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "provider must be auto, mojang, or zulu");
        return;
    };
    if ((params.value.component == null) == (params.value.major == null) or
        (params.value.component != null and params.value.component.?.len == 0) or
        (params.value.major != null and params.value.major.? == 0) or
        params.value.store_directory.len == 0 or
        params.value.workers == 0 or params.value.workers > 32 or params.value.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "java.runtime.install requires exactly one of component or major, a store directory, and in-range workers/retries");
        return;
    }

    var reporter_context: ProtocolRuntimeReporter = .{
        .io = service.io,
        .writer = writer,
        .request_id = request_id,
    };
    var outcome = service.installManagedRuntime(.{
        .component = params.value.component,
        .major = params.value.major,
        .provider = provider,
        .platform = params.value.platform,
        .store_directory = params.value.store_directory,
        .workers = params.value.workers,
        .retries = params.value.retries,
    }, .{ .context = &reporter_context, .callback = ProtocolRuntimeReporter.callback }) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        try writeRuntimeError(writer, request_id, err);
        return;
    };
    defer outcome.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("already_installed");
    try json.write(outcome.already_installed);
    try json.objectField("runtime");
    try json.beginObject();
    try writeReceiptFields(&json, outcome.runtime_name, outcome.receipt);
    try json.endObject();
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeRuntimeRemoveResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "java.runtime.remove requires a runtime name");
        return;
    };
    var params = std.json.parseFromValue(JavaRuntimeRemoveParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "java.runtime.remove parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.runtime.len == 0 or params.value.store_directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "runtime name and store directory must not be empty");
        return;
    }

    service.removeManagedRuntime(params.value.store_directory, params.value.runtime) catch |err| {
        try writeRuntimeError(writer, request_id, err);
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("removed");
    try json.write(true);
    try json.objectField("runtime");
    try json.write(params.value.runtime);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeRuntimeError(
    writer: *std.Io.Writer,
    request_id: []const u8,
    failure: anyerror,
) std.Io.Writer.Error!void {
    switch (failure) {
        error.InvalidInput, error.InvalidWorkerCount, error.InvalidRetryCount, error.InvalidRuntimeId, error.InvalidRuntimeName, error.InvalidRuntimePlatform => try writeError(writer, request_id, "INVALID_PARAMS", "Managed runtime parameters are invalid"),
        error.UnsupportedPlatform => try writeError(writer, request_id, "UNSUPPORTED_PLATFORM", "The host platform is not supported"),
        error.EnvironmentUnavailable => try writeError(writer, request_id, "ENVIRONMENT_UNAVAILABLE", "Process environment is unavailable"),
        error.RuntimeComponentUnavailable, error.RuntimePackageUnavailable => try writeError(writer, request_id, "JAVA_RUNTIME_UNAVAILABLE", "No managed runtime covers the requested version and platform"),
        error.ManagedRuntimeProbeFailed, error.ManagedRuntimeMajorMismatch, error.JavaHomeNotFound => try writeError(writer, request_id, "JAVA_RUNTIME_INVALID", "The installed managed runtime failed verification"),
        error.RuntimeNotFound => try writeError(writer, request_id, "JAVA_RUNTIME_NOT_FOUND", "The managed runtime was not found"),
        error.MetadataFetchTimeout => try writeError(writer, request_id, "METADATA_TIMEOUT", "Managed runtime metadata fetch timed out"),
        error.DownloadHashMismatch, error.DownloadSizeMismatch, error.RuntimeManifestMismatch, error.UnsuccessfulHttpStatus, error.ResponseTooLarge, error.DownloadFailed => try writeError(writer, request_id, "DOWNLOAD_FAILED", "A managed runtime download could not be verified"),
        error.InvalidRuntimeManifest, error.RuntimeManifestConflict, error.InvalidLinkTarget => try writeError(writer, request_id, "INVALID_METADATA", "Managed runtime metadata is invalid"),
        else => try writeError(writer, request_id, "JAVA_RUNTIME_ERROR", "Managed runtime operation failed"),
    }
}

test "runtime event writer mirrors installer event fields" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try writeRuntimeEvent(&output.writer, "runtime-1", .{ .stage = .downloading });
    try writeRuntimeEvent(&output.writer, "runtime-1", .{ .retry = .{
        .path = "bin/java",
        .attempt = 1,
        .max_attempts = 3,
        .error_name = "DownloadHashMismatch",
    } });
    try writeRuntimeEvent(&output.writer, "runtime-1", .{ .progress = .{
        .files_completed = 1,
        .files_total = 2,
        .bytes_processed = 10,
        .bytes_total = 20,
        .bytes_transferred = 10,
    } });
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"downloading\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"DownloadHashMismatch\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"bytes_processed\":10") != null);
}
