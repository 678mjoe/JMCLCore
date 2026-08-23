const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");
const server_protocol = @import("server.zig");
const server_content = @import("server_content.zig");
const install = @import("install.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;

const ServerDatapackTargetParams = struct {
    directory: []const u8,
    id: []const u8,
};

const ServerDatapackInstallParams = struct {
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

const ServerDatapackSetVersionParams = struct {
    directory: []const u8,
    id: []const u8,
    project: []const u8,
    version_id: []const u8,
    api_key: ?[]const u8 = null,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

const ServerDatapackSelectorParams = struct {
    directory: []const u8,
    id: []const u8,
    project: ?[]const u8 = null,
    file: ?[]const u8 = null,
};

const ServerDatapackAdoptParams = struct {
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

fn writeServerDatapackError(
    writer: *std.Io.Writer,
    request_id: []const u8,
    err: anyerror,
    fallback_code: []const u8,
    fallback_message: []const u8,
) std.Io.Writer.Error!void {
    switch (err) {
        error.InvalidServerProperties => try writeError(writer, request_id, "SERVER_PROPERTIES_INVALID", "server.properties is missing or has no usable level-name"),
        else => try server_content.writeServerContentError(writer, request_id, err, fallback_code, fallback_message),
    }
}

pub fn writeServerDatapacksListResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerDatapackTargetParams, service, writer, request_id, params_value, "server datapacks listing") orelse return;
    defer params.deinit();
    if (params.value.directory.len == 0 or params.value.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server datapacks listing requires directory and id");
        return;
    }

    var list = service.listServerDatapacks(params.value.directory, params.value.id) catch |err| {
        try writeServerDatapackError(writer, request_id, err, "CONTENT_LIST_FAILED", "Failed to read the server datapack registry");
        return;
    };
    defer list.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.datapacks.list");
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

pub fn writeServerDatapacksInstallResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerDatapackInstallParams, service, writer, request_id, params_value, "server datapacks install") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.project.len == 0 or
        input.workers == 0 or input.workers > 32 or input.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "server datapacks install parameters are out of range");
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
        .modrinth => service.installServerModrinthDatapack(
            input.directory,
            input.id,
            store,
            input.project,
            input.version_id,
            .{ .workers = input.workers, .retries = input.retries, .with_dependencies = input.with_dependencies },
            .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
        ),
        .curseforge => service.installServerCurseforgeDatapack(
            input.directory,
            input.id,
            store,
            input.api_key.?,
            input.project,
            input.version_id,
            .{ .workers = input.workers, .retries = input.retries, .with_dependencies = input.with_dependencies },
            .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
        ),
        .manual => unreachable,
    } catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        try writeServerDatapackError(writer, request_id, err, "SERVER_DATAPACKS_INSTALL_FAILED", "Failed to install the server datapack");
        return;
    };
    defer summary.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.datapacks.install");
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

pub fn writeServerDatapacksSetVersionResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerDatapackSetVersionParams, service, writer, request_id, params_value, "server datapacks version change") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.project.len == 0 or
        input.version_id.len == 0 or input.workers == 0 or input.workers > 32 or input.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "server datapacks version change parameters are out of range");
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
    var change = service.setServerDatapackVersion(
        input.directory,
        input.id,
        store,
        input.project,
        input.version_id,
        input.api_key,
        .{ .workers = input.workers, .retries = input.retries },
        .{ .context = &reporter_context, .callback = install.ProtocolInstallReporter.callback },
    ) catch |err| {
        if (reporter_context.output_failed) return error.WriteFailed;
        try writeServerDatapackError(writer, request_id, err, "SERVER_DATAPACKS_SET_VERSION_FAILED", "Failed to change the server datapack version");
        return;
    };
    defer change.deinit();
    if (reporter_context.output_failed) return error.WriteFailed;

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.datapacks.set-version");
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

pub fn writeServerDatapacksSetEnabledResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
    enabled: bool,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    const stage = if (enabled) "server.datapacks.enable" else "server.datapacks.disable";
    var params = try parseParams(ServerDatapackSelectorParams, service, writer, request_id, params_value, "server datapacks toggle") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or (input.project == null) == (input.file == null)) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server datapacks toggle requires directory, id, and exactly one of project or file");
        return;
    }

    var toggle = service.setServerDatapackEnabled(input.directory, input.id, input.project, input.file, enabled) catch |err| {
        try writeServerDatapackError(writer, request_id, err, "SERVER_DATAPACKS_TOGGLE_FAILED", "Failed to toggle the server datapack entry");
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

pub fn writeServerDatapacksRemoveResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerDatapackSelectorParams, service, writer, request_id, params_value, "server datapacks removal") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or (input.project == null) == (input.file == null)) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server datapacks removal requires directory, id, and exactly one of project or file");
        return;
    }

    var removal = service.removeServerDatapack(input.directory, input.id, input.project, input.file) catch |err| {
        try writeServerDatapackError(writer, request_id, err, "SERVER_DATAPACKS_REMOVE_FAILED", "Failed to remove the server datapack entry");
        return;
    };
    defer removal.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.datapacks.remove");
    try json.objectField("id");
    try json.write(input.id);
    try json.objectField("removed");
    try json.write(removal.entry);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeServerDatapacksAdoptResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try server_protocol.requireLinux(writer, request_id)) return;
    var params = try parseParams(ServerDatapackAdoptParams, service, writer, request_id, params_value, "server datapacks adoption") orelse return;
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server datapacks adoption requires directory and id");
        return;
    }

    const store = core.artifact_store.openRoot(service.io, input.store_directory) catch {
        try writeError(writer, request_id, "STORE_ROOT_ERROR", "Failed to open the artifact store");
        return;
    };
    defer store.close(service.io);

    var adoption = service.adoptServerDatapacks(input.directory, input.id, store) catch |err| {
        try writeServerDatapackError(writer, request_id, err, "SERVER_DATAPACKS_ADOPT_FAILED", "Failed to adopt unmanaged server datapacks");
        return;
    };
    defer adoption.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.datapacks.adopt");
    try json.objectField("id");
    try json.write(input.id);
    try json.objectField("adopted");
    try json.write(adoption.adopted);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}
