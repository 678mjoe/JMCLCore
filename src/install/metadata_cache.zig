const std = @import("std");

pub const directory_name = "metadata-cache";
pub const max_stale_seconds: u64 = 24 * 60 * 60;

const schema_version: u32 = 1;
const envelope_overhead_bytes: usize = 64 * 1024;
const key_name_bytes: usize = 40 + ".json".len;

pub const Policy = union(enum) {
    immutable,
    ttl_seconds: u64,
};

pub const Entry = struct {
    allocator: std.mem.Allocator,
    payload: []u8,
    fresh: bool,

    pub fn deinit(entry: *Entry) void {
        entry.allocator.free(entry.payload);
        entry.* = undefined;
    }
};

const Envelope = struct {
    v: u32,
    url: []const u8,
    fetched_at: i64,
    payload: []const u8,
};

pub fn keyName(url: []const u8, buffer: *[key_name_bytes]u8) []const u8 {
    var digest: [std.crypto.hash.Sha1.digest_length]u8 = undefined;
    std.crypto.hash.Sha1.hash(url, &digest, .{});
    const hex = std.fmt.bytesToHex(digest, .lower);
    @memcpy(buffer[0..40], &hex);
    @memcpy(buffer[40..], ".json");
    return buffer;
}

pub fn lookup(
    allocator: std.mem.Allocator,
    io: std.Io,
    root: std.Io.Dir,
    url: []const u8,
    max_payload_bytes: usize,
    policy: Policy,
    now_epoch_seconds: i64,
) ?Entry {
    var name_buffer: [key_name_bytes]u8 = undefined;
    const name = keyName(url, &name_buffer);
    const max_entry_bytes = std.math.add(usize, max_payload_bytes, envelope_overhead_bytes) catch return null;
    const bytes = root.readFileAlloc(io, name, allocator, .limited(max_entry_bytes + 1)) catch |err| switch (err) {
        error.FileNotFound => return null,
        else => {
            evict(io, root, url);
            return null;
        },
    };
    defer allocator.free(bytes);
    if (bytes.len > max_entry_bytes) {
        evict(io, root, url);
        return null;
    }
    var envelope = std.json.parseFromSlice(Envelope, allocator, bytes, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_entry_bytes,
    }) catch {
        evict(io, root, url);
        return null;
    };
    defer envelope.deinit();
    const value = envelope.value;
    if (value.v != schema_version or !std.mem.eql(u8, value.url, url) or value.payload.len > max_payload_bytes) {
        evict(io, root, url);
        return null;
    }
    const age: u64 = if (value.fetched_at >= now_epoch_seconds) 0 else @intCast(now_epoch_seconds - value.fetched_at);
    const fresh = switch (policy) {
        .immutable => true,
        .ttl_seconds => |ttl| age <= ttl,
    };
    if (!fresh and age > max_stale_seconds) {
        evict(io, root, url);
        return null;
    }
    const payload = allocator.dupe(u8, value.payload) catch return null;
    return .{
        .allocator = allocator,
        .payload = payload,
        .fresh = fresh,
    };
}

pub fn store(
    io: std.Io,
    root: std.Io.Dir,
    url: []const u8,
    payload: []const u8,
    now_epoch_seconds: i64,
) !void {
    var name_buffer: [key_name_bytes]u8 = undefined;
    const name = keyName(url, &name_buffer);
    var atomic_file = try root.createFileAtomic(io, name, .{ .replace = true });
    defer atomic_file.deinit(io);
    var buffer: [4096]u8 = undefined;
    var writer: std.Io.File.Writer = .init(atomic_file.file, io, &buffer);
    var json: std.json.Stringify = .{ .writer = &writer.interface };
    try json.write(Envelope{
        .v = schema_version,
        .url = url,
        .fetched_at = now_epoch_seconds,
        .payload = payload,
    });
    try writer.interface.flush();
    try atomic_file.file.sync(io);
    try atomic_file.replace(io);
}

pub fn evict(io: std.Io, root: std.Io.Dir, url: []const u8) void {
    var name_buffer: [key_name_bytes]u8 = undefined;
    root.deleteFile(io, keyName(url, &name_buffer)) catch {};
}

test "cached entries round-trip and report freshness by policy" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    const root = try temporary.dir.createDirPathOpen(io, "cache", .{});
    defer root.close(io);

    try std.testing.expect(lookup(std.testing.allocator, io, root, "https://example.invalid/a", 1024, .immutable, 1000) == null);
    try store(io, root, "https://example.invalid/a", "{\"ok\":true}", 1000);

    var immutable_entry = lookup(std.testing.allocator, io, root, "https://example.invalid/a", 1024, .immutable, 1000 + max_stale_seconds + 1).?;
    defer immutable_entry.deinit();
    try std.testing.expect(immutable_entry.fresh);
    try std.testing.expectEqualStrings("{\"ok\":true}", immutable_entry.payload);

    var fresh_entry = lookup(std.testing.allocator, io, root, "https://example.invalid/a", 1024, .{ .ttl_seconds = 600 }, 1300).?;
    defer fresh_entry.deinit();
    try std.testing.expect(fresh_entry.fresh);

    var stale_entry = lookup(std.testing.allocator, io, root, "https://example.invalid/a", 1024, .{ .ttl_seconds = 600 }, 1000 + 601).?;
    defer stale_entry.deinit();
    try std.testing.expect(!stale_entry.fresh);

    try std.testing.expect(lookup(std.testing.allocator, io, root, "https://example.invalid/a", 1024, .{ .ttl_seconds = 600 }, 1000 + max_stale_seconds + 1) == null);
}

test "corrupted and foreign entries are evicted and missed" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    const root = try temporary.dir.createDirPathOpen(io, "cache", .{});
    defer root.close(io);

    var name_buffer: [key_name_bytes]u8 = undefined;
    const name = keyName("https://example.invalid/b", &name_buffer);
    try root.writeFile(io, .{ .sub_path = name, .data = "not json" });
    try std.testing.expect(lookup(std.testing.allocator, io, root, "https://example.invalid/b", 1024, .immutable, 1000) == null);
    try std.testing.expectError(error.FileNotFound, root.access(io, name, .{}));

    try store(io, root, "https://example.invalid/b", "payload", 1000);
    try std.testing.expect(lookup(std.testing.allocator, io, root, "https://example.invalid/b", 4, .immutable, 1000) == null);
}

test "cache keys are stable and url-distinct" {
    var first: [key_name_bytes]u8 = undefined;
    var second: [key_name_bytes]u8 = undefined;
    const first_name = keyName("https://example.invalid/x", &first);
    const second_name = keyName("https://example.invalid/y", &second);
    try std.testing.expect(!std.mem.eql(u8, first_name, second_name));
    var repeat: [key_name_bytes]u8 = undefined;
    try std.testing.expectEqualStrings(first_name, keyName("https://example.invalid/x", &repeat));
}
