const std = @import("std");
const DownloadSource = @import("../install/download_source.zig").DownloadSource;

pub const schema_version: u32 = 1;
pub const default_root = "servers";
const max_manifest_bytes = 64 * 1024;
const max_servers = 4096;
pub const installation_schema_version: u32 = 2;
pub const installation_relative_path = ".jmcl/install.json";
const max_installation_bytes = 16 * 1024;

pub const Manifest = struct {
    schema_version: u32 = schema_version,
    id: []const u8,
    name: []const u8,
    version_id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    source: DownloadSource = .official,
    accept_eula: bool = false,
    java_path: ?[]const u8 = null,
};

pub const Installation = struct {
    schema_version: u32 = installation_schema_version,
    version_id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    source: DownloadSource = .official,
    server_sha1: []const u8,
    server_size: u64,
    java_major_version: u16,
};

pub const Document = std.json.Parsed(Manifest);

pub const List = struct {
    allocator: std.mem.Allocator,
    items: []Document,

    pub fn deinit(self: *List) void {
        for (self.items) |*document| document.deinit();
        self.allocator.free(self.items);
        self.* = undefined;
    }
};

pub fn validateId(id: []const u8) !void {
    if (id.len == 0 or id.len > 64 or !isLowerAlphaNumeric(id[0])) return error.InvalidServerId;
    for (id[1..]) |byte| {
        if (!isLowerAlphaNumeric(byte) and byte != '.' and byte != '_' and byte != '-') {
            return error.InvalidServerId;
        }
    }
    if (id[id.len - 1] == '.') return error.InvalidServerId;
    const stem_end = std.mem.indexOfScalar(u8, id, '.') orelse id.len;
    if (isWindowsReserved(id[0..stem_end])) return error.InvalidServerId;
}

pub fn openRoot(io: std.Io, path: []const u8) !std.Io.Dir {
    if (path.len == 0) return error.EmptyServersRoot;
    return std.Io.Dir.cwd().createDirPathOpen(io, path, .{
        .open_options = .{
            .iterate = true,
            .follow_symlinks = false,
        },
    });
}

pub fn openServer(io: std.Io, root: std.Io.Dir, id: []const u8) !std.Io.Dir {
    try validateId(id);
    return root.openDir(io, id, .{
        .iterate = true,
        .follow_symlinks = false,
    });
}

pub fn create(io: std.Io, root: std.Io.Dir, manifest: Manifest) !void {
    try validateManifest(manifest);
    try root.createDir(io, manifest.id, .default_dir);
    errdefer root.deleteTree(io, manifest.id) catch {};
    const server_dir = try root.openDir(io, manifest.id, .{ .follow_symlinks = false });
    defer server_dir.close(io);
    try writeManifest(io, server_dir, manifest);
}

pub fn update(io: std.Io, root: std.Io.Dir, manifest: Manifest) !void {
    try validateManifest(manifest);
    const server_dir = try openServer(io, root, manifest.id);
    defer server_dir.close(io);
    try writeManifest(io, server_dir, manifest);
}

pub fn load(allocator: std.mem.Allocator, io: std.Io, root: std.Io.Dir, id: []const u8) !Document {
    try validateId(id);
    const server_dir = try root.openDir(io, id, .{ .follow_symlinks = false });
    defer server_dir.close(io);
    const bytes = server_dir.readFileAlloc(
        io,
        "server.json",
        allocator,
        .limited(max_manifest_bytes + 1),
    ) catch |err| switch (err) {
        error.StreamTooLong => return error.ServerManifestTooLarge,
        else => |other| return other,
    };
    defer allocator.free(bytes);
    if (bytes.len > max_manifest_bytes) return error.ServerManifestTooLarge;
    var document = try std.json.parseFromSlice(Manifest, allocator, bytes, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_manifest_bytes,
    });
    errdefer document.deinit();
    try validateManifest(document.value);
    if (document.value.schema_version != schema_version) return error.UnsupportedServerSchema;
    if (!std.mem.eql(u8, document.value.id, id)) return error.ServerIdMismatch;
    return document;
}

