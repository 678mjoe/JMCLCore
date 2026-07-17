const std = @import("std");
const builtin = @import("builtin");
const hard_link = @import("hard_link.zig");

const Sha1 = std.crypto.hash.Sha1;

pub const default_directory = ".jmclcore";

pub const Placement = enum {
    hard_link,
    copy,
};

pub const ObjectLock = struct {
    file: std.Io.File,
    io: std.Io,

    pub fn deinit(lock: *ObjectLock) void {
        // Closing a file releases its advisory locks on every supported platform.
        // Zig 0.16's Windows unlock path contains reachable `unreachable` branches.
        lock.file.close(lock.io);
        lock.* = undefined;
    }
};

pub fn openRoot(io: std.Io, path: []const u8) !std.Io.Dir {
    if (path.len == 0) return error.EmptyStoreDirectory;
    const root = try std.Io.Dir.cwd().createDirPathOpen(io, path, .{ .open_options = .{ .follow_symlinks = false } });
    errdefer root.close(io);
    try root.createDirPath(io, "objects/sha1");
    try root.createDirPath(io, "locks/sha1");
    return root;
}

pub fn objectPath(hash: []const u8, buffer: *[64]u8) ![]const u8 {
    try validateSha1(hash);
    var normalized: [Sha1.digest_length * 2]u8 = undefined;
    for (hash, 0..) |byte, index| normalized[index] = std.ascii.toLower(byte);
    return std.fmt.bufPrint(buffer, "objects/sha1/{s}/{s}", .{ normalized[0..2], &normalized });
}

pub fn lockObject(io: std.Io, root: std.Io.Dir, hash: []const u8) !ObjectLock {
    try validateSha1(hash);
    try root.createDirPath(io, "locks/sha1");
    var normalized: [Sha1.digest_length * 2]u8 = undefined;
    for (hash, 0..) |byte, index| normalized[index] = std.ascii.toLower(byte);
    var path_buffer: [64]u8 = undefined;
    const path = try std.fmt.bufPrint(&path_buffer, "locks/sha1/{s}.lock", .{&normalized});
    const file = try root.createFile(io, path, .{
        .read = true,
        .truncate = false,
        .lock = .exclusive,
    });
    return .{ .file = file, .io = io };
}

pub fn verify(root: std.Io.Dir, io: std.Io, path: []const u8, expected_hash: []const u8, expected_size: u64) !bool {
    const file = root.openFile(io, path, .{
        .mode = .read_only,
        .allow_directory = false,
        .follow_symlinks = false,
        .resolve_beneath = true,
    }) catch |err| switch (err) {
        error.FileNotFound => return false,
        else => |other| return other,
    };
    defer file.close(io);
    const stat = try file.stat(io);
    if (stat.size != expected_size) return false;
    return digestMatches(try hashFile(file, io), expected_hash);
}

pub fn importObject(
    allocator: std.mem.Allocator,
    io: std.Io,
    store: std.Io.Dir,
    object_path: []const u8,
    source: std.Io.Dir,
    source_path: []const u8,
) !Placement {
    if (std.fs.path.dirname(object_path)) |parent| try store.createDirPath(io, parent);
    hard_link.create(allocator, io, source, source_path, store, object_path) catch |err| switch (err) {
        error.CrossDevice, error.OperationUnsupported, error.LinkQuotaExceeded, error.AccessDenied => {
            source.copyFile(source_path, store, object_path, io, .{ .make_path = true, .replace = false }) catch |copy_err| switch (copy_err) {
                error.PathAlreadyExists => {},
                else => |other| return other,
            };
            try setReadOnly(allocator, store, io, object_path);
            return .copy;
        },
        error.PathAlreadyExists => {},
        else => |other| return other,
    };
    try setReadOnly(allocator, store, io, object_path);
    return .hard_link;
}

pub fn materialize(
    allocator: std.mem.Allocator,
    io: std.Io,
    store: std.Io.Dir,
    object_path: []const u8,
    destination: std.Io.Dir,
    destination_path: []const u8,
    destination_valid: bool,
) !Placement {
    if (std.fs.path.dirname(destination_path)) |parent| try destination.createDirPath(io, parent);
    if (builtin.os.tag == .windows) {
        return materializeWindows(
            allocator,
            io,
            store,
            object_path,
            destination,
            destination_path,
            destination_valid,
        );
    }
    var random: [8]u8 = undefined;
    try io.randomSecure(&random);
    const suffix = std.fmt.bytesToHex(random, .lower);
    const temporary_path = try std.fmt.allocPrint(allocator, "{s}.jmcl-link-{s}", .{ destination_path, &suffix });
    defer allocator.free(temporary_path);

    hard_link.create(allocator, io, store, object_path, destination, temporary_path) catch |err| switch (err) {
        error.CrossDevice, error.OperationUnsupported, error.LinkQuotaExceeded, error.AccessDenied => {
            try store.copyFile(object_path, destination, destination_path, io, .{ .make_path = true, .replace = true });
            return .copy;
        },
        else => |other| return other,
    };
    errdefer destination.deleteFile(io, temporary_path) catch {};
    const temporary_stat = try destination.statFile(io, temporary_path, .{});
    const destination_stat = destination.statFile(io, destination_path, .{}) catch |err| switch (err) {
        error.FileNotFound => null,
        else => |other| return other,
    };
    if (destination_stat) |existing| {
        // Both paths are in the same directory handle, so inode equality is sufficient.
        if (existing.inode == temporary_stat.inode) {
            try destination.deleteFile(io, temporary_path);
            return .hard_link;
        }
    }
    try destination.rename(temporary_path, destination, destination_path, io);
    return .hard_link;
}

