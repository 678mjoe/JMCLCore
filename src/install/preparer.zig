const std = @import("std");
const builtin = @import("builtin");
const hard_link = @import("hard_link.zig");
const artifact_store = @import("artifact_store.zig");
const install_plan = @import("plan.zig");

const Sha1 = std.crypto.hash.Sha1;
const max_archive_entries = 4096;
const max_entry_size = 256 * 1024 * 1024;
const max_total_native_size = 1024 * 1024 * 1024;
const max_compression_ratio = 1000;

pub const Result = struct {
    allocator: std.mem.Allocator,
    native_directory: ?[]u8,
    native_files: usize,
    projections_created: usize,
    projections_cached: usize,

    pub fn deinit(result: *Result) void {
        if (result.native_directory) |path| result.allocator.free(path);
        result.* = undefined;
    }
};

const EntryInfo = struct {
    name: []const u8,
    directory: bool,
};

pub fn prepare(
    allocator: std.mem.Allocator,
    io: std.Io,
    root: std.Io.Dir,
    plan: install_plan.Data,
) !Result {
    var projections_created: usize = 0;
    var projections_cached: usize = 0;
    try materializeProjections(
        allocator,
        io,
        root,
        plan,
        &projections_created,
        &projections_cached,
    );

    const native_files = if (plan.native_extractions.len == 0)
        0
    else
        try preflightNativeArchives(allocator, io, root, plan);
    const native_path = try createNativeDirectory(allocator, io, root);
    errdefer {
        root.deleteTree(io, native_path) catch {};
        allocator.free(native_path);
    }
    const native_dir = try root.openDir(io, native_path, .{ .follow_symlinks = false });
    defer native_dir.close(io);
    if (plan.native_extractions.len != 0) try extractNativeArchives(io, root, native_dir, plan);

    return .{
        .allocator = allocator,
        .native_directory = native_path,
        .native_files = native_files,
        .projections_created = projections_created,
        .projections_cached = projections_cached,
    };
}

fn materializeProjections(
    allocator: std.mem.Allocator,
    io: std.Io,
    root: std.Io.Dir,
    plan: install_plan.Data,
    created: *usize,
    cached: *usize,
) !void {
    var verified: std.StringHashMapUnmanaged(void) = .empty;
    defer verified.deinit(allocator);

    for (plan.asset_projections) |projection| {
        try install_plan.validateRelativePath(projection.source_path);
        try install_plan.validateRelativePath(projection.destination_path);
        const expected = findPlannedFile(plan.files, projection.source_path) orelse
            return error.ProjectionSourceNotPlanned;

        if (!verified.contains(projection.source_path)) {
            const source = try openVerified(root, io, expected);
            source.close(io);
            try verified.put(allocator, projection.source_path, {});
        }

        if (verifyPath(root, io, projection.destination_path, expected)) |valid| {
            if (valid) {
                cached.* += 1;
                continue;
            }
        } else |err| switch (err) {
            error.FileNotFound => {},
            else => |other| return other,
        }

        const source = try openVerified(root, io, expected);
        defer source.close(io);
        if (std.fs.path.dirname(projection.destination_path)) |parent| {
            try root.createDirPath(io, parent);
        }

        source.hardLink(io, root, projection.destination_path, .{}) catch |err| switch (err) {
            error.OperationUnsupported, error.CrossDevice => try copyOpenFileAtomic(
                source,
                io,
                root,
                projection.destination_path,
                expected.size,
                false,
            ),
            error.PathAlreadyExists => try copyOpenFileAtomic(
                source,
                io,
                root,
                projection.destination_path,
                expected.size,
                true,
            ),
            else => |other| return other,
        };
        created.* += 1;
    }
}