pub fn list(allocator: std.mem.Allocator, io: std.Io, root: std.Io.Dir) !List {
    var documents: std.ArrayList(Document) = .empty;
    errdefer {
        for (documents.items) |*document| document.deinit();
        documents.deinit(allocator);
    }
    var iterator = root.iterate();
    while (try iterator.next(io)) |entry| {
        switch (entry.kind) {
            .directory, .unknown => {},
            else => continue,
        }
        validateId(entry.name) catch continue;
        if (documents.items.len == max_servers) return error.TooManyServers;
        var document = load(allocator, io, root, entry.name) catch |err| switch (err) {
            error.OutOfMemory => return error.OutOfMemory,
            else => continue,
        };
        documents.append(allocator, document) catch |err| {
            document.deinit();
            return err;
        };
    }
    std.sort.heap(Document, documents.items, {}, lessThanDocument);
    return .{
        .allocator = allocator,
        .items = try documents.toOwnedSlice(allocator),
    };
}

pub fn delete(allocator: std.mem.Allocator, io: std.Io, root: std.Io.Dir, id: []const u8) !void {
    var document = try load(allocator, io, root, id);
    document.deinit();
    try root.deleteTree(io, id);
}

pub fn recordCompletedInstallation(
    io: std.Io,
    server_dir: std.Io.Dir,
    manifest: Manifest,
    server_sha1: []const u8,
    server_size: u64,
    java_major_version: u16,
) !void {
    const installation: Installation = .{
        .version_id = manifest.version_id,
        .fabric_loader = manifest.fabric_loader,
        .neoforge_version = manifest.neoforge_version,
        .forge_version = manifest.forge_version,
        .source = manifest.source,
        .server_sha1 = server_sha1,
        .server_size = server_size,
        .java_major_version = java_major_version,
    };
    try validateInstallation(installation);
    var atomic_file = try server_dir.createFileAtomic(io, installation_relative_path, .{
        .make_path = true,
        .replace = true,
    });
    defer atomic_file.deinit(io);
    var buffer: [4096]u8 = undefined;
    var writer: std.Io.File.Writer = .init(atomic_file.file, io, &buffer);
    var json: std.json.Stringify = .{ .writer = &writer.interface };
    try json.write(installation);
    try writer.interface.writeByte('\n');
    try writer.interface.flush();
    try atomic_file.file.sync(io);
    try atomic_file.replace(io);
}

pub fn isInstalled(
    allocator: std.mem.Allocator,
    io: std.Io,
    server_dir: std.Io.Dir,
    manifest: Manifest,
) !bool {
    return (try installedJavaMajor(allocator, io, server_dir, manifest)) != null;
}

pub fn installedJavaMajor(
    allocator: std.mem.Allocator,
    io: std.Io,
    server_dir: std.Io.Dir,
    manifest: Manifest,
) !?u16 {
    const bytes = server_dir.readFileAlloc(
        io,
        installation_relative_path,
        allocator,
        .limited(max_installation_bytes + 1),
    ) catch |err| switch (err) {
        error.FileNotFound, error.StreamTooLong => return null,
        else => |other| return other,
    };
    defer allocator.free(bytes);
    if (bytes.len > max_installation_bytes) return null;
    var document = std.json.parseFromSlice(Installation, allocator, bytes, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_installation_bytes,
    }) catch return null;
    defer document.deinit();
    validateInstallation(document.value) catch return null;
    const installation = document.value;
    if (!std.mem.eql(u8, installation.version_id, manifest.version_id) or
        !optionalEql(installation.fabric_loader, manifest.fabric_loader) or
        !optionalEql(installation.neoforge_version, manifest.neoforge_version) or
        !optionalEql(installation.forge_version, manifest.forge_version) or
        installation.source != manifest.source)
    {
        return null;
    }
    const stat = server_dir.statFile(io, "server.jar", .{}) catch |err| switch (err) {
        error.FileNotFound => return null,
        else => |other| return other,
    };
    if (stat.kind != .file or stat.size != installation.server_size) return null;
    return installation.java_major_version;
}

