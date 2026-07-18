const std = @import("std");
const builtin = @import("builtin");
const hard_link = @import("hard_link.zig");

const Sha1 = std.crypto.hash.Sha1;

pub const default_directory = ".jmclcore";

pub const Placement = enum {
    hard_link,
    copy,
};

pub const GcOptions = struct {
    dry_run: bool = true,
    min_age_seconds: u64 = 24 * 60 * 60,
};

pub const GcSummary = struct {
    objects_scanned: u64 = 0,
    objects_eligible: u64 = 0,
    objects_deleted: u64 = 0,
    bytes_eligible: u64 = 0,
    bytes_deleted: u64 = 0,
    objects_linked: u64 = 0,
    objects_young: u64 = 0,
    objects_ignored: u64 = 0,
    objects_busy: u64 = 0,
    errors: u64 = 0,
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
    const objects = try openOrCreateDirectory(io, root, "objects");
    defer objects.close(io);
    const object_hashes = try openOrCreateDirectory(io, objects, "sha1");
    object_hashes.close(io);
    const locks = try openOrCreateDirectory(io, root, "locks");
    defer locks.close(io);
    const lock_hashes = try openOrCreateDirectory(io, locks, "sha1");
    lock_hashes.close(io);
    return root;
}

pub fn openExistingRoot(io: std.Io, path: []const u8) !std.Io.Dir {
    if (path.len == 0) return error.EmptyStoreDirectory;
    return std.Io.Dir.cwd().openDir(io, path, .{ .follow_symlinks = false });
}

pub fn objectPath(hash: []const u8, buffer: *[64]u8) ![]const u8 {
    try validateSha1(hash);
    var normalized: [Sha1.digest_length * 2]u8 = undefined;
    for (hash, 0..) |byte, index| normalized[index] = std.ascii.toLower(byte);
    return std.fmt.bufPrint(buffer, "objects/sha1/{s}/{s}", .{ normalized[0..2], &normalized });
}

pub fn lockObject(io: std.Io, root: std.Io.Dir, hash: []const u8) !ObjectLock {
    try validateSha1(hash);
    const locks = try openOrCreateDirectory(io, root, "locks");
    defer locks.close(io);
    const lock_hashes = try openOrCreateDirectory(io, locks, "sha1");
    defer lock_hashes.close(io);
    var normalized: [Sha1.digest_length * 2]u8 = undefined;
    for (hash, 0..) |byte, index| normalized[index] = std.ascii.toLower(byte);
    var name_buffer: [Sha1.digest_length * 2 + 5]u8 = undefined;
    const name = try std.fmt.bufPrint(&name_buffer, "{s}.lock", .{&normalized});
    const file = lock_hashes.createFile(io, name, .{
        .read = true,
        .truncate = false,
        .exclusive = true,
        .lock = .exclusive,
        .resolve_beneath = true,
    }) catch |err| switch (err) {
        error.PathAlreadyExists => try lock_hashes.openFile(io, name, .{
            .mode = .read_write,
            .allow_directory = false,
            .follow_symlinks = false,
            .resolve_beneath = true,
            .lock = .exclusive,
        }),
        else => |other| return other,
    };
    return .{ .file = file, .io = io };
}

