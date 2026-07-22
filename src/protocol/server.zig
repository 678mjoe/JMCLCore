const std = @import("std");
const builtin = @import("builtin");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");
const install_protocol = @import("install.zig");

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

const ServerInstallParams = struct {
    directory: []const u8,
    id: []const u8,
    accept_eula: bool = false,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
    java_paths: []const []const u8 = &.{},
};

const ServerPropertiesSetParams = struct {
    directory: []const u8,
    id: []const u8,
    properties: std.json.Value,
};

fn requireLinux(writer: *std.Io.Writer, request_id: []const u8) std.Io.Writer.Error!bool {
    if (builtin.os.tag == .linux) return true;
    try writeError(writer, request_id, "UNSUPPORTED_PLATFORM", "Server management is currently supported only on Linux");
    return false;
}

fn installationStatus(
    service: *core.Service,
    root: std.Io.Dir,
    manifest: core.server_store.Manifest,
) !bool {
    const server_dir = core.server_store.openServer(service.io, root, manifest.id) catch |err| switch (err) {
        error.FileNotFound => return false,
        else => |other| return other,
    };
    defer server_dir.close(service.io);
    return core.server_store.isInstalled(service.allocator, service.io, server_dir, manifest);
}

fn writeManifestFields(
    json: *std.json.Stringify,
    manifest: core.server_store.Manifest,
    installed: bool,
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
    try json.objectField("installed");
    try json.write(installed);
}

