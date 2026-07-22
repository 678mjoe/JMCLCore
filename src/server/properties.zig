const std = @import("std");

pub const relative_path = "server.properties";
pub const eula_relative_path = "eula.txt";
pub const max_file_bytes: usize = 256 * 1024;
pub const max_entries: usize = 1024;
pub const max_key_bytes: usize = 256;
pub const max_value_bytes: usize = 64 * 1024;

pub const default_content =
    \\#Minecraft server properties
    \\difficulty=easy
    \\enable-rcon=false
    \\gamemode=survival
    \\level-name=world
    \\max-players=20
    \\motd=A Minecraft Server
    \\online-mode=true
    \\server-ip=
    \\server-port=25565
    \\view-distance=10
    \\
;

pub const eula_content =
    \\# By changing the setting below to TRUE you indicate your agreement to the Minecraft EULA (https://aka.ms/MinecraftEULA).
    \\eula=true
    \\
;

pub const Entry = struct {
    key: []const u8,
    value: []const u8,
};

pub const Document = struct {
    parent_allocator: std.mem.Allocator,
    arena: *std.heap.ArenaAllocator,
    entries: std.ArrayList(Entry),

    pub fn deinit(document: *Document) void {
        const parent_allocator = document.parent_allocator;
        document.arena.deinit();
        parent_allocator.destroy(document.arena);
        document.* = undefined;
    }

    pub fn set(document: *Document, key: []const u8, value: []const u8) !void {
        try validateKey(key);
        try validateValue(value);
        const allocator = document.arena.allocator();
        const owned_value = try allocator.dupe(u8, value);
        for (document.entries.items) |*entry| {
            if (!std.mem.eql(u8, entry.key, key)) continue;
            entry.value = owned_value;
            return;
        }
        if (document.entries.items.len == max_entries) return error.TooManyServerProperties;
        try document.entries.append(allocator, .{
            .key = try allocator.dupe(u8, key),
            .value = owned_value,
        });
    }
};

pub fn load(allocator: std.mem.Allocator, io: std.Io, server_dir: std.Io.Dir) !Document {
    const bytes = server_dir.readFileAlloc(
        io,
        relative_path,
        allocator,
        .limited(max_file_bytes + 1),
    ) catch |err| switch (err) {
        error.StreamTooLong => return error.ServerPropertiesTooLarge,
        else => |other| return other,
    };
    defer allocator.free(bytes);
    if (bytes.len > max_file_bytes) return error.ServerPropertiesTooLarge;
    return parse(allocator, bytes);
}

pub fn parse(allocator: std.mem.Allocator, bytes: []const u8) !Document {
    if (bytes.len > max_file_bytes) return error.ServerPropertiesTooLarge;
    const arena = try allocator.create(std.heap.ArenaAllocator);
    errdefer allocator.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(allocator);
    errdefer arena.deinit();
    const scratch = arena.allocator();
    var document: Document = .{
        .parent_allocator = allocator,
        .arena = arena,
        .entries = .empty,
    };
    errdefer document.entries.deinit(scratch);
    var logical: std.ArrayList(u8) = .empty;
    var continuing = false;
    var lines = std.mem.splitScalar(u8, bytes, '\n');
    while (lines.next()) |source_line| {
        const line = if (source_line.len > 0 and source_line[source_line.len - 1] == '\r')
            source_line[0 .. source_line.len - 1]
        else
            source_line;
        const part = if (continuing) std.mem.trimStart(u8, line, " \t\x0c") else line;
        try logical.appendSlice(scratch, part);
        if (hasContinuation(logical.items)) {
            _ = logical.pop();
            continuing = true;
            continue;
        }
        continuing = false;
        try parseLogicalLine(&document, logical.items);
        logical.clearRetainingCapacity();
    }
    if (continuing) try parseLogicalLine(&document, logical.items);
    return document;
}

pub fn ensureDefault(io: std.Io, server_dir: std.Io.Dir) !bool {
    const stat = server_dir.statFile(io, relative_path, .{}) catch |err| switch (err) {
        error.FileNotFound => null,
        else => |other| return other,
    };
    if (stat) |existing| {
        if (existing.kind != .file) return error.InvalidServerPropertiesFile;
        return false;
    }
    try writeBytesAtomic(io, server_dir, relative_path, default_content);
    return true;
}

pub fn writeEula(io: std.Io, server_dir: std.Io.Dir) !void {
    try writeBytesAtomic(io, server_dir, eula_relative_path, eula_content);
}

