const std = @import("std");
const DownloadSource = @import("install/download_source.zig").DownloadSource;

pub const schema_version: u32 = 1;
pub const default_root = "instances";
const max_manifest_bytes = 64 * 1024;
pub const installation_schema_version: u32 = 1;
pub const installation_relative_path = ".jmcl/install.json";
const max_installation_bytes = 16 * 1024;
const max_instances = 4096;

pub const Manifest = struct {
    schema_version: u32 = schema_version,
    id: []const u8,
    name: []const u8,
    version_id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    source: DownloadSource = .official,
};

pub const Installation = struct {
    schema_version: u32 = installation_schema_version,
    version_id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    source: DownloadSource = .official,
    os: []const u8,
    arch: []const u8,
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
    if (id.len == 0 or id.len > 64 or !isLowerAlphaNumeric(id[0])) return error.InvalidInstanceId;
    for (id[1..]) |byte| {
        if (!isLowerAlphaNumeric(byte) and byte != '.' and byte != '_' and byte != '-') {
            return error.InvalidInstanceId;
        }
    }
    if (id[id.len - 1] == '.') return error.InvalidInstanceId;
    const stem_end = std.mem.indexOfScalar(u8, id, '.') orelse id.len;
    if (isWindowsReserved(id[0..stem_end])) return error.InvalidInstanceId;
}

pub fn openRoot(io: std.Io, path: []const u8) !std.Io.Dir {
    if (path.len == 0) return error.EmptyInstancesRoot;
    return std.Io.Dir.cwd().createDirPathOpen(io, path, .{
        .open_options = .{
            .iterate = true,
            .follow_symlinks = false,
        },
    });
}

pub fn create(io: std.Io, root: std.Io.Dir, manifest: Manifest) !void {
    try validateManifest(manifest);
    try root.createDir(io, manifest.id, .default_dir);
    errdefer root.deleteTree(io, manifest.id) catch {};
    const instance_dir = try root.openDir(io, manifest.id, .{ .follow_symlinks = false });
    defer instance_dir.close(io);
    try instance_dir.createDir(io, ".minecraft", .default_dir);
    try writeManifest(io, instance_dir, manifest);
}

pub fn load(allocator: std.mem.Allocator, io: std.Io, root: std.Io.Dir, id: []const u8) !Document {
    try validateId(id);
    const instance_dir = try root.openDir(io, id, .{ .follow_symlinks = false });
    defer instance_dir.close(io);
    const bytes = instance_dir.readFileAlloc(
        io,
        "instance.json",
        allocator,
        .limited(max_manifest_bytes + 1),
    ) catch |err| switch (err) {
        error.StreamTooLong => return error.InstanceManifestTooLarge,
        else => |other| return other,
    };
    defer allocator.free(bytes);
    if (bytes.len > max_manifest_bytes) return error.InstanceManifestTooLarge;
    var document = try std.json.parseFromSlice(Manifest, allocator, bytes, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_manifest_bytes,
    });
    errdefer document.deinit();
    try validateManifest(document.value);
    if (document.value.schema_version != schema_version) return error.UnsupportedInstanceSchema;
    if (!std.mem.eql(u8, document.value.id, id)) return error.InstanceIdMismatch;
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
        if (documents.items.len == max_instances) return error.TooManyInstances;
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