fn writePropertiesObject(
    json: *std.json.Stringify,
    document: core.server_properties.Document,
) std.Io.Writer.Error!void {
    try json.beginObject();
    for (document.entries.items) |entry| {
        try json.objectField(entry.key);
        try json.write(entry.value);
    }
    try json.endObject();
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
    try writeManifestFields(&json, manifest, false);
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
    const installed = service.allocator.alloc(bool, servers.items.len) catch {
        try writeError(writer, request_id, "SERVER_STATUS_FAILED", "Failed to read server installation status");
        return;
    };
    defer service.allocator.free(installed);
    for (servers.items, installed) |document, *status| {
        status.* = installationStatus(service, root, document.value) catch {
            try writeError(writer, request_id, "SERVER_STATUS_FAILED", "Failed to read server installation status");
            return;
        };
    }

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.list");
    try json.objectField("servers");
    try json.beginArray();
    for (servers.items, installed) |document, status| {
        try json.beginObject();
        try writeManifestFields(&json, document.value, status);
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
    const installed = installationStatus(service, root, document.value) catch {
        try writeError(writer, request_id, "SERVER_STATUS_FAILED", "Failed to read server installation status");
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.get");
    try writeManifestFields(&json, document.value, installed);
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

pub fn writeInstallResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try requireLinux(writer, request_id)) return;
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "server installation requires directory, id, and EULA acceptance");
        return;
    };
    var params = std.json.parseFromValue(ServerInstallParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "server installation parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.store_directory.len == 0 or
        input.workers == 0 or input.workers > 32 or input.retries > 5)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "server installation parameters are out of range");
        return;
    }
    if (!input.accept_eula) {
        try writeError(writer, request_id, "EULA_NOT_ACCEPTED", "Server installation requires explicit Minecraft EULA acceptance");
        return;
    }
    const root = core.server_store.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "SERVERS_ROOT_ERROR", "Failed to open the servers root");
        return;
    };
    defer root.close(service.io);
    var manifest_document = core.server_store.load(service.allocator, service.io, root, input.id) catch |err| switch (err) {
        error.FileNotFound, error.InvalidServerId => {
            try writeError(writer, request_id, "SERVER_NOT_FOUND", "No server matches the given id");
            return;
        },
        else => {
            try writeError(writer, request_id, "SERVER_LOAD_FAILED", "Failed to load the server");
            return;
        },
    };
    defer manifest_document.deinit();
    const manifest = &manifest_document.value;
    if (manifest.fabric_loader != null or manifest.neoforge_version != null or manifest.forge_version != null) {
        try writeError(writer, request_id, "SERVER_LOADER_UNSUPPORTED", "Phase 2 installs vanilla servers only");
        return;
    }
    if (!manifest.accept_eula) {
        manifest.accept_eula = true;
        core.server_store.update(service.io, root, manifest.*) catch {
            try writeError(writer, request_id, "SERVER_STATE_ERROR", "Failed to record Minecraft EULA acceptance");
            return;
        };
    }
    const target = core.install_plan.Target.host() catch {
        try writeError(writer, request_id, "UNSUPPORTED_PLATFORM", "The host platform is not supported");
        return;
    };
    var plan = service.createServerInstallPlan(manifest.version_id, manifest.source, target) catch |err| {
        switch (err) {
            error.ServerDownloadMissing => try writeError(writer, request_id, "SERVER_DOWNLOAD_UNAVAILABLE", "The selected Minecraft version has no dedicated server download"),
            else => try writeError(writer, request_id, "SERVER_PLAN_FAILED", "Failed to create the server installation plan"),
        }
        return;
    };
    defer plan.deinit();
    var explicit_java: [1][]const u8 = undefined;
    const java_paths = if (manifest.java_path) |path| blk: {
        explicit_java[0] = path;
        break :blk explicit_java[0..];
    } else input.java_paths;
    var detection = core.java_runtime.detect(
        service.allocator,
        service.io,
        service.environ_map orelse {
            try writeError(writer, request_id, "ENVIRONMENT_UNAVAILABLE", "Process environment is unavailable");
            return;
        },
        java_paths,
    ) catch {
        try writeError(writer, request_id, "JAVA_NOT_FOUND", "No Java executable candidates were found");
        return;
    };
    defer detection.deinit();
    const runtime = core.java_runtime.selectCompatible(
        detection,
        plan.value.java_major_version,
        target.arch,
    ) catch {
        try writeError(writer, request_id, "JAVA_INCOMPATIBLE", "No Java runtime matches the server requirement");
        return;
    };
    const server_dir = core.server_store.openServer(service.io, root, manifest.id) catch {
        try writeError(writer, request_id, "SERVER_ROOT_ERROR", "Failed to open the server directory");
        return;
    };
    defer server_dir.close(service.io);
    const store = core.artifact_store.openRoot(service.io, input.store_directory) catch {
        try writeError(writer, request_id, "STORE_ROOT_ERROR", "Failed to open the artifact store");
        return;
    };
    defer store.close(service.io);
    try install_protocol.writeInstallStarted(writer, request_id, manifest.source, plan.value);
    try writer.flush();
    var reporter_context: install_protocol.ProtocolInstallReporter = .{
        .io = service.io,
        .writer = writer,
        .request_id = request_id,
        .source = manifest.source,
    };
    const summary = core.installer.execute(
        service.allocator,
        service.io,
        service.environ_map,
        server_dir,
        store,
        plan.value,
        .{ .workers = input.workers, .retries = input.retries },
        .{ .context = &reporter_context, .callback = install_protocol.ProtocolInstallReporter.callback },
    ) catch {
        if (reporter_context.output_failed) return error.WriteFailed;
        try writeError(writer, request_id, "SERVER_INSTALL_FAILED", "Failed to download and materialize the server jar");
        return;
    };
    if (reporter_context.output_failed) return error.WriteFailed;
    core.server_properties.writeEula(service.io, server_dir) catch {
        try writeError(writer, request_id, "SERVER_CONFIGURATION_FAILED", "Failed to write eula.txt");
        return;
    };
    const properties_created = core.server_properties.ensureDefault(service.io, server_dir) catch {
        try writeError(writer, request_id, "SERVER_CONFIGURATION_FAILED", "Failed to initialize server.properties");
        return;
    };
    const server_file = plan.value.files[0];
    core.server_store.recordCompletedInstallation(
        service.io,
        server_dir,
        manifest.*,
        server_file.sha1,
        server_file.size,
    ) catch {
        try writeError(writer, request_id, "SERVER_STATE_ERROR", "Failed to record completed server installation");
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.install");
    try json.objectField("id");
    try json.write(manifest.id);
    try json.objectField("version_id");
    try json.write(manifest.version_id);
    try json.objectField("source");
    try json.write(@tagName(manifest.source));
    try json.objectField("java");
    try json.write(runtime);
    try json.objectField("properties_created");
    try json.write(properties_created);
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
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writePropertiesGetResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try requireLinux(writer, request_id)) return;
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "server properties lookup requires directory and id");
        return;
    };
    var params = std.json.parseFromValue(ServerTargetParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "server properties lookup parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "server properties lookup parameters are out of range");
        return;
    }
    const root = core.server_store.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "SERVERS_ROOT_ERROR", "Failed to open the servers root");
        return;
    };
    defer root.close(service.io);
    var manifest = core.server_store.load(service.allocator, service.io, root, input.id) catch |err| switch (err) {
        error.FileNotFound, error.InvalidServerId => {
            try writeError(writer, request_id, "SERVER_NOT_FOUND", "No server matches the given id");
            return;
        },
        else => {
            try writeError(writer, request_id, "SERVER_LOAD_FAILED", "Failed to load the server");
            return;
        },
    };
    defer manifest.deinit();
    const server_dir = core.server_store.openServer(service.io, root, input.id) catch {
        try writeError(writer, request_id, "SERVER_ROOT_ERROR", "Failed to open the server directory");
        return;
    };
    defer server_dir.close(service.io);
    var properties = core.server_properties.load(service.allocator, service.io, server_dir) catch |err| switch (err) {
        error.FileNotFound => {
            try writeError(writer, request_id, "SERVER_PROPERTIES_NOT_FOUND", "server.properties has not been created");
            return;
        },
        error.InvalidServerProperties,
        error.InvalidServerPropertyKey,
        error.InvalidServerPropertyValue,
        error.TooManyServerProperties,
        error.ServerPropertiesTooLarge,
        => {
            try writeError(writer, request_id, "SERVER_PROPERTIES_INVALID", "server.properties is malformed or out of range");
            return;
        },
        else => {
            try writeError(writer, request_id, "SERVER_PROPERTIES_READ_FAILED", "Failed to read server.properties");
            return;
        },
    };
    defer properties.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.properties.get");
    try json.objectField("id");
    try json.write(input.id);
    try json.objectField("properties");
    try writePropertiesObject(&json, properties);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writePropertiesSetResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    if (!try requireLinux(writer, request_id)) return;
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "server properties update requires directory, id, and properties");
        return;
    };
    var params = std.json.parseFromValue(ServerPropertiesSetParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "server properties update parameters are invalid");
        return;
    };
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.properties != .object or
        input.properties.object.count() == 0 or input.properties.object.count() > core.server_properties.max_entries)
    {
        try writeError(writer, request_id, "INVALID_PARAMS", "server properties update parameters are out of range");
        return;
    }
    const root = core.server_store.openRoot(service.io, input.directory) catch {
        try writeError(writer, request_id, "SERVERS_ROOT_ERROR", "Failed to open the servers root");
        return;
    };
    defer root.close(service.io);
    var manifest = core.server_store.load(service.allocator, service.io, root, input.id) catch |err| switch (err) {
        error.FileNotFound, error.InvalidServerId => {
            try writeError(writer, request_id, "SERVER_NOT_FOUND", "No server matches the given id");
            return;
        },
        else => {
            try writeError(writer, request_id, "SERVER_LOAD_FAILED", "Failed to load the server");
            return;
        },
    };
    defer manifest.deinit();
    const server_dir = core.server_store.openServer(service.io, root, input.id) catch {
        try writeError(writer, request_id, "SERVER_ROOT_ERROR", "Failed to open the server directory");
        return;
    };
    defer server_dir.close(service.io);
    var properties = core.server_properties.load(service.allocator, service.io, server_dir) catch |err| switch (err) {
        error.FileNotFound => {
            try writeError(writer, request_id, "SERVER_PROPERTIES_NOT_FOUND", "server.properties has not been created");
            return;
        },
        error.InvalidServerProperties,
        error.InvalidServerPropertyKey,
        error.InvalidServerPropertyValue,
        error.TooManyServerProperties,
        error.ServerPropertiesTooLarge,
        => {
            try writeError(writer, request_id, "SERVER_PROPERTIES_INVALID", "server.properties is malformed or out of range");
            return;
        },
        else => {
            try writeError(writer, request_id, "SERVER_PROPERTIES_READ_FAILED", "Failed to read server.properties");
            return;
        },
    };
    defer properties.deinit();
    var iterator = input.properties.object.iterator();
    while (iterator.next()) |entry| {
        const property_value = switch (entry.value_ptr.*) {
            .string => |string| string,
            else => {
                try writeError(writer, request_id, "INVALID_PARAMS", "server property values must be strings");
                return;
            },
        };
        properties.set(entry.key_ptr.*, property_value) catch {
            try writeError(writer, request_id, "SERVER_PROPERTIES_INVALID", "A server property key or value is out of range");
            return;
        };
    }
    core.server_properties.writeAtomic(service.io, server_dir, properties) catch {
        try writeError(writer, request_id, "SERVER_PROPERTIES_WRITE_FAILED", "Failed to atomically update server.properties");
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.properties.set");
    try json.objectField("id");
    try json.write(input.id);
    try json.objectField("properties");
    try writePropertiesObject(&json, properties);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}
