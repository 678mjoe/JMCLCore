const std = @import("std");

pub const max_payload_bytes: usize = 16 * 1024 * 1024;
pub const max_depth: usize = 64;
pub const max_string_bytes: usize = 1 << 15;
pub const max_items: usize = 1 << 24;

pub const ParseError = error{
    Truncated,
    InvalidTagId,
    DepthExceeded,
    PayloadTooLarge,
    ItemsTooLarge,
    NegativeItems,
    OutOfMemory,
};

pub const tag_end: u8 = 0;
pub const tag_byte: u8 = 1;
pub const tag_short: u8 = 2;
pub const tag_int: u8 = 3;
pub const tag_long: u8 = 4;
pub const tag_float: u8 = 5;
pub const tag_double: u8 = 6;
pub const tag_byte_array: u8 = 7;
pub const tag_string: u8 = 8;
pub const tag_list: u8 = 9;
pub const tag_compound: u8 = 10;
pub const tag_int_array: u8 = 11;
pub const tag_long_array: u8 = 12;

pub const List = struct {
    item_id: u8,
    items: []Tag,
};

pub const Entry = struct {
    name: []const u8,
    tag: Tag,
};

pub const Tag = union(enum) {
    end,
    byte: i8,
    short: i16,
    int: i32,
    long: i64,
    float: f32,
    double: f64,
    byte_array: []u8,
    string: []const u8,
    list: List,
    compound: []Entry,
    int_array: []i32,
    long_array: []i64,

    pub fn id(tag: Tag) u8 {
        return switch (tag) {
            .end => tag_end,
            .byte => tag_byte,
            .short => tag_short,
            .int => tag_int,
            .long => tag_long,
            .float => tag_float,
            .double => tag_double,
            .byte_array => tag_byte_array,
            .string => tag_string,
            .list => tag_list,
            .compound => tag_compound,
            .int_array => tag_int_array,
            .long_array => tag_long_array,
        };
    }

    pub fn eql(a: Tag, b: Tag) bool {
        if (a.id() != b.id()) return false;
        switch (a) {
            .end => return true,
            .byte => |v| return v == b.byte,
            .short => |v| return v == b.short,
            .int => |v| return v == b.int,
            .long => |v| return v == b.long,
            .float => |v| return @as(u32, @bitCast(v)) == @as(u32, @bitCast(b.float)),
            .double => |v| return @as(u64, @bitCast(v)) == @as(u64, @bitCast(b.double)),
            .byte_array => |v| return std.mem.eql(u8, v, b.byte_array),
            .string => |v| return std.mem.eql(u8, v, b.string),
            .int_array => |v| return std.mem.eql(i32, v, b.int_array),
            .long_array => |v| return std.mem.eql(i64, v, b.long_array),
            .list => |v| {
                const other = b.list;
                if (v.item_id != other.item_id or v.items.len != other.items.len) return false;
                for (v.items, other.items) |x, y| {
                    if (!x.eql(y)) return false;
                }
                return true;
            },
            .compound => |v| {
                const other = b.compound;
                if (v.len != other.len) return false;
                for (v, other) |x, y| {
                    if (!std.mem.eql(u8, x.name, y.name) or !x.tag.eql(y.tag)) return false;
                }
                return true;
            },
        }
    }
};

pub fn find(compound: []Entry, name: []const u8) ?*Tag {
    for (compound) |*entry| {
        if (std.mem.eql(u8, entry.name, name)) return &entry.tag;
    }
    return null;
}

pub fn findCompound(compound: []Entry, name: []const u8) ?[]Entry {
    const tag = find(compound, name) orelse return null;
    return switch (tag.*) {
        .compound => |entries| entries,
        else => null,
    };
}

pub fn findString(compound: []Entry, name: []const u8) ?[]const u8 {
    const tag = find(compound, name) orelse return null;
    return switch (tag.*) {
        .string => |value| value,
        else => null,
    };
}

pub fn findInt(compound: []Entry, name: []const u8) ?i32 {
    const tag = find(compound, name) orelse return null;
    return switch (tag.*) {
        .int => |value| value,
        else => null,
    };
}

pub fn findLong(compound: []Entry, name: []const u8) ?i64 {
    const tag = find(compound, name) orelse return null;
    return switch (tag.*) {
        .long => |value| value,
        else => null,
    };
}

pub fn findByte(compound: []Entry, name: []const u8) ?i8 {
    const tag = find(compound, name) orelse return null;
    return switch (tag.*) {
        .byte => |value| value,
        else => null,
    };
}