pub fn writeAtomic(io: std.Io, server_dir: std.Io.Dir, document: Document) !void {
    var atomic_file = try server_dir.createFileAtomic(io, relative_path, .{ .replace = true });
    defer atomic_file.deinit(io);
    var buffer: [4096]u8 = undefined;
    var writer: std.Io.File.Writer = .init(atomic_file.file, io, &buffer);
    try writer.interface.writeAll("#Minecraft server properties\n");
    for (document.entries.items) |entry| {
        try writeEscaped(&writer.interface, entry.key, true);
        try writer.interface.writeByte('=');
        try writeEscaped(&writer.interface, entry.value, false);
        try writer.interface.writeByte('\n');
    }
    try writer.interface.flush();
    try atomic_file.file.sync(io);
    try atomic_file.replace(io);
}

fn parseLogicalLine(document: *Document, line: []const u8) !void {
    var index: usize = 0;
    while (index < line.len and isPropertyWhitespace(line[index])) : (index += 1) {}
    if (index == line.len or line[index] == '#' or line[index] == '!') return;
    const key_start = index;
    var escaped = false;
    while (index < line.len) : (index += 1) {
        const byte = line[index];
        if (!escaped and (byte == '=' or byte == ':' or isPropertyWhitespace(byte))) break;
        if (byte == '\\' and !escaped) {
            escaped = true;
        } else {
            escaped = false;
        }
    }
    const key_end = index;
    while (index < line.len and isPropertyWhitespace(line[index])) : (index += 1) {}
    if (index < line.len and (line[index] == '=' or line[index] == ':')) index += 1;
    while (index < line.len and isPropertyWhitespace(line[index])) : (index += 1) {}
    const allocator = document.arena.allocator();
    const key = try decode(allocator, line[key_start..key_end]);
    const value = try decode(allocator, line[index..]);
    try document.set(key, value);
}

fn decode(allocator: std.mem.Allocator, raw: []const u8) ![]const u8 {
    var decoded: std.ArrayList(u8) = .empty;
    var index: usize = 0;
    while (index < raw.len) {
        if (raw[index] != '\\') {
            try decoded.append(allocator, raw[index]);
            index += 1;
            continue;
        }
        index += 1;
        if (index == raw.len) {
            try decoded.append(allocator, '\\');
            break;
        }
        const escaped = raw[index];
        index += 1;
        switch (escaped) {
            't' => try decoded.append(allocator, '\t'),
            'n' => try decoded.append(allocator, '\n'),
            'r' => try decoded.append(allocator, '\r'),
            'f' => try decoded.append(allocator, 0x0c),
            'u' => {
                const codepoint = try decodeUnicodeEscape(raw, &index);
                var buffer: [4]u8 = undefined;
                const length = std.unicode.utf8Encode(codepoint, &buffer) catch return error.InvalidServerProperties;
                try decoded.appendSlice(allocator, buffer[0..length]);
            },
            else => try decoded.append(allocator, escaped),
        }
    }
    return decoded.toOwnedSlice(allocator);
}

fn decodeUnicodeEscape(raw: []const u8, index: *usize) !u21 {
    const first = try decodeHexCodeUnit(raw, index);
    if (first >= 0xD800 and first <= 0xDBFF) {
        if (index.* + 2 > raw.len or raw[index.*] != '\\' or raw[index.* + 1] != 'u') {
            return error.InvalidServerProperties;
        }
        index.* += 2;
        const second = try decodeHexCodeUnit(raw, index);
        if (second < 0xDC00 or second > 0xDFFF) return error.InvalidServerProperties;
        return @intCast(0x10000 + ((@as(u32, first) - 0xD800) << 10) + (@as(u32, second) - 0xDC00));
    }
    if (first >= 0xDC00 and first <= 0xDFFF) return error.InvalidServerProperties;
    return @intCast(first);
}

fn decodeHexCodeUnit(raw: []const u8, index: *usize) !u16 {
    if (index.* + 4 > raw.len) return error.InvalidServerProperties;
    var value: u16 = 0;
    for (raw[index.* .. index.* + 4]) |byte| {
        value = std.math.mul(u16, value, 16) catch unreachable;
        value = std.math.add(u16, value, std.fmt.charToDigit(byte, 16) catch return error.InvalidServerProperties) catch unreachable;
    }
    index.* += 4;
    return value;
}