fn writeManifest(io: std.Io, server_dir: std.Io.Dir, manifest: Manifest) !void {
    var atomic_file = try server_dir.createFileAtomic(io, "server.json", .{ .replace = true });
    defer atomic_file.deinit(io);
    var buffer: [4096]u8 = undefined;
    var writer: std.Io.File.Writer = .init(atomic_file.file, io, &buffer);
    var json: std.json.Stringify = .{ .writer = &writer.interface };
    try json.write(manifest);
    try writer.interface.writeByte('\n');
    try writer.interface.flush();
    try atomic_file.file.sync(io);
    try atomic_file.replace(io);
}

fn validateManifest(manifest: Manifest) !void {
    try validateId(manifest.id);
    if (manifest.schema_version != schema_version) return error.UnsupportedServerSchema;
    if (manifest.name.len == 0 or manifest.name.len > 256) return error.InvalidServerName;
    if (manifest.version_id.len == 0 or manifest.version_id.len > 256) return error.InvalidVersionId;
    if (manifest.fabric_loader) |loader| {
        if (loader.len == 0 or loader.len > 256) return error.InvalidFabricLoaderVersion;
    }
    if (manifest.neoforge_version) |version| {
        if (version.len == 0 or version.len > 256) return error.InvalidNeoForgeVersion;
    }
    if (manifest.forge_version) |version| {
        if (version.len == 0 or version.len > 256) return error.InvalidForgeVersion;
    }
    const loader_count: u8 = @as(u8, @intFromBool(manifest.fabric_loader != null)) +
        @intFromBool(manifest.neoforge_version != null) +
        @intFromBool(manifest.forge_version != null);
    if (loader_count > 1) return error.LoaderConflict;
    if (manifest.java_path) |path| {
        if (path.len == 0 or path.len > 4096 or std.mem.indexOfScalar(u8, path, 0) != null) {
            return error.InvalidJavaPath;
        }
    }
}

fn validateInstallation(installation: Installation) !void {
    if (installation.schema_version != installation_schema_version) {
        return error.UnsupportedInstallationSchema;
    }
    if (installation.version_id.len == 0 or installation.version_id.len > 256) {
        return error.InvalidVersionId;
    }
    const loader_count: u8 = @as(u8, @intFromBool(installation.fabric_loader != null)) +
        @intFromBool(installation.neoforge_version != null) +
        @intFromBool(installation.forge_version != null);
    if (loader_count > 1) return error.LoaderConflict;
    if (installation.server_size == 0) return error.InvalidServerSize;
    if (installation.server_sha1.len != 40) return error.InvalidServerSha1;
    for (installation.server_sha1) |byte| {
        if (!std.ascii.isHex(byte)) return error.InvalidServerSha1;
    }
    if (installation.java_major_version == 0) return error.InvalidJavaRequirement;
}

fn optionalEql(left: ?[]const u8, right: ?[]const u8) bool {
    if (left == null or right == null) return left == null and right == null;
    return std.mem.eql(u8, left.?, right.?);
}

fn isLowerAlphaNumeric(byte: u8) bool {
    return (byte >= 'a' and byte <= 'z') or (byte >= '0' and byte <= '9');
}

fn isWindowsReserved(stem: []const u8) bool {
    if (std.mem.eql(u8, stem, "con") or std.mem.eql(u8, stem, "prn") or
        std.mem.eql(u8, stem, "aux") or std.mem.eql(u8, stem, "nul")) return true;
    if (stem.len != 4) return false;
    const prefix = stem[0..3];
    return (std.mem.eql(u8, prefix, "com") or std.mem.eql(u8, prefix, "lpt")) and
        stem[3] >= '1' and stem[3] <= '9';
}

