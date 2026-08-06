const std = @import("std");

pub const max_entries: usize = 65_535;
pub const max_entry_name_bytes: usize = 4096;
pub const max_file_bytes: u64 = 0xFFFF_FFFF;
pub const max_archive_bytes: u64 = 0xFFFF_FFFF;

pub const Error = error{
    TooManyEntries,
    EntryNameTooLong,
    EntryNameInvalid,
    FileTooLarge,
    ArchiveTooLarge,
    OutOfMemory,
};

const local_header_sig: u32 = 0x04034B50;
const central_header_sig: u32 = 0x02014B50;
const end_record_sig: u32 = 0x06054B50;
const version_needed: u16 = 20;
const method_store: u16 = 0;
const method_deflate: u16 = 8;

const CentralRecord = struct {
    name: []u8,
    crc: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    local_offset: u32,
    dos_time: u16,
    dos_date: u16,
    method: u16,
};

pub const DosDateTime = struct {
    time: u16,
    date: u16,
};

pub fn dosDateTime(epoch_seconds: i64) DosDateTime {
    const floor_1980: i64 = 315532800;
    const clamped: u64 = @intCast(@max(epoch_seconds, floor_1980));
    const epoch_seconds_value: std.time.epoch.EpochSeconds = .{ .secs = clamped };
    const epoch_day = epoch_seconds_value.getEpochDay();
    const year_day = epoch_day.calculateYearDay();
    const month_day = year_day.calculateMonthDay();
    const day_seconds = epoch_seconds_value.getDaySeconds();
    const dos_year: u16 = if (year_day.year <= 1980) 0 else @min(year_day.year - 1980, 127);
    return .{
        .date = (dos_year << 9) | (@as(u16, month_day.month.numeric()) << 5) | (@as(u16, month_day.day_index) + 1),
        .time = (@as(u16, day_seconds.getHoursIntoDay()) << 11) |
            (@as(u16, day_seconds.getMinutesIntoHour()) << 5) |
            (@as(u16, day_seconds.getSecondsIntoMinute()) / 2),
    };
}

fn validateEntryName(name: []const u8) Error!void {
    if (name.len == 0 or name.len > max_entry_name_bytes) return error.EntryNameTooLong;
    if (name[0] == '/') return error.EntryNameInvalid;
    for (name) |byte| {
        if (byte == 0 or byte == '\\') return error.EntryNameInvalid;
    }
    var parts = std.mem.splitScalar(u8, name, '/');
    while (parts.next()) |part| {
        if (std.mem.eql(u8, part, "..")) return error.EntryNameInvalid;
    }
}

