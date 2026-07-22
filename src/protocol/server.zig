const std = @import("std");
const builtin = @import("builtin");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;

const ServerCreateParams = struct {
    directory: []const u8,
    id: []const u8,
    name: ?[]const u8 = null,
    version_id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    source: []const u8 = "official",
    accept_eula: bool = false,
    java_path: ?[]const u8 = null,
};

const ServerDirectoryParams = struct {
    directory: []const u8,
};

const ServerTargetParams = struct {
    directory: []const u8,
    id: []const u8,
};

fn requireLinux(writer: *std.Io.Writer, request_id: []const u8) std.Io.Writer.Error!bool {
    if (builtin.os.tag == .linux) return true;
    try writeError(writer, request_id, "UNSUPPORTED_PLATFORM", "Server management is currently supported only on Linux");
    return false;
}

fn writeManifestFields(
    json: *std.json.Stringify,
    manifest: core.server_store.Manifest,
) std.Io.Writer.Error!void {
    try json.objectField("id");
    try json.write(manifest.id);
    try json.objectField("name");
    try json.write(manifest.name);
    try json.objectField("version_id");
    try json.write(manifest.version_id);
    try json.objectField("fabric_loader");
    try json.write(manifest.fabric_loader);
    try json.objectField("neoforge_version");
    try json.write(manifest.neoforge_version);
    try json.objectField("forge_version");
    try json.write(manifest.forge_version);
    try json.objectField("source");
    try json.write(@tagName(manifest.source));
    try json.objectField("accept_eula");
    try json.write(manifest.accept_eula);
    try json.objectField("java_path");
    try json.write(manifest.java_path);
}

pub fn writeCreateResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try requireLinux(writer, request_id)) return;
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "server creation requires directory, id, and version_id");
        return;
    };
    var params = std.json.parseFromValue(ServerCreateParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "server creation parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.version_id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server creation parameters are out of range");
        return;
    }
    core.server_store.validateId(input.id) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "the server id is invalid");
        return;
    };
    const source = core.DownloadSource.parse(input.source) orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "source must be official or bmclapi");
        return;
    };
    const loader_count: u8 = @as(u8, @intFromBool(input.fabric_loader != null)) +
        @intFromBool(input.neoforge_version != null) +
        @intFromBool(input.forge_version != null);
    if (loader_count > 1) {
        try writeError(writer, request_id, "INVALID_PARAMS", "only one loader field may be set");
        return;
    }

    var metadata = service.resolveMetadataPreferred(.{
        .id = input.version_id,
        .fabric_loader = input.fabric_loader,
        .neoforge_version = input.neoforge_version,
        .forge_version = input.forge_version,
    }, source) catch {
        try writeError(writer, request_id, "VERSION_RESOLUTION_FAILED", "Failed to resolve the Minecraft version and loader");
        return;
    };
    metadata.deinit();

    const root = core.server_store.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "SERVERS_ROOT_ERROR", "Failed to open the servers root");
        return;
    };
    defer root.close(service.io);
    const manifest: core.server_store.Manifest = .{
        .id = input.id,
        .name = input.name orelse input.id,
        .version_id = input.version_id,
        .fabric_loader = input.fabric_loader,
        .neoforge_version = input.neoforge_version,
        .forge_version = input.forge_version,
        .source = source,
        .accept_eula = input.accept_eula,
        .java_path = input.java_path,
    };
    core.server_store.create(service.io, root, manifest) catch |err| switch (err) {
        error.PathAlreadyExists => {
            try writeError(writer, request_id, "SERVER_EXISTS", "A server with this id already exists");
            return;
        },
        error.InvalidServerId,
        error.InvalidServerName,
        error.InvalidVersionId,
        error.InvalidFabricLoaderVersion,
        error.InvalidNeoForgeVersion,
        error.InvalidForgeVersion,
        error.InvalidJavaPath,
        error.LoaderConflict,
        => {
            try writeError(writer, request_id, "INVALID_PARAMS", "server creation parameters are out of range");
            return;
        },
        else => {
            try writeError(writer, request_id, "SERVER_CREATE_FAILED", "Failed to create the server");
            return;
        },
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.create");
    try writeManifestFields(&json, manifest);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeListResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try requireLinux(writer, request_id)) return;
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "server listing requires a directory");
        return;
    };
    var params = std.json.parseFromValue(ServerDirectoryParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "server listing parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server listing directory must not be empty");
        return;
    }
    const root = core.server_store.openRoot(service.io, params.value.directory) catch {
        try writeError(writer, request_id, "SERVERS_ROOT_ERROR", "Failed to open the servers root");
        return;
    };
    defer root.close(service.io);
    var servers = core.server_store.list(service.allocator, service.io, root) catch {
        try writeError(writer, request_id, "SERVER_LIST_FAILED", "Failed to list servers");
        return;
    };
    defer servers.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.list");
    try json.objectField("servers");
    try json.beginArray();
    for (servers.items) |document| {
        try json.beginObject();
        try writeManifestFields(&json, document.value);
        try json.endObject();
    }
    try json.endArray();
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeGetResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try requireLinux(writer, request_id)) return;
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "server lookup requires a directory and an id");
        return;
    };
    var params = std.json.parseFromValue(ServerTargetParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "server lookup parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server lookup parameters are out of range");
        return;
    }
    const root = core.server_store.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "SERVERS_ROOT_ERROR", "Failed to open the servers root");
        return;
    };
    defer root.close(service.io);
    var document = core.server_store.load(service.allocator, service.io, root, input.id) catch |err| switch (err) {
        error.FileNotFound, error.InvalidServerId => {
            try writeError(writer, request_id, "SERVER_NOT_FOUND", "No server matches the given id");
            return;
        },
        else => {
            try writeError(writer, request_id, "SERVER_LOAD_FAILED", "Failed to load the server");
            return;
        },
    };
    defer document.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.get");
    try writeManifestFields(&json, document.value);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeDeleteResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try requireLinux(writer, request_id)) return;
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "server deletion requires a directory and an id");
        return;
    };
    var params = std.json.parseFromValue(ServerTargetParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "server deletion parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server deletion parameters are out of range");
        return;
    }
    const root = core.server_store.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "SERVERS_ROOT_ERROR", "Failed to open the servers root");
        return;
    };
    defer root.close(service.io);
    core.server_store.delete(service.allocator, service.io, root, input.id) catch |err| switch (err) {
        error.FileNotFound, error.InvalidServerId => {
            try writeError(writer, request_id, "SERVER_NOT_FOUND", "No server matches the given id");
            return;
        },
        else => {
            try writeError(writer, request_id, "SERVER_DELETE_FAILED", "Failed to delete the server");
            return;
        },
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.delete");
    try json.objectField("id");
    try json.write(input.id);
    try json.objectField("deleted");
    try json.write(true);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}