fn preflightNativeArchives(
    allocator: std.mem.Allocator,
    io: std.Io,
    root: std.Io.Dir,
    plan: install_plan.Data,
) !usize {
    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();
    const scratch = arena.allocator();
    var output_paths: std.StringHashMapUnmanaged(void) = .empty;

    var file_count: usize = 0;
    var total_size: u64 = 0;
    for (plan.native_extractions) |extraction| {
        const expected = findPlannedFile(plan.files, extraction.archive_path) orelse
            return error.NativeArchiveNotPlanned;
        const archive = try openVerified(root, io, expected);
        defer archive.close(io);

        var read_buffer: [64 * 1024]u8 = undefined;
        var reader = archive.reader(io, &read_buffer);
        var iterator = try std.zip.Iterator.init(&reader);
        var entries: usize = 0;
        var filename_buffer: [std.fs.max_path_bytes]u8 = undefined;
        while (try iterator.next()) |entry| {
            entries += 1;
            if (entries > max_archive_entries) return error.TooManyArchiveEntries;
            const info = try inspectEntry(&reader, entry, &filename_buffer);
            if (isExcluded(info.name, extraction.exclude)) continue;
            try validateEntryLimits(entry, &total_size);
            if (info.directory) continue;

            file_count += 1;
            const owned_name = try scratch.dupe(u8, info.name);
            const result = try output_paths.getOrPut(scratch, owned_name);
            if (result.found_existing) return error.ConflictingNativeEntry;
        }
    }
    return file_count;
}

fn extractNativeArchives(
    io: std.Io,
    root: std.Io.Dir,
    native_dir: std.Io.Dir,
    plan: install_plan.Data,
) !void {
    for (plan.native_extractions) |extraction| {
        const expected = findPlannedFile(plan.files, extraction.archive_path) orelse
            return error.NativeArchiveNotPlanned;
        const archive = try openVerified(root, io, expected);
        defer archive.close(io);

        var read_buffer: [64 * 1024]u8 = undefined;
        var reader = archive.reader(io, &read_buffer);
        var iterator = try std.zip.Iterator.init(&reader);
        var filename_buffer: [std.fs.max_path_bytes]u8 = undefined;
        while (try iterator.next()) |entry| {
            const info = try inspectEntry(&reader, entry, &filename_buffer);
            if (isExcluded(info.name, extraction.exclude)) continue;
            try entry.extract(&reader, .{ .allow_backslashes = false }, &filename_buffer, native_dir);
        }
    }
}

fn inspectEntry(
    reader: *std.Io.File.Reader,
    entry: std.zip.Iterator.Entry,
    filename_buffer: []u8,
) !EntryInfo {
    if (entry.filename_len == 0 or entry.filename_len > filename_buffer.len) {
        return error.UnsafeArchivePath;
    }
    try reader.seekTo(entry.header_zip_offset);
    const header = reader.interface.takeStruct(std.zip.CentralDirectoryFileHeader, .little) catch |err| switch (err) {
        error.ReadFailed => return reader.err.?,
        error.EndOfStream => return error.ZipTruncated,
    };
    const name = filename_buffer[0..entry.filename_len];
    reader.interface.readSliceAll(name) catch |err| switch (err) {
        error.ReadFailed => return reader.err.?,
        error.EndOfStream => return error.ZipTruncated,
    };
    if (std.mem.indexOfScalar(u8, name, '\\') != null) return error.UnsafeArchivePath;

    const directory = name[name.len - 1] == '/';
    const path = if (directory) name[0 .. name.len - 1] else name;
    try install_plan.validateRelativePath(path);

    const made_by_system = header.version_made_by >> 8;
    if (made_by_system == 3) {
        const mode = header.external_file_attributes >> 16;
        const file_type = mode & 0o170000;
        if (file_type != 0 and file_type != 0o100000 and file_type != 0o040000) {
            return error.UnsupportedArchiveEntryType;
        }
        if ((file_type == 0o040000) != directory) return error.ArchiveEntryTypeMismatch;
    }
    if ((header.external_file_attributes & 0x10) != 0 and !directory) {
        return error.ArchiveEntryTypeMismatch;
    }
    return .{ .name = name, .directory = directory };
}

