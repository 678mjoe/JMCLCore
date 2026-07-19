const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;
const writeSourceField = protocol.writeSourceField;

const InstallPlanParams = struct {
    id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    source: core.DownloadSource = .official,
    os: ?core.install_plan.OperatingSystem = null,
    arch: ?core.install_plan.Architecture = null,
};

const InstallExecuteParams = struct {
    id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    java_paths: []const []const u8 = &.{},
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
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    directory: []const u8 = ".minecraft",
    source: core.DownloadSource = .official,
    os: ?core.install_plan.OperatingSystem = null,
    arch: ?core.install_plan.Architecture = null,
};

pub fn writePlanResult(
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

    var plan = service.createInstallPlanForSelection(.{
        .id = params.value.id,
        .fabric_loader = params.value.fabric_loader,
        .neoforge_version = params.value.neoforge_version,
        .forge_version = params.value.forge_version,
    }, params.value.source, target) catch |err| {
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

pub const ProtocolInstallReporter = struct {
    mutex: std.Io.Mutex = .init,
    io: std.Io,
    writer: *std.Io.Writer,
    request_id: []const u8,
    source: core.DownloadSource,
    output_failed: bool = false,

    pub fn callback(raw_context: *anyopaque, event: core.installer.Event) void {
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

pub fn writeExecuteResult(
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

    var plan = service.createInstallPlanForSelection(.{
        .id = params.value.id,
        .fabric_loader = params.value.fabric_loader,
        .neoforge_version = params.value.neoforge_version,
        .forge_version = params.value.forge_version,
    }, params.value.source, target) catch |err| {
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

    var processors_run: usize = 0;
    var patched_client: ?[]const u8 = null;
    if (plan.value.loader == .neoforge) {
        const loader_version = params.value.neoforge_version.?;
        const patched = core.neoforge_executor.run(
            service.allocator,
            service.io,
            service.environ_map,
            root,
            store,
            loader_version,
            plan.value.version_id,
            .{
                .java_paths = params.value.java_paths,
                .required_major = plan.value.java_major_version,
            },
            null,
        ) catch |err| {
            switch (err) {
                error.NoCompatibleJava => try writeError(writer, request_id, "JAVA_INCOMPATIBLE", "No Java runtime is available for the NeoForge processor chain"),
                error.EnvironmentUnavailable => try writeError(writer, request_id, "ENVIRONMENT_UNAVAILABLE", "Process environment is unavailable"),
                else => try writeError(writer, request_id, "PROCESSOR_FAILED", "The NeoForge processor chain failed"),
            }
            return;
        };
        defer service.allocator.free(patched.patched_sha1);
        processors_run = patched.processors_run;
        patched_client = patched.patched_sha1;
    }
    if (plan.value.loader == .forge) {
        const loader_version = params.value.forge_version.?;
        const patched = core.forge_executor.run(
            service.allocator,
            service.io,
            service.environ_map,
            root,
            store,
            params.value.id,
            loader_version,
            plan.value.version_id,
            .{
                .java_paths = params.value.java_paths,
                .required_major = plan.value.java_major_version,
            },
            null,
        ) catch |err| {
            switch (err) {
                error.NoCompatibleJava => try writeError(writer, request_id, "JAVA_INCOMPATIBLE", "No Java runtime is available for the Forge processor chain"),
                error.EnvironmentUnavailable => try writeError(writer, request_id, "ENVIRONMENT_UNAVAILABLE", "Process environment is unavailable"),
                else => try writeError(writer, request_id, "PROCESSOR_FAILED", "The Forge processor chain failed"),
            }
            return;
        };
        defer service.allocator.free(patched.patched_sha1);
        processors_run = patched.processors_run;
        patched_client = patched.patched_sha1;
    }

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
    if (patched_client) |sha1| {
        try json.objectField("processors_run");
        try json.write(processors_run);
        try json.objectField("patched_client");
        try json.write(sha1);
    }
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writePrepareResult(
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
    var plan = service.createInstallPlanForSelection(.{
        .id = params.value.id,
        .fabric_loader = params.value.fabric_loader,
        .neoforge_version = params.value.neoforge_version,
        .forge_version = params.value.forge_version,
    }, params.value.source, target) catch |err| {
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