pub const Writer = struct {
    allocator: std.mem.Allocator,
    io: std.Io,
    atomic_file: std.Io.File.Atomic,
    file_buffer: [8192]u8,
    file_writer: std.Io.File.Writer,
    offset: u64,
    records: std.ArrayList(CentralRecord),
    started: bool,
    finished: bool,

    pub fn create(allocator: std.mem.Allocator, io: std.Io, dir: std.Io.Dir, path: []const u8) !Writer {
        var atomic_file = try dir.createFileAtomic(io, path, .{ .replace = true });
        errdefer atomic_file.deinit(io);
        return .{
            .allocator = allocator,
            .io = io,
            .atomic_file = atomic_file,
            .file_buffer = undefined,
            .file_writer = undefined,
            .offset = 0,
            .records = .empty,
            .started = false,
            .finished = false,
        };
    }

    pub fn abort(self: *Writer) void {
        for (self.records.items) |record| self.allocator.free(record.name);
        self.records.deinit(self.allocator);
        self.atomic_file.deinit(self.io);
        self.* = undefined;
    }

    fn writeRaw(self: *Writer, bytes: []const u8) !void {
        self.offset += bytes.len;
        if (self.offset > max_archive_bytes) return error.ArchiveTooLarge;
        try self.file_writer.interface.writeAll(bytes);
    }

    fn writeInt(self: *Writer, comptime T: type, value: T) !void {
        var buffer: [@sizeOf(T)]u8 = undefined;
        std.mem.writeInt(T, &buffer, value, .little);
        try self.writeRaw(&buffer);
    }

    fn ensureStarted(self: *Writer) void {
        if (self.started) return;
        self.started = true;
        self.file_writer = .init(self.atomic_file.file, self.io, &self.file_buffer);
    }

    fn recordEntry(self: *Writer, name: []const u8, record: CentralRecord) !void {
        if (self.records.items.len >= max_entries) return error.TooManyEntries;
        var owned = record;
        owned.name = try self.allocator.dupe(u8, name);
        try self.records.append(self.allocator, owned);
    }

    pub fn addDirectory(self: *Writer, name: []const u8, epoch_seconds: i64) !void {
        self.ensureStarted();
        if (self.records.items.len >= max_entries) return error.TooManyEntries;
        var name_buffer: [max_entry_name_bytes + 1]u8 = undefined;
        const full = if (name.len == 0 or name[name.len - 1] == '/')
            name
        else
            std.fmt.bufPrint(&name_buffer, "{s}/", .{name}) catch return error.EntryNameTooLong;
        try validateEntryName(full);
        const dt = dosDateTime(epoch_seconds);
        const local_offset = self.offset;
        try self.writeInt(u32, local_header_sig);
        try self.writeInt(u16, version_needed);
        try self.writeInt(u16, 0);
        try self.writeInt(u16, method_store);
        try self.writeInt(u16, dt.time);
        try self.writeInt(u16, dt.date);
        try self.writeInt(u32, 0);
        try self.writeInt(u32, 0);
        try self.writeInt(u32, 0);
        try self.writeInt(u16, @intCast(full.len));
        try self.writeInt(u16, 0);
        try self.writeRaw(full);
        try self.recordEntry(full, .{
            .name = undefined,
            .crc = 0,
            .compressed_size = 0,
            .uncompressed_size = 0,
            .local_offset = std.math.cast(u32, local_offset) orelse return error.ArchiveTooLarge,
            .dos_time = dt.time,
            .dos_date = dt.date,
            .method = method_store,
        });
    }

    pub fn addFile(self: *Writer, name: []const u8, source_dir: std.Io.Dir, source_path: []const u8) !void {
        self.ensureStarted();
        try validateEntryName(name);
        if (self.records.items.len >= max_entries) return error.TooManyEntries;

        const source = try source_dir.openFile(self.io, source_path, .{});
        defer source.close(self.io);
        const stat = try source.stat(self.io);
        if (stat.size > max_file_bytes) return error.FileTooLarge;
        const dt = dosDateTime(stat.mtime.toSeconds());

        var compressed_buffer: std.Io.Writer.Allocating = try .initCapacity(self.allocator, 64 * 1024);
        defer compressed_buffer.deinit();
        var crc = std.hash.Crc32.init();
        var uncompressed: u64 = 0;
        {
            var window: [std.compress.flate.max_window_len]u8 = undefined;
            var compress = try std.compress.flate.Compress.init(&compressed_buffer.writer, &window, .raw, .default);
            var chunk: [16384]u8 = undefined;
            while (true) {
                const read = source.readStreaming(self.io, &.{&chunk}) catch |err| switch (err) {
                    error.EndOfStream => break,
                    else => |other| return other,
                };
                if (read == 0) break;
                crc.update(chunk[0..read]);
                uncompressed += read;
                if (uncompressed > max_file_bytes) return error.FileTooLarge;
                try compress.writer.writeAll(chunk[0..read]);
            }
            try compress.finish();
        }
        var compressed_list = compressed_buffer.toArrayList();
        defer compressed_list.deinit(self.allocator);
        const compressed = compressed_list.items;
        if (compressed.len > max_file_bytes) return error.FileTooLarge;

        const local_offset = self.offset;
        try self.writeInt(u32, local_header_sig);
        try self.writeInt(u16, version_needed);
        try self.writeInt(u16, 0);
        try self.writeInt(u16, method_deflate);
        try self.writeInt(u16, dt.time);
        try self.writeInt(u16, dt.date);
        try self.writeInt(u32, crc.final());
        try self.writeInt(u32, @intCast(compressed.len));
        try self.writeInt(u32, @intCast(uncompressed));
        try self.writeInt(u16, @intCast(name.len));
        try self.writeInt(u16, 0);
        try self.writeRaw(name);
        try self.writeRaw(compressed);
        try self.recordEntry(name, .{
            .name = undefined,
            .crc = crc.final(),
            .compressed_size = @intCast(compressed.len),
            .uncompressed_size = @intCast(uncompressed),
            .local_offset = std.math.cast(u32, local_offset) orelse return error.ArchiveTooLarge,
            .dos_time = dt.time,
            .dos_date = dt.date,
            .method = method_deflate,
        });
    }

    pub fn finish(self: *Writer) !u64 {
        if (self.finished) return error.ArchiveTooLarge;
        self.finished = true;
        self.ensureStarted();
        const cd_offset = self.offset;
        for (self.records.items) |record| {
            try self.writeInt(u32, central_header_sig);
            try self.writeInt(u16, version_needed);
            try self.writeInt(u16, version_needed);
            try self.writeInt(u16, 0);
            try self.writeInt(u16, record.method);
            try self.writeInt(u16, record.dos_time);
            try self.writeInt(u16, record.dos_date);
            try self.writeInt(u32, record.crc);
            try self.writeInt(u32, record.compressed_size);
            try self.writeInt(u32, record.uncompressed_size);
            try self.writeInt(u16, @intCast(record.name.len));
            try self.writeInt(u16, 0);
            try self.writeInt(u16, 0);
            try self.writeInt(u16, 0);
            try self.writeInt(u16, 0);
            try self.writeInt(u32, 0);
            try self.writeInt(u32, record.local_offset);
            try self.writeRaw(record.name);
        }
        const cd_size = self.offset - cd_offset;
        try self.writeInt(u32, end_record_sig);
        try self.writeInt(u16, 0);
        try self.writeInt(u16, 0);
        try self.writeInt(u16, @intCast(self.records.items.len));
        try self.writeInt(u16, @intCast(self.records.items.len));
        try self.writeInt(u32, @intCast(cd_size));
        try self.writeInt(u32, std.math.cast(u32, cd_offset) orelse return error.ArchiveTooLarge);
        try self.writeInt(u16, 0);

        try self.file_writer.interface.flush();
        try self.atomic_file.file.sync(self.io);
        const total = self.offset;
        for (self.records.items) |record| self.allocator.free(record.name);
        self.records.deinit(self.allocator);
        self.records = .empty;
        try self.atomic_file.replace(self.io);
        return total;
    }
};