fn openOrCreateDirectory(io: std.Io, parent: std.Io.Dir, name: []const u8) !std.Io.Dir {
    return parent.openDir(io, name, .{ .follow_symlinks = false }) catch |err| switch (err) {
        error.FileNotFound => {
            parent.createDir(io, name, .default_dir) catch |create_err| switch (create_err) {
                error.PathAlreadyExists => {},
                else => |other| return other,
            };
            return parent.openDir(io, name, .{ .follow_symlinks = false });
        },
        else => |other| return other,
    };
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

/// Reclaims unlinked cache objects. This is intentionally cache reclamation,
/// not instance reachability tracking: copies remain usable after collection.
pub fn collect(
    allocator: std.mem.Allocator,
    io: std.Io,
    root: std.Io.Dir,
    options: GcOptions,
) !GcSummary {
    var summary: GcSummary = .{};
    const objects_root = try root.openDir(io, "objects", .{
        .access_sub_paths = true,
        .follow_symlinks = false,
    });
    defer objects_root.close(io);
    const objects = try objects_root.openDir(io, "sha1", .{
        .iterate = true,
        .follow_symlinks = false,
    });
    defer objects.close(io);

    var prefixes = objects.iterate();
    while (try prefixes.next(io)) |prefix_entry| {
        if ((prefix_entry.kind != .directory and prefix_entry.kind != .unknown) or
            !isLowerHexPrefix(prefix_entry.name))
        {
            try increment(&summary.objects_ignored);
            continue;
        }
        const prefix = objects.openDir(io, prefix_entry.name, .{
            .iterate = true,
            .follow_symlinks = false,
        }) catch |err| switch (err) {
            error.NotDir, error.FileNotFound, error.AccessDenied => {
                try increment(&summary.objects_ignored);
                continue;
            },
            else => return err,
        };
        defer prefix.close(io);

        var entries = prefix.iterate();
        while (try entries.next(io)) |entry| {
            if ((entry.kind != .file and entry.kind != .unknown) or
                !isCanonicalObjectName(prefix_entry.name, entry.name))
            {
                try increment(&summary.objects_ignored);
                continue;
            }
            try increment(&summary.objects_scanned);
            try collectObject(allocator, io, root, prefix, entry.name, options, &summary);
        }
    }
    return summary;
}

fn collectObject(
    allocator: std.mem.Allocator,
    io: std.Io,
    root: std.Io.Dir,
    prefix: std.Io.Dir,
    hash: []const u8,
    options: GcOptions,
    summary: *GcSummary,
) !void {
    const before_lock = statObject(prefix, io, hash) catch |err| {
        try countCollectionError(summary, err);
        return;
    } orelse return;
    if (before_lock.nlink != 1) {
        try increment(&summary.objects_linked);
        return;
    }
    if (!isOldEnough(io, before_lock.mtime, options.min_age_seconds)) {
        try increment(&summary.objects_young);
        return;
    }
    if (options.dry_run) {
        try increment(&summary.objects_eligible);
        try addBytes(&summary.bytes_eligible, before_lock.size);
        return;
    }

    var lock = lockObject(io, root, hash) catch |err| {
        try countCollectionError(summary, err);
        return;
    };
    defer lock.deinit();
    const opened: std.Io.File = if (builtin.os.tag == .windows) windows: {
        break :windows hard_link.openForDeleteWindows(allocator, prefix, hash) catch |err| {
            try countCollectionError(summary, err);
            return;
        };
    } else posix: {
        const object = openObject(prefix, io, hash, false) catch |err| {
            try countCollectionError(summary, err);
            return;
        };
        break :posix object orelse return;
    };
    defer opened.close(io);
    const stat = opened.stat(io) catch |err| {
        try countCollectionError(summary, err);
        return;
    };
    if (stat.kind != .file) {
        try increment(&summary.objects_ignored);
        return;
    }
    if (stat.nlink != 1) {
        try increment(&summary.objects_linked);
        return;
    }
    if (!isOldEnough(io, stat.mtime, options.min_age_seconds)) {
        try increment(&summary.objects_young);
        return;
    }

    try increment(&summary.objects_eligible);
    try addBytes(&summary.bytes_eligible, stat.size);
    if (builtin.os.tag == .windows) {
        hard_link.deleteReadOnlyWindows(opened) catch |err| {
            try countCollectionError(summary, err);
            return;
        };
    } else prefix.deleteFile(io, hash) catch |err| {
        try countCollectionError(summary, err);
        return;
    };
    try increment(&summary.objects_deleted);
    try addBytes(&summary.bytes_deleted, stat.size);
}

fn statObject(root: std.Io.Dir, io: std.Io, path: []const u8) !?std.Io.File.Stat {
    const file = try openObject(root, io, path, false) orelse return null;
    defer file.close(io);
    const stat = try file.stat(io);
    if (stat.kind != .file) return error.InvalidObjectKind;
    return stat;
}

fn openObject(root: std.Io.Dir, io: std.Io, path: []const u8, writable: bool) !?std.Io.File {
    return root.openFile(io, path, .{
        .mode = if (writable) .read_write else .read_only,
        .allow_directory = false,
        .follow_symlinks = false,
        .resolve_beneath = true,
    }) catch |err| switch (err) {
        error.FileNotFound => null,
        else => |other| return other,
    };
}

fn isLowerHexPrefix(value: []const u8) bool {
    return value.len == 2 and isLowerHex(value[0]) and isLowerHex(value[1]);
}

fn isCanonicalObjectName(prefix: []const u8, value: []const u8) bool {
    if (value.len != Sha1.digest_length * 2 or !std.mem.eql(u8, value[0..2], prefix)) return false;
    for (value) |byte| {
        if (!isLowerHex(byte)) return false;
    }
    return true;
}

fn isLowerHex(byte: u8) bool {
    return (byte >= '0' and byte <= '9') or (byte >= 'a' and byte <= 'f');
}

fn isOldEnough(io: std.Io, modified: std.Io.Timestamp, minimum_seconds: u64) bool {
    const minimum_nanoseconds: i96 = @as(i96, @intCast(minimum_seconds)) * std.time.ns_per_s;
    const age = modified.durationTo(std.Io.Clock.real.now(io)).nanoseconds;
    return age >= minimum_nanoseconds;
}

fn countCollectionError(summary: *GcSummary, err: anyerror) !void {
    switch (err) {
        error.FileNotFound, error.PathBusy => try increment(&summary.objects_busy),
        else => try increment(&summary.errors),
    }
}

fn increment(value: *u64) !void {
    value.* = std.math.add(u64, value.*, 1) catch return error.CounterOverflow;
}

fn addBytes(value: *u64, amount: u64) !void {
    value.* = std.math.add(u64, value.*, amount) catch return error.CounterOverflow;
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

test "collect dry run reports and preserves an orphaned object" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try temporary.dir.createDir(io, "store", .default_dir);
    const store = try temporary.dir.openDir(io, "store", .{});
    defer store.close(io);
    const path = try createTestObject(io, store);
    const malformed = try store.createFile(io, "objects/sha1/a9/not-a-hash", .{});
    malformed.close(io);

    const summary = try collect(std.testing.allocator, io, store, .{ .min_age_seconds = 0 });
    try std.testing.expectEqual(@as(u64, 1), summary.objects_scanned);
    try std.testing.expectEqual(@as(u64, 1), summary.objects_eligible);
    try std.testing.expectEqual(@as(u64, 0), summary.objects_deleted);
    try std.testing.expectEqual(@as(u64, 1), summary.objects_ignored);
    _ = try store.statFile(io, path, .{});
    try std.testing.expectError(error.FileNotFound, store.statFile(io, "locks/sha1/a9993e364706816aba3e25717850c26c9cd0d89d.lock", .{}));
}

test "collect deletes only the final object link" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try temporary.dir.createDir(io, "store", .default_dir);
    try temporary.dir.createDir(io, "instance", .default_dir);
    const store = try temporary.dir.openDir(io, "store", .{});
    defer store.close(io);
    const instance = try temporary.dir.openDir(io, "instance", .{});
    defer instance.close(io);
    const path = try createTestObject(io, store);
    try hard_link.create(std.testing.allocator, io, store, path, instance, "library.jar");

    const linked = try collect(std.testing.allocator, io, store, .{
        .dry_run = false,
        .min_age_seconds = 0,
    });
    try std.testing.expectEqual(@as(u64, 1), linked.objects_linked);
    try std.testing.expectEqual(@as(u64, 0), linked.objects_deleted);
    _ = try store.statFile(io, path, .{});

    try instance.deleteFile(io, "library.jar");
    const deleted = try collect(std.testing.allocator, io, store, .{
        .dry_run = false,
        .min_age_seconds = 0,
    });
    try std.testing.expectEqual(@as(u64, 1), deleted.objects_deleted);
    try std.testing.expectError(error.FileNotFound, store.statFile(io, path, .{}));
}

test "collect retains young objects" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try temporary.dir.createDir(io, "store", .default_dir);
    const store = try temporary.dir.openDir(io, "store", .{});
    defer store.close(io);
    const path = try createTestObject(io, store);

    const summary = try collect(std.testing.allocator, io, store, .{
        .dry_run = false,
        .min_age_seconds = 60 * 60 * 24 * 365 * 100,
    });
    try std.testing.expectEqual(@as(u64, 1), summary.objects_young);
    try std.testing.expectEqual(@as(u64, 0), summary.objects_deleted);
    _ = try store.statFile(io, path, .{});
}

fn createTestObject(io: std.Io, store: std.Io.Dir) ![]const u8 {
    const hash = "a9993e364706816aba3e25717850c26c9cd0d89d";
    var path_buffer: [64]u8 = undefined;
    const path = try objectPath(hash, &path_buffer);
    try store.createDirPath(io, "objects/sha1/a9");
    const file = try store.createFile(io, path, .{});
    try std.Io.File.writeStreamingAll(file, io, "abc");
    file.close(io);
    return "objects/sha1/a9/a9993e364706816aba3e25717850c26c9cd0d89d";
}