pub fn setString(compound: []Entry, name: []const u8, value: []const u8) bool {
    const tag = find(compound, name) orelse return false;
    switch (tag.*) {
        .string => |*current| {
            current.* = value;
            return true;
        },
        else => return false,
    }
}

const Cursor = struct {
    data: []const u8,
    pos: usize = 0,

    fn byte(cursor: *Cursor) ParseError!u8 {
        if (cursor.pos >= cursor.data.len) return error.Truncated;
        const value = cursor.data[cursor.pos];
        cursor.pos += 1;
        return value;
    }

    fn intBe(cursor: *Cursor, comptime T: type) ParseError!T {
        const size = @sizeOf(T);
        if (cursor.data.len - cursor.pos < size) return error.Truncated;
        const value = std.mem.readInt(T, cursor.data[cursor.pos..][0..size], .big);
        cursor.pos += size;
        return value;
    }

    fn slice(cursor: *Cursor, len: usize) ParseError![]const u8 {
        if (cursor.data.len - cursor.pos < len) return error.Truncated;
        const value = cursor.data[cursor.pos..][0..len];
        cursor.pos += len;
        return value;
    }
};

fn parsePayload(
    cursor: *Cursor,
    allocator: std.mem.Allocator,
    tag_id: u8,
    depth: usize,
) ParseError!Tag {
    if (depth > max_depth) return error.DepthExceeded;
    switch (tag_id) {
        tag_end => return .end,
        tag_byte => return .{ .byte = @bitCast(try cursor.byte()) },
        tag_short => return .{ .short = try cursor.intBe(i16) },
        tag_int => return .{ .int = try cursor.intBe(i32) },
        tag_long => return .{ .long = try cursor.intBe(i64) },
        tag_float => return .{ .float = @bitCast(try cursor.intBe(u32)) },
        tag_double => return .{ .double = @bitCast(try cursor.intBe(u64)) },
        tag_byte_array => {
            const len = try cursor.intBe(i32);
            if (len < 0) return error.NegativeItems;
            const count: usize = @intCast(len);
            if (count > max_items) return error.ItemsTooLarge;
            const bytes = try cursor.slice(count);
            return .{ .byte_array = try allocator.dupe(u8, bytes) };
        },
        tag_string => {
            const len = try cursor.intBe(u16);
            const bytes = try cursor.slice(len);
            return .{ .string = try allocator.dupe(u8, bytes) };
        },
        tag_list => {
            const item_id = try cursor.byte();
            const len = try cursor.intBe(i32);
            if (len < 0) return error.NegativeItems;
            const count: usize = @intCast(len);
            if (count > max_items) return error.ItemsTooLarge;
            if (count == 0) return .{ .list = .{ .item_id = item_id, .items = &.{} } };
            if (item_id == tag_end or item_id > tag_long_array) return error.InvalidTagId;
            const items = try allocator.alloc(Tag, count);
            for (items) |*item| item.* = try parsePayload(cursor, allocator, item_id, depth + 1);
            return .{ .list = .{ .item_id = item_id, .items = items } };
        },
        tag_compound => {
            var entries: std.ArrayList(Entry) = .empty;
            while (true) {
                const entry_id = try cursor.byte();
                if (entry_id == tag_end) break;
                if (entry_id > tag_long_array) return error.InvalidTagId;
                const name_len = try cursor.intBe(u16);
                const name = try allocator.dupe(u8, try cursor.slice(name_len));
                const tag = try parsePayload(cursor, allocator, entry_id, depth + 1);
                try entries.append(allocator, .{ .name = name, .tag = tag });
            }
            return .{ .compound = try entries.toOwnedSlice(allocator) };
        },
        tag_int_array => {
            const len = try cursor.intBe(i32);
            if (len < 0) return error.NegativeItems;
            const count: usize = @intCast(len);
            if (count > max_items) return error.ItemsTooLarge;
            const values = try allocator.alloc(i32, count);
            for (values) |*value| value.* = try cursor.intBe(i32);
            return .{ .int_array = values };
        },
        tag_long_array => {
            const len = try cursor.intBe(i32);
            if (len < 0) return error.NegativeItems;
            const count: usize = @intCast(len);
            if (count > max_items) return error.ItemsTooLarge;
            const values = try allocator.alloc(i64, count);
            for (values) |*value| value.* = try cursor.intBe(i64);
            return .{ .long_array = values };
        },
        else => return error.InvalidTagId,
    }
}

