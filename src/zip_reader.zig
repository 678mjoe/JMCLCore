const std = @import("std");
const builtin = @import("builtin");

pub const max_entries: usize = 65_536;
pub const max_entry_name_bytes: usize = 4096;

pub const Entry = struct {
    name: []const u8,
    compression_method: std.zip.CompressionMethod,
    compressed_size: u64,
    uncompressed_size: u64,
    local_header_offset: u64,
};

pub fn findEntry(jar: []const u8, name: []const u8) !?Entry {
    var iterator = try entries(jar);
    while (try iterator.next()) |entry| {
        if (std.mem.eql(u8, entry.name, name)) return entry;
    }
    return null;
}

pub fn readEntryAlloc(
    allocator: std.mem.Allocator,
    jar: []const u8,
    name: []const u8,
    max_uncompressed_bytes: usize,
) !?[]u8 {
    const entry = (try findEntry(jar, name)) orelse return null;
    return try readAlloc(allocator, jar, entry, max_uncompressed_bytes);
}

pub fn readAlloc(
    allocator: std.mem.Allocator,
    jar: []const u8,
    entry: Entry,
    max_uncompressed_bytes: usize,
) ![]u8 {
    if (entry.uncompressed_size > max_uncompressed_bytes) return error.ZipEntryTooLarge;
    const local_offset = std.math.cast(usize, entry.local_header_offset) orelse return error.ZipTruncated;
    if (local_offset + @sizeOf(std.zip.LocalFileHeader) > jar.len) return error.ZipTruncated;
    const local: std.zip.LocalFileHeader = readStruct(std.zip.LocalFileHeader, jar, local_offset);
    if (!std.mem.eql(u8, &local.signature, &std.zip.local_file_header_sig)) return error.ZipTruncated;
    if (local.flags.encrypted) return error.ZipEntryEncrypted;

    const name_len: usize = local.filename_len;
    const extra_len: usize = local.extra_len;
    const data_offset = std.math.add(usize, local_offset + @sizeOf(std.zip.LocalFileHeader), name_len +| extra_len) catch
        return error.ZipTruncated;
    const compressed_size = std.math.cast(usize, entry.compressed_size) orelse return error.ZipTruncated;
    const data_end = std.math.add(usize, data_offset, compressed_size) catch return error.ZipTruncated;
    if (data_end > jar.len) return error.ZipTruncated;
    const compressed = jar[data_offset..data_end];

    const uncompressed_size = std.math.cast(usize, entry.uncompressed_size) orelse return error.ZipEntryTooLarge;
    switch (entry.compression_method) {
        .store => {
            if (compressed.len != uncompressed_size) return error.ZipTruncated;
            return allocator.dupe(u8, compressed);
        },
        .deflate => {
            const output = try allocator.alloc(u8, uncompressed_size);
            errdefer allocator.free(output);
            var input: std.Io.Reader = .fixed(compressed);
            var decompress_buffer: [std.compress.flate.max_window_len]u8 = undefined;
            var decompress: std.compress.flate.Decompress = .init(&input, .raw, &decompress_buffer);
            decompress.reader.readSliceAll(output) catch return error.ZipTruncated;
            const extra = decompress.reader.takeByte() catch |err| switch (err) {
                error.EndOfStream => 0,
                error.ReadFailed => return error.ZipTruncated,
            };
            if (extra != 0) return error.ZipTruncated;
            return output;
        },
        else => return error.ZipUnsupportedCompression,
    }
}

