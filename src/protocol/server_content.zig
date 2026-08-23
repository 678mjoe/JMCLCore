const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");
const server_protocol = @import("server.zig");
const install = @import("install.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;

const ServerContentTargetParams = struct {
    directory: []const u8,
    id: []const u8,
};

const ServerContentInstallParams = struct {
    directory: []const u8,
    id: []const u8,
    provider: ?[]const u8 = null,
    api_key: ?[]const u8 = null,
    project: []const u8,
    version_id: ?[]const u8 = null,
    with_dependencies: bool = true,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

const ServerContentSetVersionParams = struct {
    directory: []const u8,
    id: []const u8,
    project: []const u8,
    version_id: []const u8,
    api_key: ?[]const u8 = null,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

const ServerContentSelectorParams = struct {
    directory: []const u8,
    id: []const u8,
    project: ?[]const u8 = null,
    file: ?[]const u8 = null,
};

const ServerContentAdoptParams = struct {
    directory: []const u8,
    id: []const u8,
    store_directory: []const u8 = core.artifact_store.default_directory,
};

fn parseParams(
    comptime Params: type,
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
    what: []const u8,
) std.Io.Writer.Error!?std.json.Parsed(Params) {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "missing parameters");
        return null;
    };
    const params = std.json.parseFromValue(Params, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        var message_buffer: [128]u8 = undefined;
        const message = std.fmt.bufPrint(&message_buffer, "{s} parameters are invalid", .{what}) catch "parameters are invalid";
        try writeError(writer, request_id, "INVALID_PARAMS", message);
        return null;
    };
    return params;
}

pub fn writeServerContentError(
    writer: *std.Io.Writer,
    request_id: []const u8,
    err: anyerror,
    fallback_code: []const u8,
    fallback_message: []const u8,
) std.Io.Writer.Error!void {
    switch (err) {
        error.ServerNotFound => try writeError(writer, request_id, "SERVER_NOT_FOUND", "No server matches the given id"),
        error.ServerStatusFailed => try writeError(writer, request_id, "SERVER_STATUS_FAILED", "Failed to validate the server lifecycle status"),
        error.ServerAlreadyRunning => try writeError(writer, request_id, "SERVER_ALREADY_RUNNING", "Stop the server before changing its content"),
        error.ModLoaderRequired => try writeError(writer, request_id, "SERVER_LOADER_REQUIRED", "The server has no mod loader; mods require Fabric, NeoForge, or Forge"),
        error.ContentNotFound => try writeError(writer, request_id, "CONTENT_NOT_FOUND", "No registered content matches the given selector"),
        error.InvalidModrinthProject, error.InvalidModrinthVersionId, error.InvalidGameVersion, error.InvalidCurseforgeId => try writeError(writer, request_id, "INVALID_PARAMS", "Project, version, or Minecraft version identifiers are invalid"),
        error.NoCompatibleContentVersion => try writeError(writer, request_id, "NO_COMPATIBLE_VERSION", "No release of the project supports the server's Minecraft version and loader"),
        error.IncompatibleContentVersion => try writeError(writer, request_id, "INCOMPATIBLE_VERSION", "The requested version does not support the server's Minecraft version and loader"),
        error.ModrinthVersionProjectMismatch, error.CurseforgeFileProjectMismatch => try writeError(writer, request_id, "VERSION_PROJECT_MISMATCH", "The requested version does not belong to the registered project"),
        error.ContentFileConflict => try writeError(writer, request_id, "CONTENT_CONFLICT", "A different registered content entry already occupies the target file"),
        error.ContentKindMismatch => try writeError(writer, request_id, "CONTENT_KIND_MISMATCH", "The project type does not match the requested content kind"),
        error.UnsupportedProjectType => try writeError(writer, request_id, "UNSUPPORTED_PROJECT_TYPE", "The project type is not supported"),
        error.CurseforgeApiKeyRequired => try writeError(writer, request_id, "INVALID_PARAMS", "Switching a curseforge entry requires an api_key supplied by the caller"),
        error.CurseforgeDownloadUnavailable => try writeError(writer, request_id, "DOWNLOAD_UNAVAILABLE", "The project file is not available for third-party downloads"),
        error.UnsuccessfulHttpStatus => try writeError(writer, request_id, "HTTP_STATUS_ERROR", "The content API returned an unsuccessful HTTP status"),
        error.ModrinthResponseTooLarge, error.CurseforgeResponseTooLarge => try writeError(writer, request_id, "RESPONSE_TOO_LARGE", "The content API response exceeds the size limit"),
        error.UnsafePath, error.InvalidContentFile => try writeError(writer, request_id, "INVALID_PARAMS", "The selector is invalid"),
        else => try writeError(writer, request_id, fallback_code, fallback_message),
    }
}

pub fn writeServerModsListResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerContentTargetParams, service, writer, request_id, params_value, "server mods listing") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server mods listing requires directory and id");
        return;
    }

    var list = service.listServerContent(params.value.directory, params.value.id, .mod) catch |err| {
        try writeServerContentError(writer, request_id, err, "CONTENT_LIST_FAILED", "Failed to read the server content registry");
        return;
    };
    defer list.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.mods.list");
    try json.objectField("id");
    try json.write(params.value.id);
    try json.objectField("entries");
    try json.write(list.entries);
    try json.objectField("unmanaged");
    try json.write(list.unmanaged);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeServerModsInstallResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerContentInstallParams, service, writer, request_id, params_value, "server mods install") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.project.len == 0 or
        input.workers == 0 or input.workers > 32 or input.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "server mods install parameters are out of range");
        return;
    }
    const provider: core.content_registry.Provider = blk: {
        const raw = input.provider orelse break :blk .modrinth;
        if (std.mem.eql(u8, raw, "modrinth")) break :blk .modrinth;
        if (std.mem.eql(u8, raw, "curseforge")) break :blk .curseforge;
        try writeError(writer, request_id, "INVALID_PARAMS", "provider must be modrinth or curseforge");
        return;
    };
    switch (provider) {
        .curseforge => {
            const key = input.api_key orelse {
                try writeError(writer, request_id, "INVALID_PARAMS", "curseforge installs require an api_key supplied by the caller");
                return;
            };
            core.curseforge.validateApiKey(key) catch {
                try writeError(writer, request_id, "INVALID_PARAMS", "the curseforge api_key is invalid");
                return;
            };
        },
        .modrinth => {
            if (input.api_key != null) {
                try writeError(writer, request_id, "INVALID_PARAMS", "api_key is only used with the curseforge provider");
                return;
            }
        },
        .manual => unreachable,
    }

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
    var summary = switch (provider) {
        .modrinth => service.installServerModrinthContent(
            input.directory,
            input.id,
            store,
            .mod,
            input.project,
            input.version_id,
            .{ .workers = input.workers, .retries = input.retries, .with_dependencies = input.with_dependencies },
            .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
        ),
        .curseforge => service.installServerCurseforgeContent(
            input.directory,
            input.id,
            store,
            .mod,
            input.api_key.?,
            input.project,
            input.version_id,
            .{ .workers = input.workers, .retries = input.retries, .with_dependencies = input.with_dependencies },
            .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
        ),
        .manual => unreachable,
    } catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        try writeServerContentError(writer, request_id, err, "SERVER_CONTENT_INSTALL_FAILED", "Failed to install the server mod");
        return;
    };
    defer summary.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.mods.install");
    try json.objectField("id");
    try json.write(input.id);
    try json.objectField("installed");
    try json.write(summary.installed);
    try json.objectField("skipped");
    try json.write(summary.skipped);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeServerModsSetVersionResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerContentSetVersionParams, service, writer, request_id, params_value, "server mods version change") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.project.len == 0 or
        input.version_id.len == 0 or input.workers == 0 or input.workers > 32 or input.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "server mods version change parameters are out of range");
        return;
    }
    if (input.api_key) |key| {
        core.curseforge.validateApiKey(key) catch {
            try writeError(writer, request_id, "INVALID_PARAMS", "the curseforge api_key is invalid");
            return;
        };
    }

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
    var change = service.setServerContentVersion(
        input.directory,
        input.id,
        store,
        .mod,
        input.project,
        input.version_id,
        input.api_key,
        .{ .workers = input.workers, .retries = input.retries },
        .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
    ) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        try writeServerContentError(writer, request_id, err, "SERVER_CONTENT_SET_VERSION_FAILED", "Failed to change the server mod version");
        return;
    };
    defer change.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.mods.set-version");
    try json.objectField("id");
    try json.write(input.id);
    try json.objectField("changed");
    try json.write(change.changed);
    try json.objectField("file");
    try json.write(change.file);
    try json.objectField("sha1");
    try json.write(change.sha1);
    try json.objectField("size");
    try json.write(change.size);
    try json.objectField("old_version_id");
    try json.write(change.old_version_id);
    try json.objectField("new_version_id");
    try json.write(change.new_version_id);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeServerModsSetEnabledResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
    enabled: bool,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    const stage = if (enabled) "server.mods.enable" else "server.mods.disable";
    var params = try parseParams(ServerContentSelectorParams, service, writer, request_id, params_value, "server mods toggle") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or (input.project == null) == (input.file == null)) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server mods toggle requires directory, id, and exactly one of project or file");
        return;
    }

    var toggle = service.setServerContentEnabled(input.directory, input.id, .mod, input.project, input.file, enabled) catch |err| {
        try writeServerContentError(writer, request_id, err, "SERVER_CONTENT_TOGGLE_FAILED", "Failed to toggle the server content entry");
        return;
    };
    defer toggle.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write(stage);
    try json.objectField("id");
    try json.write(input.id);
    try json.objectField("changed");
    try json.write(toggle.changed);
    try json.objectField("entry");
    try json.write(toggle.entry);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeServerModsRemoveResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerContentSelectorParams, service, writer, request_id, params_value, "server mods removal") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or (input.project == null) == (input.file == null)) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server mods removal requires directory, id, and exactly one of project or file");
        return;
    }

    var removal = service.removeServerContent(input.directory, input.id, .mod, input.project, input.file) catch |err| {
        try writeServerContentError(writer, request_id, err, "SERVER_CONTENT_REMOVE_FAILED", "Failed to remove the server content entry");
        return;
    };
    defer removal.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.mods.remove");
    try json.objectField("id");
    try json.write(input.id);
    try json.objectField("removed");
    try json.write(removal.entry);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeServerModsAdoptResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerContentAdoptParams, service, writer, request_id, params_value, "server mods adoption") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server mods adoption requires directory and id");
        return;
    }

    const store = core.artifact_store.openRoot(service.io, input.store_directory) catch {
        try writeError(writer, request_id, "STORE_ROOT_ERROR", "Failed to open the artifact store");
        return;
    };
    defer store.close(service.io);

    var adoption = service.adoptServerContent(input.directory, input.id, store, .mod) catch |err| {
        try writeServerContentError(writer, request_id, err, "SERVER_CONTENT_ADOPT_FAILED", "Failed to adopt unmanaged server content");
        return;
    };
    defer adoption.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.mods.adopt");
    try json.objectField("id");
    try json.write(input.id);
    try json.objectField("adopted");
    try json.write(adoption.adopted);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}