fn lessThanDocument(_: void, left: Document, right: Document) bool {
    return std.mem.lessThan(u8, left.value.id, right.value.id);
}
test "validates portable server ids" {
    try validateId("vanilla-1.21.4");
    try validateId("fabric_server");
    for ([_][]const u8{ "", ".", "..", "Upper", "has space", "../escape", "con", "com1", "name." }) |id| {
        try std.testing.expectError(error.InvalidServerId, validateId(id));
    }
}

test "creates lists loads and deletes server manifests" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try temporary.dir.createDir(io, "servers", .default_dir);
    const root = try temporary.dir.openDir(io, "servers", .{ .iterate = true });
    defer root.close(io);

    try create(io, root, .{
        .id = "fabric-server",
        .name = "Fabric Server",
        .version_id = "1.21.4",
        .fabric_loader = "0.16.10",
        .source = .bmclapi,
        .accept_eula = true,
        .java_path = "/usr/bin/java",
    });
    var document = try load(std.testing.allocator, io, root, "fabric-server");
    defer document.deinit();
    try std.testing.expectEqualStrings("1.21.4", document.value.version_id);
    try std.testing.expectEqualStrings("0.16.10", document.value.fabric_loader.?);
    try std.testing.expectEqual(DownloadSource.bmclapi, document.value.source);
    try std.testing.expect(document.value.accept_eula);
    try std.testing.expectEqualStrings("/usr/bin/java", document.value.java_path.?);

    var servers = try list(std.testing.allocator, io, root);
    defer servers.deinit();
    try std.testing.expectEqual(@as(usize, 1), servers.items.len);
    try std.testing.expectEqualStrings("fabric-server", servers.items[0].value.id);

    try delete(std.testing.allocator, io, root, "fabric-server");
    try std.testing.expectError(error.FileNotFound, load(std.testing.allocator, io, root, "fabric-server"));
}

test "rejects conflicting loaders and invalid Java overrides" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();

    try std.testing.expectError(error.LoaderConflict, create(io, temporary.dir, .{
        .id = "conflict",
        .name = "Conflict",
        .version_id = "1.21.4",
        .fabric_loader = "0.16.10",
        .forge_version = "49.2.0",
    }));
    try std.testing.expectError(error.InvalidJavaPath, create(io, temporary.dir, .{
        .id = "invalid-java",
        .name = "Invalid Java",
        .version_id = "1.21.4",
        .java_path = "",
    }));
}

test "installation marker matches only the completed server identity" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    const manifest: Manifest = .{
        .id = "vanilla",
        .name = "Vanilla",
        .version_id = "1.21.4",
        .source = .official,
        .accept_eula = true,
    };
    const server_file = try temporary.dir.createFile(io, "server.jar", .{});
    try server_file.writeStreamingAll(io, "jar");
    server_file.close(io);
    try std.testing.expect(!try isInstalled(
        std.testing.allocator,
        io,
        temporary.dir,
        manifest,
    ));
    try recordCompletedInstallation(
        io,
        temporary.dir,
        manifest,
        "1111111111111111111111111111111111111111",
        3,
        21,
    );
    try std.testing.expect(try isInstalled(
        std.testing.allocator,
        io,
        temporary.dir,
        manifest,
    ));
    try std.testing.expectEqual(@as(?u16, 21), try installedJavaMajor(
        std.testing.allocator,
        io,
        temporary.dir,
        manifest,
    ));
    var changed = manifest;
    changed.version_id = "1.21.5";
    try std.testing.expect(!try isInstalled(
        std.testing.allocator,
        io,
        temporary.dir,
        changed,
    ));
    changed = manifest;
    changed.source = .bmclapi;
    try std.testing.expect(!try isInstalled(
        std.testing.allocator,
        io,
        temporary.dir,
        changed,
    ));
    try temporary.dir.deleteFile(io, "server.jar");
    try std.testing.expect(!try isInstalled(
        std.testing.allocator,
        io,
        temporary.dir,
        manifest,
    ));
}