test "dosDateTime encodes 2024-01-02 03:04:06 UTC" {
    const dt = dosDateTime(1704164646);
    try std.testing.expectEqual(@as(u16, ((2024 - 1980) << 9) | (1 << 5) | 2), dt.date);
    try std.testing.expectEqual(@as(u16, (3 << 11) | (4 << 5) | 3), dt.time);
}

test "zip writer round-trips through zip_reader" {
    const zip_reader = @import("../install/zip_reader.zig");
    const io = std.testing.io;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    try tmp.dir.createDirPath(io, "world/region");
    var region = try tmp.dir.openDir(io, "world/region", .{});
    try region.writeFile(io, .{ .sub_path = "r.0.0.mca", .data = "chunk-data-bytes" });
    region.close(io);
    var world = try tmp.dir.openDir(io, "world", .{});
    try world.writeFile(io, .{ .sub_path = "level.dat", .data = "nbt-bytes" });
    world.close(io);

    var writer = try Writer.create(std.testing.allocator, io, tmp.dir, "out.zip");
    defer writer.abort();
    var world_dir = try tmp.dir.openDir(io, "world", .{ .iterate = true });
    defer world_dir.close(io);
    try writer.addDirectory("world", 1704164646);
    try writer.addFile("world/level.dat", tmp.dir, "world/level.dat");
    try writer.addFile("world/region/r.0.0.mca", tmp.dir, "world/region/r.0.0.mca");
    const total = try writer.finish();
    try std.testing.expect(total > 0);

    const jar = try tmp.dir.readFileAlloc(io, "out.zip", std.testing.allocator, .limited(1 << 20));
    defer std.testing.allocator.free(jar);
    var iterator = try zip_reader.entries(jar);
    var names: std.ArrayList([]const u8) = .empty;
    defer names.deinit(std.testing.allocator);
    while (try iterator.next()) |entry| try names.append(std.testing.allocator, entry.name);
    try std.testing.expectEqual(@as(usize, 3), names.items.len);
    try std.testing.expectEqualStrings("world/", names.items[0]);
    try std.testing.expectEqualStrings("world/level.dat", names.items[1]);
    try std.testing.expectEqualStrings("world/region/r.0.0.mca", names.items[2]);

    const level = try zip_reader.readEntryAlloc(std.testing.allocator, jar, "world/level.dat", 1024);
    defer if (level) |bytes| std.testing.allocator.free(bytes);
    try std.testing.expectEqualStrings("nbt-bytes", level.?);
    const chunk = try zip_reader.readEntryAlloc(std.testing.allocator, jar, "world/region/r.0.0.mca", 1024);
    defer if (chunk) |bytes| std.testing.allocator.free(bytes);
    try std.testing.expectEqualStrings("chunk-data-bytes", chunk.?);
}

test "zip writer rejects traversal names" {
    const io = std.testing.io;
    var tmp = std.testing.tmpDir(.{});
    defer tmp.cleanup();
    var writer = try Writer.create(std.testing.allocator, io, tmp.dir, "bad.zip");
    defer writer.abort();
    try std.testing.expectError(error.EntryNameInvalid, writer.addFile("../evil", tmp.dir, "nope"));
    try std.testing.expectError(error.EntryNameInvalid, writer.addDirectory("/abs", 0));
}