fn writeEscaped(writer: *std.Io.Writer, value: []const u8, key: bool) !void {
    for (value, 0..) |byte, index| {
        switch (byte) {
            '\\' => try writer.writeAll("\\\\"),
            '\t' => try writer.writeAll("\\t"),
            '\n' => try writer.writeAll("\\n"),
            '\r' => try writer.writeAll("\\r"),
            0x0c => try writer.writeAll("\\f"),
            ' ' => if (key or index == 0) try writer.writeAll("\\ ") else try writer.writeByte(byte),
            '=', ':' => if (key) {
                try writer.writeByte('\\');
                try writer.writeByte(byte);
            } else try writer.writeByte(byte),
            '#', '!' => if (key or index == 0) {
                try writer.writeByte('\\');
                try writer.writeByte(byte);
            } else try writer.writeByte(byte),
            else => try writer.writeByte(byte),
        }
    }
}

fn writeBytesAtomic(io: std.Io, server_dir: std.Io.Dir, path: []const u8, bytes: []const u8) !void {
    var atomic_file = try server_dir.createFileAtomic(io, path, .{ .replace = true });
    defer atomic_file.deinit(io);
    try atomic_file.file.writeStreamingAll(io, bytes);
    try atomic_file.file.sync(io);
    try atomic_file.replace(io);
}

fn hasContinuation(line: []const u8) bool {
    var backslashes: usize = 0;
    var index = line.len;
    while (index > 0 and line[index - 1] == '\\') : (index -= 1) backslashes += 1;
    return backslashes % 2 == 1;
}

fn isPropertyWhitespace(byte: u8) bool {
    return byte == ' ' or byte == '\t' or byte == 0x0c;
}

fn validateKey(key: []const u8) !void {
    if (key.len == 0 or key.len > max_key_bytes or !std.unicode.utf8ValidateSlice(key)) {
        return error.InvalidServerPropertyKey;
    }
    if (std.mem.indexOfAny(u8, key, "\x00\r\n") != null) return error.InvalidServerPropertyKey;
}

fn validateValue(value: []const u8) !void {
    if (value.len > max_value_bytes or !std.unicode.utf8ValidateSlice(value)) {
        return error.InvalidServerPropertyValue;
    }
    if (std.mem.indexOfScalar(u8, value, 0) != null) return error.InvalidServerPropertyValue;
}

fn expectProperty(document: Document, key: []const u8, expected: []const u8) !void {
    for (document.entries.items) |entry| {
        if (std.mem.eql(u8, entry.key, key)) {
            try std.testing.expectEqualStrings(expected, entry.value);
            return;
        }
    }
    return error.TestExpectedEqual;
}

test "server properties parse Java escapes continuations and duplicate keys" {
    const source =
        \\  # ignored
        \\escaped\ key=value\ with\ spaces
        \\continued=first\
        \\  second
        \\unicode=\u4f60\u597d\uD83D\uDE00
        \\duplicate=first
        \\duplicate:second
        \\
    ;
    var document = try parse(std.testing.allocator, source);
    defer document.deinit();
    try std.testing.expectEqual(@as(usize, 4), document.entries.items.len);
    try expectProperty(document, "escaped key", "value with spaces");
    try expectProperty(document, "continued", "firstsecond");
    try expectProperty(document, "unicode", "你好😀");
    try expectProperty(document, "duplicate", "second");
}

test "server properties reject malformed Unicode and invalid UTF-8" {
    try std.testing.expectError(
        error.InvalidServerProperties,
        parse(std.testing.allocator, "motd=\\uD83Dbroken\n"),
    );
    try std.testing.expectError(
        error.InvalidServerPropertyValue,
        parse(std.testing.allocator, "motd=\xff\n"),
    );
}

test "server properties defaults atomic updates and EULA are stable" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try std.testing.expect(try ensureDefault(io, temporary.dir));
    try std.testing.expect(!try ensureDefault(io, temporary.dir));
    var document = try load(std.testing.allocator, io, temporary.dir);
    defer document.deinit();
    try expectProperty(document, "motd", "A Minecraft Server");
    try document.set("motd", "JMCL Core 世界");
    try document.set("server-port", "25566");
    try writeAtomic(io, temporary.dir, document);
    var reloaded = try load(std.testing.allocator, io, temporary.dir);
    defer reloaded.deinit();
    try expectProperty(reloaded, "motd", "JMCL Core 世界");
    try expectProperty(reloaded, "server-port", "25566");
    try writeEula(io, temporary.dir);
    const eula = try temporary.dir.readFileAlloc(
        io,
        eula_relative_path,
        std.testing.allocator,
        .limited(1024),
    );
    defer std.testing.allocator.free(eula);
    try std.testing.expectEqualStrings(eula_content, eula);
}
