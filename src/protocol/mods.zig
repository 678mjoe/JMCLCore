const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");
const install = @import("install.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;

const ModsListParams = struct {
    directory: []const u8,
};

const ModsInstallParams = struct {
    directory: []const u8,
    minecraft_version: []const u8,
    loader: []const u8,
    project: []const u8,
    version_id: ?[]const u8 = null,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

const ModsRemoveParams = struct {
    directory: []const u8,
    project: ?[]const u8 = null,
    file: ?[]const u8 = null,
};

const ModsAdoptParams = struct {
    directory: []const u8,
    store_directory: []const u8 = core.artifact_store.default_directory,
};

pub fn writeListResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "mods.list requires a directory");
        return;
    };
    var params = std.json.parseFromValue(ModsListParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "mods.list parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "mods.list directory must not be empty");
        return;
    }
    const minecraft = core.installer.openRoot(service.io, params.value.directory) catch {
        try writeError(writer, request_id, "INSTALL_ROOT_ERROR", "Failed to open the install root");
        return;
    };
    defer minecraft.close(service.io);
    var list = service.listInstanceContent(minecraft) catch {
        try writeError(writer, request_id, "MODS_LIST_FAILED", "Failed to read the instance content registry");
        return;
    };
    defer list.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("mods.list");
    try json.objectField("entries");
    try json.write(list.document.value.entries);
    try json.objectField("unmanaged");
    try json.write(list.unmanaged);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeInstallResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "mods.install requires directory, minecraft_version, loader, and project");
        return;
    };
    var params = std.json.parseFromValue(ModsInstallParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "mods.install parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    const loader = core.modrinth.Loader.parse(input.loader);
    if (input.directory.len == 0 or input.minecraft_version.len == 0 or input.project.len == 0 or
        loader == null or input.workers == 0 or input.workers > 32 or input.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "mods.install parameters are out of range");
        return;
    }

    const minecraft = core.installer.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "INSTALL_ROOT_ERROR", "Failed to open the install root");
        return;
    };
    defer minecraft.close(service.io);
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
    var summary = service.installModrinthProject(
        minecraft,
        store,
        input.minecraft_version,
        loader.?,
        input.project,
        input.version_id,
        .{ .workers = input.workers, .retries = input.retries },
        .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
    ) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        switch (err) {
            error.InvalidModrinthProject, error.InvalidModrinthVersionId, error.InvalidGameVersion => try writeError(writer, request_id, "INVALID_PARAMS", "Project, version, or Minecraft version identifiers are invalid"),
            error.NoCompatibleModVersion => try writeError(writer, request_id, "NO_COMPATIBLE_VERSION", "No release of the project supports the requested Minecraft version and loader"),
            error.IncompatibleModVersion => try writeError(writer, request_id, "INCOMPATIBLE_VERSION", "The requested version does not support the Minecraft version and loader"),
            error.ContentFileConflict => try writeError(writer, request_id, "CONTENT_CONFLICT", "A different registered mod already occupies the target file"),
            error.UnsuccessfulHttpStatus => try writeError(writer, request_id, "HTTP_STATUS_ERROR", "The Modrinth API returned an unsuccessful HTTP status"),
            error.ModrinthResponseTooLarge => try writeError(writer, request_id, "RESPONSE_TOO_LARGE", "The Modrinth API response exceeds the size limit"),
            else => try writeError(writer, request_id, "MODS_INSTALL_FAILED", "Failed to install the Modrinth project"),
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
    try json.write("mods.install");
    try json.objectField("installed");
    try json.write(summary.installed);
    try json.objectField("skipped");
    try json.write(summary.skipped);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeRemoveResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "mods.remove requires a directory and a selector");
        return;
    };
    var params = std.json.parseFromValue(ModsRemoveParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "mods.remove parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or (input.project == null) == (input.file == null)) {
        try writeError(writer, request_id, "INVALID_PARAMS", "mods.remove requires a directory and exactly one of project or file");
        return;
    }
    const minecraft = core.installer.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "INSTALL_ROOT_ERROR", "Failed to open the install root");
        return;
    };
    defer minecraft.close(service.io);

    var removal = service.removeInstanceContent(minecraft, input.project, input.file) catch |err| {
        switch (err) {
            error.ContentNotFound => try writeError(writer, request_id, "CONTENT_NOT_FOUND", "No registered mod matches the given selector"),
            error.InvalidModrinthProject, error.UnsafePath, error.InvalidContentFile => try writeError(writer, request_id, "INVALID_PARAMS", "The selector is invalid"),
            else => try writeError(writer, request_id, "MODS_REMOVE_FAILED", "Failed to remove the mod"),
        }
        return;
    };
    defer removal.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("mods.remove");
    try json.objectField("removed");
    try json.write(removal.entry);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeAdoptResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "mods.adopt requires a directory");
        return;
    };
    var params = std.json.parseFromValue(ModsAdoptParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "mods.adopt parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "mods.adopt directory must not be empty");
        return;
    }
    const minecraft = core.installer.openRoot(service.io, params.value.directory) catch {
        try writeError(writer, request_id, "INSTALL_ROOT_ERROR", "Failed to open the install root");
        return;
    };
    defer minecraft.close(service.io);
    const store = core.artifact_store.openRoot(service.io, params.value.store_directory) catch {
        try writeError(writer, request_id, "STORE_ROOT_ERROR", "Failed to open the artifact store");
        return;
    };
    defer store.close(service.io);

    var adoption = service.adoptInstanceContent(minecraft, store) catch {
        try writeError(writer, request_id, "MODS_ADOPT_FAILED", "Failed to adopt unmanaged mods");
        return;
    };
    defer adoption.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("mods.adopt");
    try json.objectField("adopted");
    try json.write(adoption.adopted);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}
