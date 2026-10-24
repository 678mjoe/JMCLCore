//! Bounded GameSpy4 Query client for an IPv4 loopback Minecraft server.
const std = @import("std");
const builtin = @import("builtin");

pub const max_datagram_bytes: usize = 16 * 1024;
pub const max_string_bytes: usize = 1024;
pub const max_players: usize = 256;
pub const deadline_ms: i64 = 3_000;

pub const Mode = enum { basic, full };

pub const Result = struct {
    arena: *std.heap.ArenaAllocator,
    mode: Mode,
    hostname: []const u8,
    game_type: []const u8,
    map: []const u8,
    version: ?[]const u8,
    players: u32,
    max_players: u32,
    host_port: u16,
    host_ip: []const u8,
    plugins: ?[]const u8,
    player_names: []const []const u8,

    pub fn deinit(result: *Result) void {
        const allocator = result.arena.child_allocator;
        result.arena.deinit();
        allocator.destroy(result.arena);
        result.* = undefined;
    }
};

/// The service supplies the property-derived port. No host is accepted here:
/// the client always sends to 127.0.0.1 and rejects a reply from any other peer.
pub fn execute(allocator: std.mem.Allocator, io: std.Io, port: u16, mode: Mode) !Result {
    const target: std.Io.net.IpAddress = .{ .ip4 = .loopback(port) };
    const local: std.Io.net.IpAddress = .{ .ip4 = .loopback(0) };
    var socket = std.Io.net.IpAddress.bind(&local, io, .{ .mode = .dgram, .protocol = .udp }) catch return error.TransportUnavailable;
    defer socket.close(io);
    const session = nextSession(io);
    var handshake: [7]u8 = .{ 0xfe, 0xfd, 0x09, session[0], session[1], session[2], session[3] };
    socket.send(io, &target, &handshake) catch return error.TransportUnavailable;
    var buffer: [max_datagram_bytes + 1]u8 = undefined;
    const timeout: std.Io.Timeout = .{ .duration = .{ .raw = std.Io.Duration.fromMilliseconds(deadline_ms), .clock = .awake } };
    const challenge_reply = socket.receiveTimeout(io, &buffer, timeout) catch return error.TransportUnavailable;
    if (!challenge_reply.from.eql(&target) or challenge_reply.flags.trunc) return error.ProtocolError;
    const challenge = try parseChallenge(challenge_reply.data, session);
    var request: [15]u8 = undefined;
    request[0] = 0xfe;
    request[1] = 0xfd;
    request[2] = 0x00;
    @memcpy(request[3..7], &session);
    std.mem.writeInt(i32, request[7..11], challenge, .big);
    const request_len: usize = switch (mode) {
        .basic => 11,
        .full => 15,
    };
    if (mode == .full) @memset(request[11..15], 0);
    socket.send(io, &target, request[0..request_len]) catch return error.TransportUnavailable;
    const reply = socket.receiveTimeout(io, &buffer, timeout) catch return error.TransportUnavailable;
    if (!reply.from.eql(&target) or reply.flags.trunc) return error.ProtocolError;
    return parseStatus(allocator, reply.data, session, mode);
}

fn nextSession(io: std.Io) [4]u8 {
    const milliseconds = std.Io.Timestamp.now(io, .awake).toMilliseconds();
    const value: u32 = @truncate(@as(u64, @bitCast(milliseconds)));
    // Old Query implementations expect each session nibble to be in 0..15.
    return .{ @as(u8, @truncate(value >> 24)) & 0x0f, @as(u8, @truncate(value >> 16)) & 0x0f, @as(u8, @truncate(value >> 8)) & 0x0f, @as(u8, @truncate(value)) & 0x0f };
}

fn parseChallenge(packet: []const u8, session: [4]u8) !i32 {
    if (packet.len < 7 or packet[0] != 0x09 or !std.mem.eql(u8, packet[1..5], &session)) return error.ProtocolError;
    const end = std.mem.indexOfScalar(u8, packet[5..], 0) orelse return error.ProtocolError;
    const token = packet[5 .. 5 + end];
    if (token.len == 0 or token.len > 16) return error.ProtocolError;
    for (token) |byte| if (!(byte == '-' or (byte >= '0' and byte <= '9'))) return error.ProtocolError;
    const value = std.fmt.parseInt(i32, token, 10) catch return error.ProtocolError;
    if (5 + end + 1 != packet.len) return error.ProtocolError;
    return value;
}