pub const Document = struct {
    name: []const u8,
    root: []Entry,
};

pub fn parse(allocator: std.mem.Allocator, data: []const u8) ParseError!Document {
    if (data.len > max_payload_bytes) return error.PayloadTooLarge;
    var cursor: Cursor = .{ .data = data };
    const root_id = try cursor.byte();
    if (root_id != tag_compound) return error.InvalidTagId;
    const name_len = try cursor.intBe(u16);
    const name = try allocator.dupe(u8, try cursor.slice(name_len));
    const root = try parsePayload(&cursor, allocator, tag_compound, 1);
    if (cursor.pos != data.len) return error.Truncated;
    return .{ .name = name, .root = root.compound };
}

fn writeStringRaw(out: *std.ArrayList(u8), allocator: std.mem.Allocator, value: []const u8) ParseError!void {
    if (value.len > max_string_bytes) return error.PayloadTooLarge;
    var len_buffer: [2]u8 = undefined;
    std.mem.writeInt(u16, &len_buffer, @intCast(value.len), .big);
    try out.appendSlice(allocator, &len_buffer);
    try out.appendSlice(allocator, value);
}

fn writePayload(out: *std.ArrayList(u8), allocator: std.mem.Allocator, tag: Tag) ParseError!void {
    switch (tag) {
        .end => {},
        .byte => |value| try out.append(allocator, @bitCast(value)),
        .short => |value| {
            var buffer: [2]u8 = undefined;
            std.mem.writeInt(i16, &buffer, value, .big);
            try out.appendSlice(allocator, &buffer);
        },
        .int => |value| {
            var buffer: [4]u8 = undefined;
            std.mem.writeInt(i32, &buffer, value, .big);
            try out.appendSlice(allocator, &buffer);
        },
        .long => |value| {
            var buffer: [8]u8 = undefined;
            std.mem.writeInt(i64, &buffer, value, .big);
            try out.appendSlice(allocator, &buffer);
        },
        .float => |value| {
            var buffer: [4]u8 = undefined;
            std.mem.writeInt(u32, &buffer, @bitCast(value), .big);
            try out.appendSlice(allocator, &buffer);
        },
        .double => |value| {
            var buffer: [8]u8 = undefined;
            std.mem.writeInt(u64, &buffer, @bitCast(value), .big);
            try out.appendSlice(allocator, &buffer);
        },
        .byte_array => |values| {
            if (values.len > max_items) return error.ItemsTooLarge;
            var buffer: [4]u8 = undefined;
            std.mem.writeInt(i32, &buffer, @intCast(values.len), .big);
            try out.appendSlice(allocator, &buffer);
            try out.appendSlice(allocator, values);
        },
        .string => |value| try writeStringRaw(out, allocator, value),
        .list => |list| {
            if (list.items.len > max_items) return error.ItemsTooLarge;
            try out.append(allocator, if (list.items.len == 0) tag_end else list.item_id);
            var buffer: [4]u8 = undefined;
            std.mem.writeInt(i32, &buffer, @intCast(list.items.len), .big);
            try out.appendSlice(allocator, &buffer);
            for (list.items) |item| try writePayload(out, allocator, item);
        },
        .compound => |entries| {
            for (entries) |entry| {
                if (entry.tag == .end) continue;
                try out.append(allocator, entry.tag.id());
                try writeStringRaw(out, allocator, entry.name);
                try writePayload(out, allocator, entry.tag);
            }
            try out.append(allocator, tag_end);
        },
        .int_array => |values| {
            if (values.len > max_items) return error.ItemsTooLarge;
            var buffer: [4]u8 = undefined;
            std.mem.writeInt(i32, &buffer, @intCast(values.len), .big);
            try out.appendSlice(allocator, &buffer);
            for (values) |value| {
                std.mem.writeInt(i32, &buffer, value, .big);
                try out.appendSlice(allocator, &buffer);
            }
        },
        .long_array => |values| {
            if (values.len > max_items) return error.ItemsTooLarge;
            var buffer: [4]u8 = undefined;
            std.mem.writeInt(i32, &buffer, @intCast(values.len), .big);
            try out.appendSlice(allocator, &buffer);
            var wide: [8]u8 = undefined;
            for (values) |value| {
                std.mem.writeInt(i64, &wide, value, .big);
                try out.appendSlice(allocator, &wide);
            }
        },
    }
}