pub fn recordInstallation(io: std.Io, minecraft_dir: std.Io.Dir, installation: Installation) !void {
    try validateInstallation(installation);
    var atomic_file = try minecraft_dir.createFileAtomic(io, installation_relative_path, .{
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
    minecraft_dir: std.Io.Dir,
    manifest: Manifest,
) !bool {
    const bytes = minecraft_dir.readFileAlloc(
        io,
        installation_relative_path,
        allocator,
        .limited(max_installation_bytes + 1),
    ) catch |err| switch (err) {
        error.FileNotFound, error.StreamTooLong => return false,
        else => |other| return other,
    };
    defer allocator.free(bytes);
    if (bytes.len > max_installation_bytes) return false;
    var document = std.json.parseFromSlice(Installation, allocator, bytes, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_installation_bytes,
    }) catch return false;
    defer document.deinit();
    validateInstallation(document.value) catch return false;
    const installation = document.value;
    return std.mem.eql(u8, installation.version_id, manifest.version_id) and
        optionalEql(installation.fabric_loader, manifest.fabric_loader) and
        optionalEql(installation.neoforge_version, manifest.neoforge_version) and
        optionalEql(installation.forge_version, manifest.forge_version) and
        installation.source == manifest.source;
}

pub fn minecraftPathAlloc(allocator: std.mem.Allocator, instances_root: []const u8, id: []const u8) ![]u8 {
    try validateId(id);
    return std.fs.path.join(allocator, &.{ instances_root, id, ".minecraft" });
}

fn writeManifest(io: std.Io, instance_dir: std.Io.Dir, manifest: Manifest) !void {
    var atomic_file = try instance_dir.createFileAtomic(io, "instance.json", .{ .replace = true });
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
    if (manifest.schema_version != schema_version) return error.UnsupportedInstanceSchema;
    if (manifest.name.len == 0 or manifest.name.len > 256) return error.InvalidInstanceName;
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
    var loaders: u8 = 0;
    if (manifest.fabric_loader != null) loaders += 1;
    if (manifest.neoforge_version != null) loaders += 1;
    if (manifest.forge_version != null) loaders += 1;
    if (loaders > 1) return error.LoaderConflict;
}

fn validateInstallation(installation: Installation) !void {
    if (installation.schema_version != installation_schema_version) {
        return error.UnsupportedInstallationSchema;
    }
    if (installation.version_id.len == 0 or installation.version_id.len > 256) {
        return error.InvalidVersionId;
    }
    for ([_]?[]const u8{
        installation.fabric_loader,
        installation.neoforge_version,
        installation.forge_version,
    }) |loader| {
        if (loader) |value| {
            if (value.len == 0 or value.len > 256) return error.InvalidLoaderVersion;
        }
    }
    const loader_count: u8 = @intFromBool(installation.fabric_loader != null) +
        @intFromBool(installation.neoforge_version != null) +
        @intFromBool(installation.forge_version != null);
    if (loader_count > 1) return error.LoaderConflict;
    if (installation.os.len == 0 or installation.os.len > 32 or
        installation.arch.len == 0 or installation.arch.len > 32)
    {
        return error.InvalidInstallationTarget;
    }
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

test "validates portable instance ids" {
    try validateId("vanilla-26.1");
    try validateId("fabric_test");
    for ([_][]const u8{ "", ".", "..", "Upper", "has space", "../escape", "con", "com1", "name." }) |id| {
        try std.testing.expectError(error.InvalidInstanceId, validateId(id));
    }
}

test "creates lists loads and deletes isolated instances" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try temporary.dir.createDir(io, "instances", .default_dir);
    const root = try temporary.dir.openDir(io, "instances", .{ .iterate = true });
    defer root.close(io);

    try create(io, root, .{
        .id = "test-instance",
        .name = "Test Instance",
        .version_id = "26.1",
        .fabric_loader = "0.16.10",
        .source = .bmclapi,
    });
    var document = try load(std.testing.allocator, io, root, "test-instance");
    defer document.deinit();
    try std.testing.expectEqualStrings("26.1", document.value.version_id);
    try std.testing.expectEqualStrings("0.16.10", document.value.fabric_loader.?);
    try std.testing.expectEqual(DownloadSource.bmclapi, document.value.source);
    const minecraft = try root.openDir(io, "test-instance/.minecraft", .{});
    minecraft.close(io);

    var instances = try list(std.testing.allocator, io, root);
    defer instances.deinit();
    try std.testing.expectEqual(@as(usize, 1), instances.items.len);
    try std.testing.expectEqualStrings("test-instance", instances.items[0].value.id);

    try delete(std.testing.allocator, io, root, "test-instance");
    try std.testing.expectError(error.FileNotFound, load(std.testing.allocator, io, root, "test-instance"));
}

test "installation marker matches only its completed manifest" {
    const io = std.testing.io;
    const allocator = std.testing.allocator;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    const manifest: Manifest = .{
        .id = "fabric",
        .name = "Fabric",
        .version_id = "1.21.4",
        .fabric_loader = "0.16.10",
        .source = .bmclapi,
    };

    try std.testing.expect(!try isInstalled(allocator, io, temporary.dir, manifest));
    try recordInstallation(io, temporary.dir, .{
        .version_id = manifest.version_id,
        .fabric_loader = manifest.fabric_loader,
        .source = manifest.source,
        .os = "linux",
        .arch = "x86_64",
    });
    try std.testing.expect(try isInstalled(allocator, io, temporary.dir, manifest));

    var other = manifest;
    other.fabric_loader = "0.16.11";
    try std.testing.expect(!try isInstalled(allocator, io, temporary.dir, other));
    other = manifest;
    other.version_id = "1.21.5";
    try std.testing.expect(!try isInstalled(allocator, io, temporary.dir, other));

    var file = try temporary.dir.createFile(io, installation_relative_path, .{ .truncate = true });
    defer file.close(io);
    try file.writeStreamingAll(io, "{broken");
    try std.testing.expect(!try isInstalled(allocator, io, temporary.dir, manifest));
}
