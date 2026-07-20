const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");
const install = @import("install.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;

const ContentListParams = struct {
    directory: []const u8,
};

const ContentInstallParams = struct {
    directory: []const u8,
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

const ContentRemoveParams = struct {
    directory: []const u8,
    project: ?[]const u8 = null,
    file: ?[]const u8 = null,
};

const ContentAdoptParams = struct {
    directory: []const u8,
    store_directory: []const u8 = core.artifact_store.default_directory,
};

fn stageName(buffer: []u8, directory: []const u8, operation: []const u8) []const u8 {
    return std.fmt.bufPrint(buffer, "{s}.{s}", .{ directory, operation }) catch unreachable;
}

pub fn writeListResultWithKind(
    kind: core.content_registry.Kind,
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const command = kind.directory();
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "content listing requires a directory");
        return;
    };
    var params = std.json.parseFromValue(ContentListParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "content listing parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "content listing directory must not be empty");
        return;
    }
    const minecraft = core.installer.openRoot(service.io, params.value.directory) catch {
        try writeError(writer, request_id, "INSTALL_ROOT_ERROR", "Failed to open the install root");
        return;
    };
    defer minecraft.close(service.io);
    var list = service.listInstanceContent(minecraft, kind) catch {
        try writeError(writer, request_id, "CONTENT_LIST_FAILED", "Failed to read the instance content registry");
        return;
    };
    defer list.deinit();

    var stage_buffer: [64]u8 = undefined;
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write(stageName(&stage_buffer, command, "list"));
    try json.objectField("entries");
    try json.write(list.entries);
    try json.objectField("unmanaged");
    try json.write(list.unmanaged);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeInstallResultWithKind(
    kind: core.content_registry.Kind,
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const command = kind.directory();
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "content install requires directory, minecraft_version, and project");
        return;
    };
    var params = std.json.parseFromValue(ContentInstallParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "content install parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.minecraft_version.len == 0 or input.project.len == 0 or
        input.workers == 0 or input.workers > 32 or input.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "content install parameters are out of range");
        return;
    }
    const loader: ?core.modrinth.Loader = switch (kind) {
        .mod => blk: {
            const raw = input.loader orelse {
                try writeError(writer, request_id, "INVALID_PARAMS", "mods.install requires a loader");
                return;
            };
            const parsed = core.modrinth.Loader.parse(raw);
            if (parsed == null or parsed.? == .minecraft) {
                try writeError(writer, request_id, "INVALID_PARAMS", "mods.install requires a fabric, forge, or neoforge loader");
                return;
            }
            break :blk parsed;
        },
        .resourcepack => blk: {
            if (input.loader) |raw| {
                const parsed = core.modrinth.Loader.parse(raw);
                if (parsed != .minecraft) {
                    try writeError(writer, request_id, "INVALID_PARAMS", "resourcepacks.install does not accept a mod loader");
                    return;
                }
            }
            break :blk null;
        },
        .shaderpack => blk: {
            if (input.loader != null) {
                try writeError(writer, request_id, "INVALID_PARAMS", "shaderpacks.install does not accept a loader");
                return;
            }
            break :blk null;
        },
    };

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
    var summary = switch (provider) {
        .modrinth => service.installModrinthContent(
            minecraft,
            store,
            kind,
            input.minecraft_version,
            loader,
            input.project,
            input.version_id,
            .{ .workers = input.workers, .retries = input.retries, .with_dependencies = input.with_dependencies },
            .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
        ),
        .curseforge => service.installCurseforgeContent(
            minecraft,
            store,
            kind,
            input.minecraft_version,
            loader,
            input.api_key.?,
            input.project,
            input.version_id,
            .{ .workers = input.workers, .retries = input.retries, .with_dependencies = input.with_dependencies },
            .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
        ),
        .manual => unreachable,
    } catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        switch (err) {
            error.InvalidModrinthProject, error.InvalidModrinthVersionId, error.InvalidGameVersion, error.InvalidCurseforgeId => try writeError(writer, request_id, "INVALID_PARAMS", "Project, version, or Minecraft version identifiers are invalid"),
            error.NoCompatibleContentVersion => try writeError(writer, request_id, "NO_COMPATIBLE_VERSION", "No release of the project supports the requested Minecraft version and loader"),
            error.IncompatibleContentVersion => try writeError(writer, request_id, "INCOMPATIBLE_VERSION", "The requested version does not support the Minecraft version and loader"),
            error.ContentFileConflict => try writeError(writer, request_id, "CONTENT_CONFLICT", "A different registered content entry already occupies the target file"),
            error.ContentKindMismatch => try writeError(writer, request_id, "CONTENT_KIND_MISMATCH", "The project type does not match the requested content kind"),
            error.UnsupportedProjectType => try writeError(writer, request_id, "UNSUPPORTED_PROJECT_TYPE", "The project type is not supported"),
            error.CurseforgeDownloadUnavailable => try writeError(writer, request_id, "DOWNLOAD_UNAVAILABLE", "The project file is not available for third-party downloads"),
            error.UnsuccessfulHttpStatus => try writeError(writer, request_id, "HTTP_STATUS_ERROR", "The content API returned an unsuccessful HTTP status"),
            error.ModrinthResponseTooLarge, error.CurseforgeResponseTooLarge => try writeError(writer, request_id, "RESPONSE_TOO_LARGE", "The content API response exceeds the size limit"),
            else => try writeError(writer, request_id, "CONTENT_INSTALL_FAILED", "Failed to install the project"),
        }
        return;
    };
    defer summary.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    var stage_buffer: [64]u8 = undefined;
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write(stageName(&stage_buffer, command, "install"));
    try json.objectField("installed");
    try json.write(summary.installed);
    try json.objectField("skipped");
    try json.write(summary.skipped);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