pub fn serialize(allocator: std.mem.Allocator, document: Document) ParseError![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(allocator);
    try out.append(allocator, tag_compound);
    try writeStringRaw(&out, allocator, document.name);
    try writePayload(&out, allocator, .{ .compound = document.root });
    return out.toOwnedSlice(allocator);
}

test "parse and serialize round-trip preserves every tag type" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const allocator = arena.allocator();

    var longs = [_]i64{ 1, -2, 0x7fffffffffffffff };
    var ints = [_]i32{ 3, -4 };
    var bytes = [_]u8{ 0, 1, 255 };
    var list_items = [_]Tag{ .{ .int = 7 }, .{ .int = -8 } };
    var nested_entries = [_]Entry{
        .{ .name = "inner", .tag = .{ .string = "value" } },
    };
    var root_entries = [_]Entry{
        .{ .name = "b", .tag = .{ .byte = -5 } },
        .{ .name = "s", .tag = .{ .short = 300 } },
        .{ .name = "i", .tag = .{ .int = 70000 } },
        .{ .name = "l", .tag = .{ .long = -9000000000 } },
        .{ .name = "f", .tag = .{ .float = 1.5 } },
        .{ .name = "d", .tag = .{ .double = -2.25 } },
        .{ .name = "ba", .tag = .{ .byte_array = &bytes } },
        .{ .name = "str", .tag = .{ .string = "héllo" } },
        .{ .name = "list", .tag = .{ .list = .{ .item_id = tag_int, .items = &list_items } } },
        .{ .name = "nested", .tag = .{ .compound = &nested_entries } },
        .{ .name = "ia", .tag = .{ .int_array = &ints } },
        .{ .name = "la", .tag = .{ .long_array = &longs } },
    };
    const document: Document = .{ .name = "", .root = &root_entries };

    const encoded = try serialize(allocator, document);
    const decoded = try parse(allocator, encoded);
    try std.testing.expectEqualStrings("", decoded.name);
    const expected: Tag = .{ .compound = document.root };
    const actual: Tag = .{ .compound = decoded.root };
    try std.testing.expect(expected.eql(actual));
}

test "parse rejects truncated and trailing data" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const allocator = arena.allocator();

    try std.testing.expectError(error.Truncated, parse(allocator, &[_]u8{ tag_compound, 0 }));

    var entries = [_]Entry{};
    const document: Document = .{ .name = "", .root = &entries };
    const encoded = try serialize(allocator, document);
    const padded = try allocator.dupe(u8, encoded);
    const with_trailing = try allocator.alloc(u8, padded.len + 1);
    @memcpy(with_trailing[0..padded.len], padded);
    with_trailing[padded.len] = 0;
    try std.testing.expectError(error.Truncated, parse(allocator, with_trailing));
}

test "parse rejects invalid tag ids and negative lengths" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const allocator = arena.allocator();

    // Root tag id 99 is invalid.
    try std.testing.expectError(error.InvalidTagId, parse(allocator, &[_]u8{ 99, 0, 0, tag_end }));
    // Byte array with negative length.
    const negative = [_]u8{ tag_compound, 0, 0, tag_byte_array, 0, 1, 0xff, 0xff, 0xff, 0xff, tag_end };
    try std.testing.expectError(error.NegativeItems, parse(allocator, &negative));
}

test "find helpers navigate compounds" {
    var child = [_]Entry{.{ .name = "Name", .tag = .{ .string = "1.20.1" } }};
    var root = [_]Entry{
        .{ .name = "LevelName", .tag = .{ .string = "World" } },
        .{ .name = "Version", .tag = .{ .compound = &child } },
        .{ .name = "GameType", .tag = .{ .int = 1 } },
        .{ .name = "LastPlayed", .tag = .{ .long = 42 } },
        .{ .name = "hardcore", .tag = .{ .byte = 0 } },
    };
    try std.testing.expectEqualStrings("World", findString(&root, "LevelName").?);
    try std.testing.expectEqual(@as(?i32, 1), findInt(&root, "GameType"));
    try std.testing.expectEqual(@as(?i64, 42), findLong(&root, "LastPlayed"));
    try std.testing.expectEqual(@as(?i8, 0), findByte(&root, "hardcore"));
    try std.testing.expectEqualStrings("1.20.1", findString(findCompound(&root, "Version").?, "Name").?);
    try std.testing.expect(findString(&root, "GameType") == null);
    try std.testing.expect(setString(&root, "LevelName", "Renamed"));
    try std.testing.expectEqualStrings("Renamed", findString(&root, "LevelName").?);
    try std.testing.expect(!setString(&root, "GameType", "x"));
}