fn validateEntryLimits(entry: std.zip.Iterator.Entry, total_size: *u64) !void {
    switch (entry.compression_method) {
        .store, .deflate => {},
        else => return error.UnsupportedCompressionMethod,
    }
    if (entry.uncompressed_size > max_entry_size) return error.ArchiveEntryTooLarge;
    if (entry.uncompressed_size > 0 and entry.compressed_size == 0) return error.InvalidCompressionRatio;
    if (entry.compressed_size > 0 and
        entry.uncompressed_size / entry.compressed_size > max_compression_ratio)
    {
        return error.InvalidCompressionRatio;
    }
    total_size.* = std.math.add(u64, total_size.*, entry.uncompressed_size) catch
        return error.NativeSizeOverflow;
    if (total_size.* > max_total_native_size) return error.NativeArchiveSetTooLarge;
}

fn isExcluded(name: []const u8, exclusions: []const []const u8) bool {
    for (exclusions) |prefix| {
        if (std.mem.startsWith(u8, name, prefix)) return true;
    }
    return false;
}

fn createNativeDirectory(allocator: std.mem.Allocator, io: std.Io, root: std.Io.Dir) ![]u8 {
    const parent = ".jmcl/runtime/natives";
    try root.createDirPath(io, parent);
    for (0..16) |_| {
        var nonce: u64 = undefined;
        try io.randomSecure(std.mem.asBytes(&nonce));
        const path = try std.fmt.allocPrint(allocator, "{s}/{x}", .{ parent, nonce });
        root.createDir(io, path, .default_dir) catch |err| switch (err) {
            error.PathAlreadyExists => {
                allocator.free(path);
                continue;
            },
            else => |other| {
                allocator.free(path);
                return other;
            },
        };
        return path;
    }
    return error.NativeDirectoryCollision;
}

fn findPlannedFile(files: []const install_plan.File, path: []const u8) ?install_plan.File {
    for (files) |file| {
        if (std.mem.eql(u8, file.path, path)) return file;
    }
    return null;
}

fn openVerified(root: std.Io.Dir, io: std.Io, expected: install_plan.File) !std.Io.File {
    var file = try root.openFile(io, expected.path, .{
        .mode = .read_only,
        .allow_directory = false,
        .follow_symlinks = false,
        .resolve_beneath = true,
    });
    errdefer file.close(io);
    // Zig 0.16 opens no-follow Windows files with ASYNCHRONOUS IO but marks
    // the returned File synchronous. Correct that metadata so both ZIP
    // preflight and extraction readers wait for pending positional reads.
    // Keep the no-follow / resolve-beneath options above unchanged.
    if (builtin.os.tag == .windows) file.flags.nonblocking = true;
    if (!try verifyOpenFile(file, io, expected)) return error.CacheFileInvalid;
    return file;
}

fn verifyPath(root: std.Io.Dir, io: std.Io, path: []const u8, expected: install_plan.File) !bool {
    const file = try root.openFile(io, path, .{
        .mode = .read_only,
        .allow_directory = false,
        .follow_symlinks = false,
        .resolve_beneath = true,
    });
    defer file.close(io);
    return verifyOpenFile(file, io, expected);
}

fn verifyOpenFile(file: std.Io.File, io: std.Io, expected: install_plan.File) !bool {
    const stat = try file.stat(io);
    if (stat.size != expected.size) return false;
    const digest = std.fmt.bytesToHex(try artifact_store.hashFile(file, io), .lower);
    return std.ascii.eqlIgnoreCase(&digest, expected.sha1);
}