pub const Iterator = struct {
    jar: []const u8,
    offset: usize,
    remaining: usize,

    pub fn next(iterator: *Iterator) !?Entry {
        if (iterator.remaining == 0) return null;
        if (iterator.offset + @sizeOf(std.zip.CentralDirectoryFileHeader) > iterator.jar.len) return error.ZipTruncated;
        const header: std.zip.CentralDirectoryFileHeader = readStruct(
            std.zip.CentralDirectoryFileHeader,
            iterator.jar,
            iterator.offset,
        );
        if (!std.mem.eql(u8, &header.signature, &std.zip.central_file_header_sig)) return error.ZipTruncated;
        if (header.flags.encrypted) return error.ZipEntryEncrypted;
        const name_len: usize = header.filename_len;
        if (name_len == 0 or name_len > max_entry_name_bytes) return error.ZipUnsafeEntryName;
        const extra_len: usize = header.extra_len;
        const comment_len: usize = header.comment_len;
        const name_offset = iterator.offset + @sizeOf(std.zip.CentralDirectoryFileHeader);
        const record_end = std.math.add(usize, name_offset, name_len +| extra_len +| comment_len) catch
            return error.ZipTruncated;
        if (record_end > iterator.jar.len) return error.ZipTruncated;
        const name = iterator.jar[name_offset .. name_offset + name_len];
        if (std.mem.indexOfScalar(u8, name, '\\') != null) return error.ZipUnsafeEntryName;
        iterator.offset = record_end;
        iterator.remaining -= 1;
        return .{
            .name = name,
            .compression_method = header.compression_method,
            .compressed_size = header.compressed_size,
            .uncompressed_size = header.uncompressed_size,
            .local_header_offset = header.local_file_header_offset,
        };
    }
};

pub fn entries(jar: []const u8) !Iterator {
    const record = try findEndRecord(jar);
    if (record.need_zip64()) return error.Zip64Unsupported;
    const offset = std.math.cast(usize, record.central_directory_offset) orelse return error.ZipTruncated;
    const size = std.math.cast(usize, record.central_directory_size) orelse return error.ZipTruncated;
    const end = std.math.add(usize, offset, size) catch return error.ZipTruncated;
    if (end > jar.len) return error.ZipTruncated;
    if (record.record_count_total > max_entries) return error.ZipTooManyEntries;
    return .{
        .jar = jar,
        .offset = offset,
        .remaining = record.record_count_total,
    };
}

fn findEndRecord(jar: []const u8) !std.zip.EndRecord {
    const position = std.mem.lastIndexOf(u8, jar, &std.zip.end_record_sig) orelse return error.ZipNoEndRecord;
    if (position + @sizeOf(std.zip.EndRecord) > jar.len) return error.ZipTruncated;
    return readStruct(std.zip.EndRecord, jar, position);
}

fn readStruct(comptime T: type, bytes: []const u8, offset: usize) T {
    const slice = bytes[offset..][0..@sizeOf(T)];
    const value: T = @bitCast(slice.*);
    if (builtin.target.cpu.arch.endian() == .little) return value;
    var copy = value;
    std.mem.byteSwapAllFields(T, &copy);
    return copy;
}