fn parseStatus(allocator: std.mem.Allocator, packet: []const u8, session: [4]u8, mode: Mode) !Result {
    if (packet.len > max_datagram_bytes or packet.len < 7 or packet[0] != 0x00 or !std.mem.eql(u8, packet[1..5], &session)) return error.ProtocolError;
    const arena = try allocator.create(std.heap.ArenaAllocator);
    errdefer allocator.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(allocator);
    errdefer arena.deinit();
    const scratch = arena.allocator();
    return switch (mode) {
        .basic => parseBasic(scratch, arena, packet[5..]),
        .full => parseFull(scratch, arena, packet[5..]),
    };
}

fn parseBasic(allocator: std.mem.Allocator, arena: *std.heap.ArenaAllocator, body: []const u8) !Result {
    var index: usize = 0;
    const hostname = try takeString(allocator, body, &index);
    const game_type = try takeString(allocator, body, &index);
    const map = try takeString(allocator, body, &index);
    const players = try parseDecimal(try takeString(allocator, body, &index));
    const max_players_value = try parseDecimal(try takeString(allocator, body, &index));
    if (players > max_players_value or players > max_players or max_players_value > max_players) return error.ProtocolError;
    if (index + 2 > body.len) return error.ProtocolError;
    const host_port = std.mem.readInt(u16, @ptrCast(body[index .. index + 2]), .little);
    if (host_port == 0) return error.ProtocolError;
    index += 2;
    const host_ip = try takeString(allocator, body, &index);
    if (index != body.len) return error.ProtocolError;
    return .{ .arena = arena, .mode = .basic, .hostname = hostname, .game_type = game_type, .map = map, .version = null, .players = players, .max_players = max_players_value, .host_port = host_port, .host_ip = host_ip, .plugins = null, .player_names = &.{} };
}

fn parseFull(allocator: std.mem.Allocator, arena: *std.heap.ArenaAllocator, body: []const u8) !Result {
    // GameSpy4 full status: ASCII splitnum, NUL, 0x80, NUL.
    // The leading type+session is consumed by parseStatus.
    if (body.len < 11 or !std.mem.eql(u8, body[0..11], "splitnum\x00\x80\x00")) return error.ProtocolError;
    var index: usize = 11;
    var hostname: ?[]const u8 = null;
    var game_type: ?[]const u8 = null;
    var map: ?[]const u8 = null;
    var version: ?[]const u8 = null;
    var players: ?u32 = null;
    var max_players_value: ?u32 = null;
    var host_port: ?u16 = null;
    var host_ip: ?[]const u8 = null;
    var plugins: ?[]const u8 = null;
    var game_id_seen = false;
    while (true) {
        const key = try takeString(allocator, body, &index);
        if (key.len == 0) break;
        const value = try takeString(allocator, body, &index);
        if (std.mem.eql(u8, key, "hostname") and hostname == null) hostname = value else if (std.mem.eql(u8, key, "gametype") and game_type == null) game_type = value else if (std.mem.eql(u8, key, "map") and map == null) map = value else if (std.mem.eql(u8, key, "version") and version == null) version = value else if (std.mem.eql(u8, key, "numplayers") and players == null) players = try parseDecimal(value) else if (std.mem.eql(u8, key, "maxplayers") and max_players_value == null) max_players_value = try parseDecimal(value) else if (std.mem.eql(u8, key, "hostport") and host_port == null) {
            const port_value = try parseDecimal(value);
            if (port_value == 0 or port_value > std.math.maxInt(u16)) return error.ProtocolError;
            host_port = @intCast(port_value);
        } else if (std.mem.eql(u8, key, "hostip") and host_ip == null) host_ip = value else if (std.mem.eql(u8, key, "plugins") and plugins == null) plugins = value
            // Vanilla sends game_id=MINECRAFT. It is protocol metadata, not a
            // user-facing arbitrary field, so validate it and intentionally omit it.
        else if (std.mem.eql(u8, key, "game_id") and !game_id_seen and std.mem.eql(u8, value, "MINECRAFT")) {
            game_id_seen = true;
        } else return error.ProtocolError;
    }
    if (index >= body.len or body[index] != 1) return error.ProtocolError;
    index += 1;
    if (!std.mem.startsWith(u8, body[index..], "player_\x00")) return error.ProtocolError;
    index += "player_\x00".len;
    // Vanilla's full response has the required empty player-section
    // separator before the NUL-terminated name list.
    if (index >= body.len or body[index] != 0) return error.ProtocolError;
    index += 1;
    var names: std.ArrayList([]const u8) = .empty;
    while (true) {
        const name = try takeString(allocator, body, &index);
        if (name.len == 0) break;
        if (names.items.len == max_players) return error.ProtocolError;
        try names.append(allocator, name);
    }
    if (index != body.len or hostname == null or game_type == null or map == null or players == null or max_players_value == null or host_port == null or host_ip == null) return error.ProtocolError;
    if (players.? > max_players_value.? or players.? > max_players or max_players_value.? > max_players or names.items.len > players.?) return error.ProtocolError;
    return .{ .arena = arena, .mode = .full, .hostname = hostname.?, .game_type = game_type.?, .map = map.?, .version = version, .players = players.?, .max_players = max_players_value.?, .host_port = host_port.?, .host_ip = host_ip.?, .plugins = plugins, .player_names = try names.toOwnedSlice(allocator) };
}