fn copyOpenFileAtomic(
    source: std.Io.File,
    io: std.Io,
    root: std.Io.Dir,
    destination: []const u8,
    size: u64,
    replace: bool,
) !void {
    var atomic_file = try root.createFileAtomic(io, destination, .{
        .make_path = true,
        .replace = replace,
    });
    defer atomic_file.deinit(io);

    var source_buffer: [64 * 1024]u8 = undefined;
    var destination_buffer: [64 * 1024]u8 = undefined;
    var writer: std.Io.File.Writer = .init(atomic_file.file, io, &destination_buffer);
    if (builtin.os.tag == .windows) {
        // The source is an overlapped Windows handle. Explicit offsets avoid
        // Zig 0.16's synchronous-reader path and remain independent of hashing.
        var offset: u64 = 0;
        while (offset < size) {
            const wanted: usize = @intCast(@min(source_buffer.len, size - offset));
            const count = try hard_link.readFileWindows(source, source_buffer[0..wanted], offset);
            if (count == 0) return error.UnexpectedEndOfFile;
            try writer.interface.writeAll(source_buffer[0..count]);
            offset += count;
        }
    } else {
        var reader: std.Io.File.Reader = .initSize(source, io, &source_buffer, size);
        _ = reader.interface.streamRemaining(&writer.interface) catch |err| switch (err) {
            error.ReadFailed => return reader.err.?,
            error.WriteFailed => return writer.err.?,
        };
    }
    try writer.flush();
    try atomic_file.file.sync(io);
    if (replace) {
        try atomic_file.replace(io);
    } else {
        try atomic_file.link(io);
    }
}

test "prepares native files and legacy asset projections" {
    var zip_output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer zip_output.deinit();
    const zip_writer = &zip_output.writer;
    const filename = "native.bin";
    const crc32: u32 = 0x352441c2;

    try zip_writer.writeAll("PK\x03\x04");
    try zip_writer.writeInt(u16, 20, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u32, crc32, .little);
    try zip_writer.writeInt(u32, 3, .little);
    try zip_writer.writeInt(u32, 3, .little);
    try zip_writer.writeInt(u16, filename.len, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeAll(filename);
    try zip_writer.writeAll("abc");

    const central_offset: u32 = @intCast(zip_writer.end);
    try zip_writer.writeAll("PK\x01\x02");
    try zip_writer.writeInt(u16, 0x0314, .little);
    try zip_writer.writeInt(u16, 20, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u32, crc32, .little);
    try zip_writer.writeInt(u32, 3, .little);
    try zip_writer.writeInt(u32, 3, .little);
    try zip_writer.writeInt(u16, filename.len, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u32, 0o100644 << 16, .little);
    try zip_writer.writeInt(u32, 0, .little);
    try zip_writer.writeAll(filename);
    const central_size: u32 = @intCast(zip_writer.end - central_offset);

    try zip_writer.writeAll("PK\x05\x06");
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u16, 0, .little);
    try zip_writer.writeInt(u16, 1, .little);
    try zip_writer.writeInt(u16, 1, .little);
    try zip_writer.writeInt(u32, central_size, .little);
    try zip_writer.writeInt(u32, central_offset, .little);
    try zip_writer.writeInt(u16, 0, .little);
    const zip_bytes = zip_output.written();

    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try temporary.dir.createDirPath(io, "libraries/native");
    const archive = try temporary.dir.createFile(io, "libraries/native/test.jar", .{});
    try std.Io.File.writeStreamingAll(archive, io, zip_bytes);
    archive.close(io);
    try temporary.dir.createDirPath(io, "assets/objects/a9");
    const object = try temporary.dir.createFile(io, "assets/objects/a9/object", .{});
    try std.Io.File.writeStreamingAll(object, io, "abc");
    object.close(io);

    var archive_digest: [Sha1.digest_length]u8 = undefined;
    Sha1.hash(zip_bytes, &archive_digest, .{});
    const archive_hash = std.fmt.bytesToHex(archive_digest, .lower);
    const files = [_]install_plan.File{
        .{
            .path = "libraries/native/test.jar",
            .url = "https://example.invalid/native.jar",
            .fallback_url = null,
            .sha1 = &archive_hash,
            .size = zip_bytes.len,
            .usage = .{ .native = true },
        },
        .{
            .path = "assets/objects/a9/object",
            .url = "https://example.invalid/object",
            .fallback_url = null,
            .sha1 = "a9993e364706816aba3e25717850c26c9cd0d89d",
            .size = 3,
            .usage = .{ .asset_object = true },
        },
    };
    const extractions = [_]install_plan.NativeExtraction{.{
        .archive_path = "libraries/native/test.jar",
        .exclude = &.{"META-INF/"},
    }};
    const projections = [_]install_plan.AssetProjection{.{
        .source_path = "assets/objects/a9/object",
        .destination_path = "assets/virtual/test/example.txt",
        .kind = .virtual_assets,
    }};
    const plan: install_plan.Data = .{
        .version_id = "test",
        .source = .official,
        .target = .{ .os = .linux, .arch = .x86_64 },
        .files = &files,
        .classpath = &.{},
        .native_extractions = &extractions,
        .asset_projections = &projections,
        .total_download_size = zip_bytes.len + 3,
    };

    var result = try prepare(std.testing.allocator, io, temporary.dir, plan);
    defer result.deinit();
    try std.testing.expectEqual(@as(usize, 1), result.native_files);
    try std.testing.expectEqual(@as(usize, 1), result.projections_created);

    const native_path = try std.fmt.allocPrint(
        std.testing.allocator,
        "{s}/native.bin",
        .{result.native_directory.?},
    );
    defer std.testing.allocator.free(native_path);
    const expected_native: install_plan.File = .{
        .path = native_path,
        .url = "https://example.invalid/native.bin",
        .fallback_url = null,
        .sha1 = "a9993e364706816aba3e25717850c26c9cd0d89d",
        .size = 3,
        .usage = .{ .native = true },
    };
    try std.testing.expect(try verifyPath(temporary.dir, io, native_path, expected_native));
    try std.testing.expect(try verifyPath(
        temporary.dir,
        io,
        "assets/virtual/test/example.txt",
        files[1],
    ));

    var second_result = try prepare(std.testing.allocator, io, temporary.dir, plan);
    defer second_result.deinit();
    try std.testing.expectEqual(@as(usize, 0), second_result.projections_created);
    try std.testing.expectEqual(@as(usize, 1), second_result.projections_cached);
}