const ContentSetVersionParams = struct {
    directory: []const u8,
    minecraft_version: []const u8,
    loader: ?[]const u8 = null,
    project: []const u8,
    version_id: []const u8,
    api_key: ?[]const u8 = null,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

pub fn writeSetVersionResultWithKind(
    kind: core.content_registry.Kind,
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const command = kind.directory();
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "content version change requires directory, minecraft_version, project, and version_id");
        return;
    };
    var params = std.json.parseFromValue(ContentSetVersionParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "content version change parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.minecraft_version.len == 0 or input.project.len == 0 or
        input.version_id.len == 0 or input.workers == 0 or input.workers > 32 or input.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "content version change parameters are out of range");
        return;
    }
    const loader: ?core.modrinth.Loader = switch (kind) {
        .mod => blk: {
            const raw = input.loader orelse {
                try writeError(writer, request_id, "INVALID_PARAMS", "mods.set-version requires a loader");
                return;
            };
            const parsed = core.modrinth.Loader.parse(raw);
            if (parsed == null or parsed.? == .minecraft) {
                try writeError(writer, request_id, "INVALID_PARAMS", "mods.set-version requires a fabric, forge, or neoforge loader");
                return;
            }
            break :blk parsed;
        },
        .resourcepack => blk: {
            if (input.loader) |raw| {
                const parsed = core.modrinth.Loader.parse(raw);
                if (parsed != .minecraft) {
                    try writeError(writer, request_id, "INVALID_PARAMS", "resourcepacks.set-version does not accept a mod loader");
                    return;
                }
            }
            break :blk null;
        },
        .shaderpack => blk: {
            if (input.loader != null) {
                try writeError(writer, request_id, "INVALID_PARAMS", "shaderpacks.set-version does not accept a loader");
                return;
            }
            break :blk null;
        },
    };
    if (input.api_key) |key| {
        core.curseforge.validateApiKey(key) catch {
            try writeError(writer, request_id, "INVALID_PARAMS", "the curseforge api_key is invalid");
            return;
        };
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
    var change = service.setInstanceContentVersion(
        minecraft,
        store,
        kind,
        input.minecraft_version,
        loader,
        input.project,
        input.version_id,
        input.api_key,
        .{ .workers = input.workers, .retries = input.retries },
        .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
    ) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        switch (err) {
            error.ContentNotFound => try writeError(writer, request_id, "CONTENT_NOT_FOUND", "No registered content matches the given project"),
            error.InvalidModrinthProject, error.InvalidModrinthVersionId, error.InvalidGameVersion, error.InvalidCurseforgeId => try writeError(writer, request_id, "INVALID_PARAMS", "Project, version, or Minecraft version identifiers are invalid"),
            error.IncompatibleContentVersion => try writeError(writer, request_id, "INCOMPATIBLE_VERSION", "The requested version does not support the Minecraft version and loader"),
            error.ModrinthVersionProjectMismatch, error.CurseforgeFileProjectMismatch => try writeError(writer, request_id, "VERSION_PROJECT_MISMATCH", "The requested version does not belong to the registered project"),
            error.ContentFileConflict => try writeError(writer, request_id, "CONTENT_CONFLICT", "A different registered content entry already occupies the target file"),
            error.CurseforgeApiKeyRequired => try writeError(writer, request_id, "INVALID_PARAMS", "Switching a curseforge entry requires an api_key supplied by the caller"),
            error.CurseforgeDownloadUnavailable => try writeError(writer, request_id, "DOWNLOAD_UNAVAILABLE", "The project file is not available for third-party downloads"),
            error.UnsuccessfulHttpStatus => try writeError(writer, request_id, "HTTP_STATUS_ERROR", "The content API returned an unsuccessful HTTP status"),
            else => try writeError(writer, request_id, "CONTENT_SET_VERSION_FAILED", "Failed to change the content version"),
        }
        return;
    };
    defer change.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    var stage_buffer: [64]u8 = undefined;
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write(stageName(&stage_buffer, command, "set-version"));
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