fn takeString(allocator: std.mem.Allocator, data: []const u8, index: *usize) ![]const u8 {
    if (index.* >= data.len) return error.ProtocolError;
    const suffix = data[index.*..];
    const end = std.mem.indexOfScalar(u8, suffix, 0) orelse return error.ProtocolError;
    if (end > max_string_bytes or !std.unicode.utf8ValidateSlice(suffix[0..end])) return error.ProtocolError;
    index.* += end + 1;
    return allocator.dupe(u8, suffix[0..end]);
}

fn parseDecimal(text: []const u8) !u32 {
    if (text.len == 0) return error.ProtocolError;
    for (text) |byte| if (byte < '0' or byte > '9') return error.ProtocolError;
    return std.fmt.parseInt(u32, text, 10) catch error.ProtocolError;
}

test "Query basic parsing validates endian fields" {
    const packet = "\x00\x01\x02\x03\x04host\x00SMP\x00world\x002\x0020\x00\xdd\x63" ++ "127.0.0.1\x00";
    var result = try parseStatus(std.testing.allocator, packet, .{ 1, 2, 3, 4 }, .basic);
    defer result.deinit();
    try std.testing.expectEqualStrings("host", result.hostname);
    try std.testing.expectEqual(@as(u16, 25565), result.host_port);
    try std.testing.expectEqual(@as(u32, 2), result.players);
}

test "Query rejects malformed challenge packets" {
    try std.testing.expectEqual(@as(i32, 1), try parseChallenge("\x09\x01\x02\x03\x041\x00", .{ 1, 2, 3, 4 }));
    try std.testing.expectError(error.ProtocolError, parseChallenge("\x09\x01\x02\x03\x04bad\x00", .{ 1, 2, 3, 4 }));
    try std.testing.expectError(error.ProtocolError, parseChallenge("\x09\x01\x02\x03\x04", .{ 1, 2, 3, 4 }));
    try std.testing.expectError(error.ProtocolError, parseChallenge("\x09\x01\x02\x03\x041\x00x", .{ 1, 2, 3, 4 }));
}

test "Query basic rejects zero ports and declared player limits" {
    const zero_port = "\x00\x01\x02\x03\x04host\x00SMP\x00world\x002\x0020\x00\x00\x00" ++ "127.0.0.1\x00";
    try std.testing.expectError(error.ProtocolError, parseStatus(std.testing.allocator, zero_port, .{ 1, 2, 3, 4 }, .basic));
    const too_many = "\x00\x01\x02\x03\x04host\x00SMP\x00world\x00257\x00257\x00\xdd\x63" ++ "127.0.0.1\x00";
    try std.testing.expectError(error.ProtocolError, parseStatus(std.testing.allocator, too_many, .{ 1, 2, 3, 4 }, .basic));
}

