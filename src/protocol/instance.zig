const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;

const InstanceCreateParams = struct {
    directory: []const u8,
    id: []const u8,
    name: ?[]const u8 = null,
    version_id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    source: []const u8 = "official",
};

const InstanceDirectoryParams = struct {
    directory: []const u8,
};

const InstanceTargetParams = struct {
    directory: []const u8,
    id: []const u8,
};

fn parseSource(raw: []const u8) ?core.DownloadSource {
    return core.DownloadSource.parse(raw);
}

fn writeManifestFields(json: *std.json.Stringify, manifest: core.instance_store.Manifest) std.Io.Writer.Error!void {
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
}

pub fn writeCreateResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "instance creation requires directory, id, and version_id");
        return;
    };
    var params = std.json.parseFromValue(InstanceCreateParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "instance creation parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.version_id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "instance creation parameters are out of range");
        return;
    }
    core.instance_store.validateId(input.id) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "the instance id is invalid");
        return;
    };
    const source = parseSource(input.source) orelse {
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

    const root = core.instance_store.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "INSTANCES_ROOT_ERROR", "Failed to open the instances root");
        return;
    };
    defer root.close(service.io);
    const manifest: core.instance_store.Manifest = .{
        .id = input.id,
        .name = input.name orelse input.id,
        .version_id = input.version_id,
        .fabric_loader = input.fabric_loader,
        .neoforge_version = input.neoforge_version,
        .forge_version = input.forge_version,
        .source = source,
    };
    core.instance_store.create(service.io, root, manifest) catch |err| switch (err) {
        error.PathAlreadyExists => {
            try writeError(writer, request_id, "INSTANCE_EXISTS", "An instance with this id already exists");
            return;
        },
        else => {
            try writeError(writer, request_id, "INSTANCE_CREATE_FAILED", "Failed to create the instance");
            return;
        },
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("instance.create");
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
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "instance listing requires a directory");
        return;
    };
    var params = std.json.parseFromValue(InstanceDirectoryParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "instance listing parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "instance listing directory must not be empty");
        return;
    }
    const root = core.instance_store.openRoot(service.io, params.value.directory) catch {
        try writeError(writer, request_id, "INSTANCES_ROOT_ERROR", "Failed to open the instances root");
        return;
    };
    defer root.close(service.io);
    var instances = core.instance_store.list(service.allocator, service.io, root) catch {
        try writeError(writer, request_id, "INSTANCE_LIST_FAILED", "Failed to list instances");
        return;
    };
    defer instances.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("instance.list");
    try json.objectField("instances");
    try json.beginArray();
    for (instances.items) |document| {
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
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "instance lookup requires a directory and an id");
        return;
    };
    var params = std.json.parseFromValue(InstanceTargetParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "instance lookup parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "instance lookup parameters are out of range");
        return;
    }
    const root = core.instance_store.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "INSTANCES_ROOT_ERROR", "Failed to open the instances root");
        return;
    };
    defer root.close(service.io);
    var document = core.instance_store.load(service.allocator, service.io, root, input.id) catch |err| switch (err) {
        error.FileNotFound, error.InvalidInstanceId => {
            try writeError(writer, request_id, "INSTANCE_NOT_FOUND", "No instance matches the given id");
            return;
        },
        else => {
            try writeError(writer, request_id, "INSTANCE_LOAD_FAILED", "Failed to load the instance");
            return;
        },
    };
    defer document.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("instance.get");
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
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "instance deletion requires a directory and an id");
        return;
    };
    var params = std.json.parseFromValue(InstanceTargetParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "instance deletion parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "instance deletion parameters are out of range");
        return;
    }
    const root = core.instance_store.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "INSTANCES_ROOT_ERROR", "Failed to open the instances root");
        return;
    };
    defer root.close(service.io);
    core.instance_store.delete(service.allocator, service.io, root, input.id) catch |err| switch (err) {
        error.FileNotFound, error.InvalidInstanceId => {
            try writeError(writer, request_id, "INSTANCE_NOT_FOUND", "No instance matches the given id");
            return;
        },
        else => {
            try writeError(writer, request_id, "INSTANCE_DELETE_FAILED", "Failed to delete the instance");
            return;
        },
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("instance.delete");
    try json.objectField("id");
    try json.write(input.id);
    try json.objectField("deleted");
    try json.write(true);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}