pub fn writeRemoveResultWithKind(
    kind: core.content_registry.Kind,
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const command = kind.directory();
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "content removal requires a directory and a selector");
        return;
    };
    var params = std.json.parseFromValue(ContentRemoveParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "content removal parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or (input.project == null) == (input.file == null)) {
        try writeError(writer, request_id, "INVALID_PARAMS", "content removal requires a directory and exactly one of project or file");
        return;
    }
    const minecraft = core.installer.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "INSTALL_ROOT_ERROR", "Failed to open the install root");
        return;
    };
    defer minecraft.close(service.io);

    var removal = service.removeInstanceContent(minecraft, kind, input.project, input.file) catch |err| {
        switch (err) {
            error.ContentNotFound => try writeError(writer, request_id, "CONTENT_NOT_FOUND", "No registered content matches the given selector"),
            error.InvalidModrinthProject, error.UnsafePath, error.InvalidContentFile => try writeError(writer, request_id, "INVALID_PARAMS", "The selector is invalid"),
            else => try writeError(writer, request_id, "CONTENT_REMOVE_FAILED", "Failed to remove the content entry"),
        }
        return;
    };
    defer removal.deinit();

    var stage_buffer: [64]u8 = undefined;
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write(stageName(&stage_buffer, command, "remove"));
    try json.objectField("removed");
    try json.write(removal.entry);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeAdoptResultWithKind(
    kind: core.content_registry.Kind,
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const command = kind.directory();
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "content adoption requires a directory");
        return;
    };
    var params = std.json.parseFromValue(ContentAdoptParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "content adoption parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "content adoption directory must not be empty");
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

    var adoption = service.adoptInstanceContent(minecraft, store, kind) catch {
        try writeError(writer, request_id, "CONTENT_ADOPT_FAILED", "Failed to adopt unmanaged content");
        return;
    };
    defer adoption.deinit();

    var stage_buffer: [64]u8 = undefined;
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write(stageName(&stage_buffer, command, "adopt"));
    try json.objectField("adopted");
    try json.write(adoption.adopted);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}
