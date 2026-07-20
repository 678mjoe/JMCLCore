const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");
const install = @import("install.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;

const ModpackInstallParams = struct {
    directory: []const u8,
    id: []const u8,
    file: []const u8,
    api_key: ?[]const u8 = null,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

pub fn writeInstallResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "modpack install requires directory, id, and file");
        return;
    };
    var params = std.json.parseFromValue(ModpackInstallParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "modpack install parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.file.len == 0 or
        input.workers == 0 or input.workers > 32 or input.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "modpack install parameters are out of range");
        return;
    }
    core.instance_store.validateId(input.id) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "the instance id is invalid");
        return;
    };
    if (input.api_key) |key| {
        core.curseforge.validateApiKey(key) catch {
            try writeError(writer, request_id, "INVALID_PARAMS", "the curseforge api_key is invalid");
            return;
        };
    }

    const root = core.instance_store.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "INSTANCES_ROOT_ERROR", "Failed to open the instances root");
        return;
    };
    defer root.close(service.io);
    const store = core.artifact_store.openRoot(service.io, input.store_directory) catch {
        try writeError(writer, request_id, "STORE_ROOT_ERROR", "Failed to open the artifact store");
        return;
    };
    defer store.close(service.io);

    var reporter_context: install.ProtocolInstallReporter = .{
        .io = service.io,
        .writer = writer,
        .request_id = request_id,
        .source = .official,
    };
    var summary = service.installModpack(
        root,
        store,
        input.id,
        input.file,
        input.api_key,
        .{ .workers = input.workers, .retries = input.retries },
        .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
    ) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        switch (err) {
            error.InstanceAlreadyExists => try writeError(writer, request_id, "INSTANCE_EXISTS", "An instance with this id already exists"),
            error.UnknownModpackFormat => try writeError(writer, request_id, "UNKNOWN_MODPACK_FORMAT", "The file is not a recognized modpack archive"),
            error.UnsupportedModpackFormat => try writeError(writer, request_id, "UNSUPPORTED_MODPACK_FORMAT", "The modpack format version is not supported"),
            error.UnsupportedModpackLoader => try writeError(writer, request_id, "UNSUPPORTED_LOADER", "The modpack's mod loader is not supported"),
            error.ModpackLoaderConflict => try writeError(writer, request_id, "LOADER_CONFLICT", "The modpack declares conflicting mod loaders"),
            error.CurseforgeApiKeyRequired => try writeError(writer, request_id, "INVALID_PARAMS", "CurseForge modpacks require an api_key supplied by the caller"),
            error.CurseforgeDownloadUnavailable => try writeError(writer, request_id, "DOWNLOAD_UNAVAILABLE", "A project file is not available for third-party downloads"),
            error.UnsupportedProjectType => try writeError(writer, request_id, "UNSUPPORTED_PROJECT_TYPE", "A project in the pack is not a mod, resource pack, or shader pack"),
            error.FileNotFound => try writeError(writer, request_id, "MODPACK_NOT_FOUND", "The modpack file does not exist"),
            error.UnsafePath => try writeError(writer, request_id, "UNSAFE_MODPACK", "The modpack contains unsafe paths"),
            error.UnsuccessfulHttpStatus => try writeError(writer, request_id, "HTTP_STATUS_ERROR", "The content API returned an unsuccessful HTTP status"),
            else => try writeError(writer, request_id, "MODPACK_INSTALL_FAILED", "Failed to install the modpack"),
        }
        return;
    };
    defer summary.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("modpack.install");
    try json.objectField("id");
    try json.write(summary.id);
    try json.objectField("name");
    try json.write(summary.name);
    try json.objectField("version_id");
    try json.write(summary.version_id);
    try json.objectField("loader");
    if (summary.loader_name) |loader_name| {
        try json.beginObject();
        try json.objectField("name");
        try json.write(loader_name);
        try json.objectField("version");
        try json.write(summary.loader_version);
        try json.endObject();
    } else {
        try json.write(null);
    }
    try json.objectField("files_installed");
    try json.write(summary.files_installed);
    try json.objectField("files_skipped");
    try json.write(summary.files_skipped);
    try json.objectField("overrides_written");
    try json.write(summary.overrides_written);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}