test "atomic asset projection copies multiple buffers after hashing the source" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    var data: [128 * 1024 + 17]u8 = undefined;
    for (&data, 0..) |*byte, index| byte.* = @truncate(index);
    try temporary.dir.writeFile(io, .{ .sub_path = "source", .data = &data });
    try temporary.dir.writeFile(io, .{ .sub_path = "destination", .data = "old" });
    const source = try temporary.dir.openFile(io, "source", .{});
    defer source.close(io);
    const expected_hash = try artifact_store.hashFile(source, io);
    try copyOpenFileAtomic(source, io, temporary.dir, "destination", data.len, true);
    const destination = try temporary.dir.openFile(io, "destination", .{});
    defer destination.close(io);
    try std.testing.expectEqual(@as(u64, data.len), (try destination.stat(io)).size);
    try std.testing.expectEqualSlices(u8, &expected_hash, &(try artifact_store.hashFile(destination, io)));
}

test "verified no-follow archives support positional readers" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    const data = "verified archive bytes";
    try temporary.dir.writeFile(io, .{ .sub_path = "archive", .data = data });
    var digest: [Sha1.digest_length]u8 = undefined;
    Sha1.hash(data, &digest, .{});
    const hash = std.fmt.bytesToHex(digest, .lower);
    const file = try openVerified(temporary.dir, io, .{
        .path = "archive",
        .url = "https://example.invalid/archive",
        .fallback_url = null,
        .sha1 = &hash,
        .size = data.len,
        .usage = .{ .native = true },
    });
    defer file.close(io);
    if (builtin.os.tag == .windows) try std.testing.expect(file.flags.nonblocking);
    var buffer: [8]u8 = undefined;
    var reader = file.reader(io, &buffer);
    try reader.seekTo(9);
    var actual: [7]u8 = undefined;
    try reader.interface.readSliceAll(&actual);
    try std.testing.expectEqualStrings("archive", &actual);
    try reader.seekTo(0);
    try reader.interface.readSliceAll(&actual);
    try std.testing.expectEqualStrings("verifie", &actual);
}