const TestZipBuilder = struct {
    const Pending = struct {
        name: []const u8,
        method: std.zip.CompressionMethod,
        data: []const u8,
        local_offset: u32,
        compressed_size: u32,
    };

    fn build(allocator: std.mem.Allocator) ![]u8 {
        var output: std.Io.Writer.Allocating = .init(allocator);
        defer output.deinit();
        var pending: std.ArrayList(Pending) = .empty;
        defer pending.deinit(allocator);

        try appendEntry(allocator, &output, &pending, "stored.txt", .store, "stored-content");
        try appendEntry(allocator, &output, &pending, "deflated.txt", .deflate, "deflated-content-deflated-content");

        const central_offset: u32 = @intCast(output.written().len);
        for (pending.items) |entry| {
            try writeInt(&output.writer, u32, 0x02014b50);
            try writeInt(&output.writer, u16, 20);
            try writeInt(&output.writer, u16, 20);
            try writeInt(&output.writer, u16, 0);
            try writeInt(&output.writer, u16, @intFromEnum(entry.method));
            try writeInt(&output.writer, u16, 0);
            try writeInt(&output.writer, u16, 0);
            try writeInt(&output.writer, u32, 0);
            try writeInt(&output.writer, u32, entry.compressed_size);
            try writeInt(&output.writer, u32, @intCast(entry.data.len));
            try writeInt(&output.writer, u16, @intCast(entry.name.len));
            try writeInt(&output.writer, u16, 0);
            try writeInt(&output.writer, u16, 0);
            try writeInt(&output.writer, u16, 0);
            try writeInt(&output.writer, u16, 0);
            try writeInt(&output.writer, u32, 0);
            try writeInt(&output.writer, u32, entry.local_offset);
            try output.writer.writeAll(entry.name);
        }
        const central_size: u32 = @intCast(output.written().len - central_offset);
        try writeInt(&output.writer, u32, 0x06054b50);
        try writeInt(&output.writer, u16, 0);
        try writeInt(&output.writer, u16, 0);
        try writeInt(&output.writer, u16, @intCast(pending.items.len));
        try writeInt(&output.writer, u16, @intCast(pending.items.len));
        try writeInt(&output.writer, u32, central_size);
        try writeInt(&output.writer, u32, central_offset);
        try writeInt(&output.writer, u16, 0);
        return output.toOwnedSlice();
    }

    fn appendEntry(
        allocator: std.mem.Allocator,
        output: *std.Io.Writer.Allocating,
        pending: *std.ArrayList(Pending),
        name: []const u8,
        method: std.zip.CompressionMethod,
        data: []const u8,
    ) !void {
        const compressed = switch (method) {
            .store => data,
            .deflate => try deflateAlloc(allocator, data),
            else => unreachable,
        };
        defer if (method == .deflate) allocator.free(compressed);
        const local_offset: u32 = @intCast(output.written().len);
        try writeInt(&output.writer, u32, 0x04034b50);
        try writeInt(&output.writer, u16, 20);
        try writeInt(&output.writer, u16, 0);
        try writeInt(&output.writer, u16, @intFromEnum(method));
        try writeInt(&output.writer, u16, 0);
        try writeInt(&output.writer, u16, 0);
        try writeInt(&output.writer, u32, 0);
        try writeInt(&output.writer, u32, @intCast(compressed.len));
        try writeInt(&output.writer, u32, @intCast(data.len));
        try writeInt(&output.writer, u16, @intCast(name.len));
        try writeInt(&output.writer, u16, 0);
        try output.writer.writeAll(name);
        try output.writer.writeAll(compressed);
        try pending.append(allocator, .{
            .name = name,
            .method = method,
            .data = data,
            .local_offset = local_offset,
            .compressed_size = @intCast(compressed.len),
        });
    }

    fn deflateAlloc(allocator: std.mem.Allocator, input: []const u8) ![]u8 {
        var compressed: std.Io.Writer.Allocating = .init(allocator);
        defer compressed.deinit();
        try compressed.writer.ensureUnusedCapacity(64);
        const window = try allocator.alloc(u8, std.compress.flate.max_window_len);
        defer allocator.free(window);
        var compressor = try std.compress.flate.Compress.init(&compressed.writer, window, .raw, .default);
        try compressor.writer.writeAll(input);
        try compressor.finish();
        return compressed.toOwnedSlice();
    }

    fn writeInt(writer: *std.Io.Writer, comptime T: type, value: T) !void {
        var bytes: [@sizeOf(T)]u8 = undefined;
        std.mem.writeInt(T, &bytes, value, .little);
        try writer.writeAll(&bytes);
    }
};

test "reads stored and deflated entries from an in-memory archive" {
    const allocator = std.testing.allocator;
    const jar = try TestZipBuilder.build(allocator);
    defer allocator.free(jar);

    const stored = (try readEntryAlloc(allocator, jar, "stored.txt", 1024)).?;
    defer allocator.free(stored);
    try std.testing.expectEqualStrings("stored-content", stored);

    const deflated = (try readEntryAlloc(allocator, jar, "deflated.txt", 1024)).?;
    defer allocator.free(deflated);
    try std.testing.expectEqualStrings("deflated-content-deflated-content", deflated);

    try std.testing.expect((try readEntryAlloc(allocator, jar, "missing.txt", 1024)) == null);
    try std.testing.expectError(error.ZipEntryTooLarge, readEntryAlloc(allocator, jar, "stored.txt", 4));
}

test "rejects truncated archives" {
    try std.testing.expectError(error.ZipNoEndRecord, entries("not a zip"));
}