test "Query full parsing accepts bounded key values and player names" {
    const packet =
        "\x00\x01\x02\x03\x04splitnum\x00\x80\x00" ++
        "hostname\x00host\x00gametype\x00SMP\x00map\x00world\x00version\x001.21\x00game_id\x00MINECRAFT\x00numplayers\x001\x00maxplayers\x0020\x00hostport\x0025565\x00hostip\x00127.0.0.1\x00" ++
        "\x00\x01player_\x00\x00Alex\x00\x00";
    var result = try parseStatus(std.testing.allocator, packet, .{ 1, 2, 3, 4 }, .full);
    defer result.deinit();
    try std.testing.expectEqualStrings("1.21", result.version.?);
    try std.testing.expectEqualStrings("Alex", result.player_names[0]);
}

test "Query full rejects invalid prefix and unknown keys" {
    const invalid_prefix = "\x00\x01\x02\x03\x04splitnope\x00\x80\x00";
    try std.testing.expectError(error.ProtocolError, parseStatus(std.testing.allocator, invalid_prefix, .{ 1, 2, 3, 4 }, .full));
    const unknown_key = "\x00\x01\x02\x03\x04splitnum\x00\x80\x00wat\x00x\x00\x00\x01player_\x00\x00";
    try std.testing.expectError(error.ProtocolError, parseStatus(std.testing.allocator, unknown_key, .{ 1, 2, 3, 4 }, .full));
}

test "Query parsers reject malformed strings duplicates separators and oversized packets" {
    const missing_nul = "\x00\x01\x02\x03\x04host";
    try std.testing.expectError(error.ProtocolError, parseStatus(std.testing.allocator, missing_nul, .{ 1, 2, 3, 4 }, .basic));
    const invalid_utf8 = "\x00\x01\x02\x03\x04\xff\x00SMP\x00world\x000\x001\x00\xdd\x63ip\x00";
    try std.testing.expectError(error.ProtocolError, parseStatus(std.testing.allocator, invalid_utf8, .{ 1, 2, 3, 4 }, .basic));
    const duplicate = "\x00\x01\x02\x03\x04splitnum\x00\x80\x00hostname\x00a\x00hostname\x00b\x00\x00\x01player_\x00\x00\x00";
    try std.testing.expectError(error.ProtocolError, parseStatus(std.testing.allocator, duplicate, .{ 1, 2, 3, 4 }, .full));
    const bad_separator = "\x00\x01\x02\x03\x04splitnum\x00\x80\x00hostname\x00h\x00gametype\x00SMP\x00map\x00w\x00numplayers\x000\x00maxplayers\x001\x00hostport\x0025565\x00hostip\x00ip\x00\x00\x01player_\x00bad";
    try std.testing.expectError(error.ProtocolError, parseStatus(std.testing.allocator, bad_separator, .{ 1, 2, 3, 4 }, .full));
    const oversized = try std.testing.allocator.alloc(u8, max_datagram_bytes + 1);
    defer std.testing.allocator.free(oversized);
    @memset(oversized, 0);
    try std.testing.expectError(error.ProtocolError, parseStatus(std.testing.allocator, oversized, .{ 1, 2, 3, 4 }, .basic));
}

