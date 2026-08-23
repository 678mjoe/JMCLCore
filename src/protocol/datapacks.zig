const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");
const install = @import("install.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;

const DatapackWorldParams = struct {
    directory: []const u8,
    world: []const u8,
};

const DatapackInstallParams = struct {
    directory: []const u8,
    world: []const u8,
    minecraft_version: []const u8,
    loader: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    api_key: ?[]const u8 = null,
    project: []const u8,
    version_id: ?[]const u8 = null,
    with_dependencies: bool = true,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

const DatapackSetVersionParams = struct {
    directory: []const u8,
    world: []const u8,
    minecraft_version: []const u8,
    loader: ?[]const u8 = null,
    project: []const u8,
    version_id: []const u8,
    api_key: ?[]const u8 = null,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

const DatapackSelectorParams = struct {
    directory: []const u8,
    world: []const u8,
    project: ?[]const u8 = null,
    file: ?[]const u8 = null,
};

const DatapackAdoptParams = struct {
    directory: []const u8,
    world: []const u8,
    store_directory: []const u8 = core.artifact_store.default_directory,
};

fn writeWorldDatapackError(
    writer: *std.Io.Writer,
    request_id: []const u8,
    err: anyerror,
    fallback_code: []const u8,
    fallback_message: []const u8,
) std.Io.Writer.Error!void {
    switch (err) {
        error.WorldNotFound => try writeError(writer, request_id, "WORLD_NOT_FOUND", "No world matches the given name"),
        error.WorldLocked => try writeError(writer, request_id, "WORLD_LOCKED", "The world is in use by a running game"),
        error.ContentNotFound => try writeError(writer, request_id, "CONTENT_NOT_FOUND", "No registered content matches the given selector"),
        error.InvalidModrinthProject, error.InvalidModrinthVersionId, error.InvalidGameVersion, error.InvalidCurseforgeId => try writeError(writer, request_id, "INVALID_PARAMS", "Project, version, or Minecraft version identifiers are invalid"),
        error.NoCompatibleContentVersion => try writeError(writer, request_id, "NO_COMPATIBLE_VERSION", "No release of the project supports the requested Minecraft version"),
        error.IncompatibleContentVersion => try writeError(writer, request_id, "INCOMPATIBLE_VERSION", "The requested version does not support the Minecraft version"),
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

pub fn writeDatapacksListResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack listing requires a directory and world");
        return;
    };
    var params = std.json.parseFromValue(DatapackWorldParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack listing parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.world.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack listing requires a directory and world");
        return;
    }

    var list = service.listWorldDatapacks(params.value.directory, params.value.world) catch |err| {
        try writeWorldDatapackError(writer, request_id, err, "CONTENT_LIST_FAILED", "Failed to read the world datapack registry");
        return;
    };
    defer list.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("datapacks.list");
    try json.objectField("world");
    try json.write(params.value.world);
    try json.objectField("entries");
    try json.write(list.entries);
    try json.objectField("unmanaged");
    try json.write(list.unmanaged);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeDatapacksInstallResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack install requires directory, world, minecraft_version, and project");
        return;
    };
    var params = std.json.parseFromValue(DatapackInstallParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack install parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.world.len == 0 or input.minecraft_version.len == 0 or input.project.len == 0 or
        input.workers == 0 or input.workers > 32 or input.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack install parameters are out of range");
        return;
    }
    if (input.loader != null) {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapacks.install does not accept a loader");
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
        .modrinth => service.installWorldModrinthDatapack(
            input.directory,
            input.world,
            store,
            input.minecraft_version,
            input.project,
            input.version_id,
            .{ .workers = input.workers, .retries = input.retries, .with_dependencies = input.with_dependencies },
            .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
        ),
        .curseforge => service.installWorldCurseforgeDatapack(
            input.directory,
            input.world,
            store,
            input.minecraft_version,
            input.api_key.?,
            input.project,
            input.version_id,
            .{ .workers = input.workers, .retries = input.retries, .with_dependencies = input.with_dependencies },
            .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
        ),
        .manual => unreachable,
    } catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        try writeWorldDatapackError(writer, request_id, err, "CONTENT_INSTALL_FAILED", "Failed to install the datapack");
        return;
    };
    defer summary.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("datapacks.install");
    try json.objectField("world");
    try json.write(input.world);
    try json.objectField("installed");
    try json.write(summary.installed);
    try json.objectField("skipped");
    try json.write(summary.skipped);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeDatapacksSetVersionResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack version change requires directory, world, minecraft_version, project, and version_id");
        return;
    };
    var params = std.json.parseFromValue(DatapackSetVersionParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack version change parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.world.len == 0 or input.minecraft_version.len == 0 or input.project.len == 0 or
        input.version_id.len == 0 or input.workers == 0 or input.workers > 32 or input.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack version change parameters are out of range");
        return;
    }
    if (input.loader != null) {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapacks.set-version does not accept a loader");
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
    var change = service.setWorldDatapackVersion(
        input.directory,
        input.world,
        store,
        input.minecraft_version,
        input.project,
        input.version_id,
        input.api_key,
        .{ .workers = input.workers, .retries = input.retries },
        .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
    ) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        try writeWorldDatapackError(writer, request_id, err, "CONTENT_SET_VERSION_FAILED", "Failed to change the datapack version");
        return;
    };
    defer change.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("datapacks.set-version");
    try json.objectField("world");
    try json.write(input.world);
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

pub fn writeDatapacksSetEnabledResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
    enabled: bool,
) std.Io.Writer.Error!void {
    const stage = if (enabled) "datapacks.enable" else "datapacks.disable";
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack toggle requires a directory, world, and selector");
        return;
    };
    var params = std.json.parseFromValue(DatapackSelectorParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack toggle parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.world.len == 0 or (input.project == null) == (input.file == null)) {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack toggle requires a directory, world, and exactly one of project or file");
        return;
    }

    var toggle = service.setWorldDatapackEnabled(input.directory, input.world, input.project, input.file, enabled) catch |err| {
        try writeWorldDatapackError(writer, request_id, err, "CONTENT_TOGGLE_FAILED", "Failed to toggle the datapack entry");
        return;
    };
    defer toggle.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write(stage);
    try json.objectField("world");
    try json.write(input.world);
    try json.objectField("changed");
    try json.write(toggle.changed);
    try json.objectField("entry");
    try json.write(toggle.entry);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeDatapacksRemoveResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack removal requires a directory, world, and selector");
        return;
    };
    var params = std.json.parseFromValue(DatapackSelectorParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack removal parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.world.len == 0 or (input.project == null) == (input.file == null)) {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack removal requires a directory, world, and exactly one of project or file");
        return;
    }

    var removal = service.removeWorldDatapack(input.directory, input.world, input.project, input.file) catch |err| {
        try writeWorldDatapackError(writer, request_id, err, "CONTENT_REMOVE_FAILED", "Failed to remove the datapack entry");
        return;
    };
    defer removal.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("datapacks.remove");
    try json.objectField("world");
    try json.write(input.world);
    try json.objectField("removed");
    try json.write(removal.entry);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeDatapacksAdoptResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack adoption requires a directory and world");
        return;
    };
    var params = std.json.parseFromValue(DatapackAdoptParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack adoption parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.world.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "datapack adoption requires a directory and world");
        return;
    }

    const store = core.artifact_store.openRoot(service.io, params.value.store_directory) catch {
        try writeError(writer, request_id, "STORE_ROOT_ERROR", "Failed to open the artifact store");
        return;
    };
    defer store.close(service.io);

    var adoption = service.adoptWorldDatapacks(params.value.directory, params.value.world, store) catch |err| {
        try writeWorldDatapackError(writer, request_id, err, "CONTENT_ADOPT_FAILED", "Failed to adopt unmanaged datapacks");
        return;
    };
    defer adoption.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("datapacks.adopt");
    try json.objectField("world");
    try json.write(params.value.world);
    try json.objectField("adopted");
    try json.write(adoption.adopted);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}