pub fn setReadOnly(allocator: std.mem.Allocator, root: std.Io.Dir, io: std.Io, path: []const u8) !void {
    return hard_link.setReadOnly(allocator, io, root, path, true);
}

fn materializeWindows(
    allocator: std.mem.Allocator,
    io: std.Io,
    store: std.Io.Dir,
    object_path: []const u8,
    destination: std.Io.Dir,
    destination_path: []const u8,
    destination_valid: bool,
) !Placement {
    const object_stat = try store.statFile(io, object_path, .{});
    const destination_stat = destination.statFile(io, destination_path, .{}) catch |err| switch (err) {
        error.FileNotFound => null,
        else => |other| return other,
    };
    if (destination_stat) |existing| {
        if (destination_valid and existing.inode == object_stat.inode) return .hard_link;
        if (destination_valid) return .copy;
        hard_link.setReadOnly(allocator, io, destination, destination_path, false) catch |err| switch (err) {
            error.FileNotFound => {},
            else => |other| return other,
        };
        destination.deleteFile(io, destination_path) catch |err| switch (err) {
            error.FileNotFound => {},
            else => |other| return other,
        };
    }
    hard_link.create(allocator, io, store, object_path, destination, destination_path) catch |err| switch (err) {
        error.CrossDevice, error.OperationUnsupported, error.LinkQuotaExceeded, error.AccessDenied => {
            try store.copyFile(object_path, destination, destination_path, io, .{ .make_path = true, .replace = true });
            return .copy;
        },
        else => |other| return other,
    };
    return .hard_link;
}

fn validateSha1(hash: []const u8) !void {
    if (hash.len != Sha1.digest_length * 2) return error.InvalidSha1;
    for (hash) |byte| if (!std.ascii.isHex(byte)) return error.InvalidSha1;
}

pub fn hashFile(file: std.Io.File, io: std.Io) ![Sha1.digest_length]u8 {
    if (builtin.os.tag == .windows) return hashFileWindows(file);
    var read_buffer: [64 * 1024]u8 = undefined;
    var reader = file.reader(io, &read_buffer);
    var chunk: [64 * 1024]u8 = undefined;
    var hasher = Sha1.init(.{});
    while (true) {
        const count = reader.interface.readSliceShort(&chunk) catch return reader.err.?;
        if (count == 0) break;
        hasher.update(chunk[0..count]);
    }
    return hasher.finalResult();
}

fn hashFileWindows(file: std.Io.File) ![Sha1.digest_length]u8 {
    var chunk: [64 * 1024]u8 = undefined;
    var hasher = Sha1.init(.{});
    var offset: u64 = 0;
    while (true) {
        const count = try hard_link.readFileWindows(file, &chunk, offset);
        if (count == 0) break;
        hasher.update(chunk[0..count]);
        offset += count;
    }
    return hasher.finalResult();
}

fn digestMatches(digest: [Sha1.digest_length]u8, expected: []const u8) bool {
    const actual = std.fmt.bytesToHex(digest, .lower);
    return std.ascii.eqlIgnoreCase(&actual, expected);
}

test "imports and materializes a shared object" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try temporary.dir.createDir(io, "store", .default_dir);
    try temporary.dir.createDir(io, "first", .default_dir);
    try temporary.dir.createDir(io, "second", .default_dir);
    const store = try temporary.dir.openDir(io, "store", .{});
    defer store.close(io);
    const first = try temporary.dir.openDir(io, "first", .{});
    defer first.close(io);
    const second = try temporary.dir.openDir(io, "second", .{});
    defer second.close(io);
    const source = try first.createFile(io, "library.jar", .{});
    try std.Io.File.writeStreamingAll(source, io, "abc");
    source.close(io);
    var path_buffer: [64]u8 = undefined;
    const path = try objectPath("a9993e364706816aba3e25717850c26c9cd0d89d", &path_buffer);
    _ = try importObject(std.testing.allocator, io, store, path, first, "library.jar");
    const placement = try materialize(std.testing.allocator, io, store, path, second, "library.jar", false);
    try std.testing.expectEqual(Placement.hard_link, placement);
    const store_stat = try store.statFile(io, path, .{});
    const second_stat = try second.statFile(io, "library.jar", .{});
    try std.testing.expectEqual(store_stat.inode, second_stat.inode);
    try std.testing.expect(store_stat.nlink >= 3);
}