const FakeUdpServer = struct {
    io: std.Io,
    socket: *std.Io.net.Socket,
    failed: std.atomic.Value(bool) = .init(false),
    behavior: Behavior = .full,

    const Behavior = enum { full, basic, wrong_session };

    fn serve(context: *FakeUdpServer) void {
        context.serveInner() catch context.failed.store(true, .release);
    }

    fn serveInner(context: *FakeUdpServer) !void {
        var buffer: [max_datagram_bytes]u8 = undefined;
        const timeout: std.Io.Timeout = .{ .duration = .{ .raw = std.Io.Duration.fromMilliseconds(deadline_ms), .clock = .awake } };
        const handshake = try context.socket.receiveTimeout(context.io, &buffer, timeout);
        if (handshake.data.len != 7 or !std.mem.eql(u8, handshake.data[0..3], "\xfe\xfd\x09")) return error.BadHandshake;
        const session = handshake.data[3..7].*;
        const challenge = [_]u8{ 0x09, session[0], session[1], session[2], session[3], '1', '2', '3', '4', 0 };
        try context.socket.send(context.io, &handshake.from, &challenge);
        const stat = try context.socket.receiveTimeout(context.io, &buffer, timeout);
        const expected_len: usize = if (context.behavior == .basic) 11 else 15;
        if (stat.data.len != expected_len or !std.mem.eql(u8, stat.data[0..3], "\xfe\xfd\x00") or !std.mem.eql(u8, stat.data[3..7], &session) or std.mem.readInt(i32, @ptrCast(stat.data[7..11]), .big) != 1234 or (context.behavior != .basic and !std.mem.eql(u8, stat.data[11..15], "\x00\x00\x00\x00"))) return error.BadStatRequest;
        if (context.behavior == .basic) {
            const response = "\x00" ++ &session ++ "host\x00SMP\x00world\x001\x0020\x00\xdd\x63" ++ "127.0.0.1\x00";
            try context.socket.send(context.io, &stat.from, response);
            return;
        }
        var reply_session = session;
        if (context.behavior == .wrong_session) reply_session[0] +%= 1;
        const response =
            "\x00" ++ &reply_session ++ "splitnum\x00\x80\x00" ++
            "hostname\x00host\x00gametype\x00SMP\x00map\x00world\x00game_id\x00MINECRAFT\x00numplayers\x001\x00maxplayers\x0020\x00hostport\x0025565\x00hostip\x00127.0.0.1\x00\x00\x01player_\x00\x00Alex\x00\x00";
        try context.socket.send(context.io, &stat.from, response);
    }
};

test "Query fake UDP server verifies handshake and full stat wire shape" {
    // Managed server Query is Linux-only; Zig 0.16 Windows lacks timed UDP receive.
    if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
    const io = std.testing.io;
    var address = std.Io.net.IpAddress.parse("127.0.0.1", 0) catch unreachable;
    var socket = address.bind(io, .{ .mode = .dgram, .protocol = .udp }) catch return error.SkipZigTest;
    defer socket.close(io);
    var context: FakeUdpServer = .{ .io = io, .socket = &socket };
    const thread = try std.Thread.spawn(.{}, FakeUdpServer.serve, .{&context});
    defer thread.join();
    var result = try execute(std.testing.allocator, io, socket.address.getPort(), .full);
    defer result.deinit();
    try std.testing.expect(!context.failed.load(.acquire));
    try std.testing.expectEqualStrings("host", result.hostname);
    try std.testing.expectEqualStrings("Alex", result.player_names[0]);
}

test "Query fake UDP server verifies basic stat request and reply" {
    // Managed server Query is Linux-only; Zig 0.16 Windows lacks timed UDP receive.
    if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
    const io = std.testing.io;
    var address = std.Io.net.IpAddress.parse("127.0.0.1", 0) catch unreachable;
    var socket = address.bind(io, .{ .mode = .dgram, .protocol = .udp }) catch return error.SkipZigTest;
    defer socket.close(io);
    var context: FakeUdpServer = .{ .io = io, .socket = &socket, .behavior = .basic };
    const thread = try std.Thread.spawn(.{}, FakeUdpServer.serve, .{&context});
    defer thread.join();
    var result = try execute(std.testing.allocator, io, socket.address.getPort(), .basic);
    defer result.deinit();
    try std.testing.expectEqualStrings("host", result.hostname);
    try std.testing.expectEqual(@as(u16, 25565), result.host_port);
}

test "Query fake UDP server rejects wrong session" {
    // Managed server Query is Linux-only; Zig 0.16 Windows lacks timed UDP receive.
    if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
    const io = std.testing.io;
    var address = std.Io.net.IpAddress.parse("127.0.0.1", 0) catch unreachable;
    var socket = address.bind(io, .{ .mode = .dgram, .protocol = .udp }) catch return error.SkipZigTest;
    defer socket.close(io);
    var context: FakeUdpServer = .{ .io = io, .socket = &socket, .behavior = .wrong_session };
    const thread = try std.Thread.spawn(.{}, FakeUdpServer.serve, .{&context});
    defer thread.join();
    try std.testing.expectError(error.ProtocolError, execute(std.testing.allocator, io, socket.address.getPort(), .full));
}
