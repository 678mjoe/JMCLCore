//! Bounded, IPv4-loopback-only Source RCON client.
//!
//! After the first command response arrives, an empty command is sent as a
//! sentinel. Its response marks the end of the multi-packet command reply;
//! this never uses a timeout as a delimiter.
const std = @import("std");
const builtin = @import("builtin");

pub const max_frame_bytes: usize = 64 * 1024;
pub const max_reply_bytes: usize = 256 * 1024;
pub const max_command_bytes: usize = 4095;
pub const deadline_ms: i64 = 3_000;

pub const Reply = struct {
    allocator: std.mem.Allocator,
    text: []u8,
    truncated: bool,

    pub fn deinit(reply: *Reply) void {
        reply.allocator.free(reply.text);
        reply.* = undefined;
    }
};

const auth_type: i32 = 3;
const command_type: i32 = 2;
const response_type: i32 = 0;
const auth_response_type: i32 = 2;
var next_request_id: std.atomic.Value(u32) = .init(1);

const Packet = struct { id: i32, typ: i32, body: []const u8 };

/// The port is supplied only by the service after strict server.properties
/// resolution. This module itself always dials the IPv4 loopback literal.
pub fn execute(
    allocator: std.mem.Allocator,
    io: std.Io,
    port: u16,
    password: []const u8,
    command: []const u8,
    requested_max_bytes: usize,
) !Reply {
    if (password.len == 0 or password.len > max_command_bytes or command.len == 0 or command.len > max_command_bytes or
        std.mem.indexOfScalar(u8, password, 0) != null or std.mem.indexOfScalar(u8, command, 0) != null or
        !std.unicode.utf8ValidateSlice(password) or !std.unicode.utf8ValidateSlice(command)) return error.InvalidInput;
    const reply_limit = if (requested_max_bytes == 0) max_reply_bytes else requested_max_bytes;
    if (reply_limit > max_reply_bytes) return error.InvalidInput;

    const address: std.Io.net.IpAddress = .{ .ip4 = .loopback(port) };
    var stream = std.Io.net.IpAddress.connect(&address, io, .{
        .mode = .stream,
        .protocol = .tcp,
    }) catch return error.TransportUnavailable;
    defer stream.close(io);
    const deadline = nowMilliseconds(io) + deadline_ms;
    const ids = requestIds();
    const auth_id = ids[0];
    const command_id = ids[1];
    const sentinel_id = ids[2];
    try writePacket(io, stream.socket.handle, deadline, auth_id, auth_type, password);
    var scratch: [max_frame_bytes]u8 = undefined;
    // Some compatible servers send an empty RESPONSE_VALUE before the actual
    // SERVERDATA_AUTH_RESPONSE. It has the same request id and is ignored.
    while (true) {
        const auth = readTransportPacket(io, stream.socket.handle, deadline, &scratch) catch |err| return err;
        if (auth.id == -1) return error.AuthenticationFailed;
        if (auth.id != auth_id) return error.ProtocolError;
        if (auth.typ == response_type and auth.body.len == 0) continue;
        if (auth.typ != auth_response_type) return error.ProtocolError;
        if (auth.body.len != 0) return error.ProtocolError;
        break;
    }

    try writePacket(io, stream.socket.handle, deadline, command_id, command_type, command);
    var result: std.ArrayList(u8) = .empty;
    errdefer result.deinit(allocator);
    var truncated = false;
    // Minecraft can read exactly one packet from TCP at a time. Do not write
    // the probe until it has acknowledged command receipt with its first value.
    const first = try readTransportPacket(io, stream.socket.handle, deadline, &scratch);
    if (first.typ != response_type or first.id != command_id) return error.ProtocolError;
    try appendReply(&result, allocator, reply_limit, first.body, &truncated);
    try writePacket(io, stream.socket.handle, deadline, sentinel_id, command_type, "");
    while (true) {
        const packet = try readTransportPacket(io, stream.socket.handle, deadline, &scratch);
        if (packet.typ != response_type) return error.ProtocolError;
        if (packet.id == sentinel_id) {
            if (packet.body.len != 0) return error.ProtocolError;
            break;
        }
        if (packet.id != command_id) return error.ProtocolError;
        try appendReply(&result, allocator, reply_limit, packet.body, &truncated);
    }
    if (!std.unicode.utf8ValidateSlice(result.items)) {
        if (!truncated) return error.ProtocolError;
        // A byte ceiling may cut a valid server reply midway through a UTF-8
        // codepoint. Preserve the text contract by retaining a valid prefix.
        while (result.items.len > 0 and !std.unicode.utf8ValidateSlice(result.items)) _ = result.pop();
    }
    return .{ .allocator = allocator, .text = try result.toOwnedSlice(allocator), .truncated = truncated };
}

fn requestIds() [3]i32 {
    // Request id -1 has special authentication-failure meaning and is never
    // emitted. Wrap safely into the positive signed range.
    // Reserve all three together so wrapping cannot duplicate an id within a
    // single authentication/command/probe exchange.
    const start = next_request_id.fetchAdd(3, .monotonic) & 0x7fff_ffff;
    if (start == 0 or start > 0x7fff_fffc) return requestIds();
    return .{ @intCast(start), @intCast(start + 1), @intCast(start + 2) };
}

fn appendReply(result: *std.ArrayList(u8), allocator: std.mem.Allocator, limit: usize, body: []const u8, truncated: *bool) !void {
    const available = limit -| result.items.len;
    const take = @min(available, body.len);
    try result.appendSlice(allocator, body[0..take]);
    if (take != body.len) truncated.* = true;
}

fn writePacket(io: std.Io, handle: std.Io.net.Socket.Handle, deadline: i64, id: i32, typ: i32, body: []const u8) !void {
    if (body.len > max_frame_bytes - 10) return error.ProtocolError;
    var packet: [max_frame_bytes + 4]u8 = undefined;
    defer std.crypto.secureZero(u8, &packet);
    const length: i32 = @intCast(body.len + 10);
    std.mem.writeInt(i32, packet[0..4], length, .little);
    std.mem.writeInt(i32, packet[4..8], id, .little);
    std.mem.writeInt(i32, packet[8..12], typ, .little);
    @memcpy(packet[12 .. 12 + body.len], body);
    packet[12 + body.len] = 0;
    packet[13 + body.len] = 0;
    writeAll(io, handle, deadline, packet[0 .. body.len + 14]) catch return error.TransportUnavailable;
}

fn readTransportPacket(io: std.Io, handle: std.Io.net.Socket.Handle, deadline: i64, scratch: []u8) !Packet {
    return readPacket(io, handle, deadline, scratch) catch |err| switch (err) {
        error.Timeout, error.EndOfStream, error.ReadFailed => error.TransportUnavailable,
        else => |other| other,
    };
}

fn readPacket(io: std.Io, handle: std.Io.net.Socket.Handle, deadline: i64, scratch: []u8) !Packet {
    var prefix: [4]u8 = undefined;
    try readExact(io, handle, deadline, &prefix);
    const length = std.mem.readInt(i32, &prefix, .little);
    if (length < 10 or length > max_frame_bytes or @as(usize, @intCast(length)) > scratch.len) return error.ProtocolError;
    const frame = scratch[0..@intCast(length)];
    try readExact(io, handle, deadline, frame);
    return decodeFrame(frame);
}

fn decodeFrame(frame: []const u8) !Packet {
    if (frame.len < 10 or frame.len > max_frame_bytes or frame[frame.len - 1] != 0 or frame[frame.len - 2] != 0) return error.ProtocolError;
    return .{
        .id = std.mem.readInt(i32, frame[0..4], .little),
        .typ = std.mem.readInt(i32, frame[4..8], .little),
        .body = frame[8 .. frame.len - 2],
    };
}

fn waitReadable(io: std.Io, handle: std.Io.net.Socket.Handle, deadline: i64) !void {
    const remaining = deadline - nowMilliseconds(io);
    if (remaining <= 0) return error.Timeout;
    var fds = [_]std.posix.pollfd{.{ .fd = handle, .events = std.posix.POLL.IN | std.posix.POLL.ERR, .revents = 0 }};
    const count = std.posix.poll(&fds, @intCast(@min(remaining, std.math.maxInt(i32)))) catch return error.ReadFailed;
    if (count == 0) return error.Timeout;
}

fn readExact(io: std.Io, handle: std.Io.net.Socket.Handle, deadline: i64, output: []u8) !void {
    var index: usize = 0;
    while (index < output.len) {
        try waitReadable(io, handle, deadline);
        var vectors = [_][]u8{output[index..]};
        const count = io.vtable.netRead(io.userdata, handle, &vectors) catch return error.ReadFailed;
        if (count == 0) return error.EndOfStream;
        index += count;
    }
}

fn writeAll(io: std.Io, handle: std.Io.net.Socket.Handle, deadline: i64, input: []const u8) !void {
    var index: usize = 0;
    while (index < input.len) {
        if (deadline - nowMilliseconds(io) <= 0) return error.Timeout;
        const written = io.vtable.netWrite(io.userdata, handle, "", &.{input[index..]}, 1) catch return error.WriteFailed;
        if (written == 0) return error.WriteFailed;
        index += written;
    }
}

fn nowMilliseconds(io: std.Io) i64 {
    return std.Io.Timestamp.now(io, .awake).toMilliseconds();
}

test "RCON packet framing is little endian and rejects malformed packets" {
    var bytes: [14]u8 = undefined;
    std.mem.writeInt(i32, bytes[0..4], 10, .little);
    std.mem.writeInt(i32, bytes[4..8], 0x10203040, .little);
    std.mem.writeInt(i32, bytes[8..12], auth_type, .little);
    bytes[12] = 0;
    bytes[13] = 0;
    try std.testing.expectEqual(@as(i32, 10), std.mem.readInt(i32, bytes[0..4], .little));
    try std.testing.expectEqual(@as(i32, 0x10203040), std.mem.readInt(i32, bytes[4..8], .little));
    bytes[13] = 1;
    try std.testing.expectError(error.ProtocolError, decodeFrame(&bytes));
    const undersized = [_]u8{0} ** 9;
    try std.testing.expectError(error.ProtocolError, decodeFrame(&undersized));
    var oversized: [max_frame_bytes + 1]u8 = @splat(0);
    try std.testing.expectError(error.ProtocolError, decodeFrame(&oversized));
}

const FakeServer = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    failed: std.atomic.Value(bool) = .init(false),
    behavior: Behavior = .normal,

    const Behavior = enum { normal, auth_failure, wrong_auth_type, wrong_auth_id, wrong_command_type, wrong_command_id, invalid_utf8, truncate_utf8, nonempty_sentinel };

    fn serve(context: *FakeServer) void {
        context.serveInner() catch context.failed.store(true, .release);
    }

    fn serveInner(context: *FakeServer) !void {
        var stream = try context.listener.accept(context.io);
        defer stream.close(context.io);
        var scratch: [max_frame_bytes]u8 = undefined;
        const deadline = nowMilliseconds(context.io) + deadline_ms;
        const auth = try readPacket(context.io, stream.socket.handle, deadline, &scratch);
        if (auth.typ != auth_type or !std.mem.eql(u8, auth.body, "secret")) return error.BadAuth;
        if (context.behavior == .auth_failure) {
            try writePacket(context.io, stream.socket.handle, deadline, -1, auth_response_type, "");
            return;
        }
        if (context.behavior == .wrong_auth_type) {
            try writePacket(context.io, stream.socket.handle, deadline, auth.id, auth_type, "");
            return;
        }
        if (context.behavior == .wrong_auth_id) {
            try writePacket(context.io, stream.socket.handle, deadline, auth.id + 1, auth_response_type, "");
            return;
        }
        try writePacket(context.io, stream.socket.handle, deadline, auth.id, response_type, "");
        try writePacket(context.io, stream.socket.handle, deadline, auth.id, auth_response_type, "");
        const command = try readPacket(context.io, stream.socket.handle, deadline, &scratch);
        if (command.typ != command_type or !std.mem.eql(u8, command.body, "list")) return error.BadCommand;
        if (context.behavior == .wrong_command_type) {
            try writePacket(context.io, stream.socket.handle, deadline, command.id, auth_response_type, "");
            return;
        }
        if (context.behavior == .wrong_command_id) {
            try writePacket(context.io, stream.socket.handle, deadline, command.id + 1, response_type, "");
            return;
        }
        // The client must wait for this response before writing the sentinel.
        var fds = [_]std.posix.pollfd{.{ .fd = stream.socket.handle, .events = std.posix.POLL.IN, .revents = 0 }};
        if (try std.posix.poll(&fds, 75) != 0) return error.ProbeSentTooEarly;
        const first_body: []const u8 = switch (context.behavior) {
            .invalid_utf8 => "\xff",
            .truncate_utf8 => "a€",
            else => "one ",
        };
        try writePacket(context.io, stream.socket.handle, deadline, command.id, response_type, first_body);
        const sentinel = try readPacket(context.io, stream.socket.handle, deadline, &scratch);
        if (sentinel.typ != command_type or sentinel.body.len != 0 or sentinel.id == command.id) return error.BadSentinel;
        if (context.behavior == .normal) try writePacket(context.io, stream.socket.handle, deadline, command.id, response_type, "two");
        try writePacket(
            context.io,
            stream.socket.handle,
            deadline,
            sentinel.id,
            response_type,
            if (context.behavior == .nonempty_sentinel) "unexpected" else "",
        );
    }
};

test "RCON fake server proves auth and command sentinel ordering" {
    // Server operations are Linux-only; this transport fixture requires POSIX poll.
    if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
    const io = std.testing.io;
    var address = std.Io.net.IpAddress.parse("127.0.0.1", 0) catch unreachable;
    var listener = address.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    var context: FakeServer = .{ .io = io, .listener = &listener };
    const thread = try std.Thread.spawn(.{}, FakeServer.serve, .{&context});
    defer thread.join();
    var reply = try execute(std.testing.allocator, io, listener.socket.address.getPort(), "secret", "list", 1024);
    defer reply.deinit();
    try std.testing.expect(!context.failed.load(.acquire));
    try std.testing.expectEqualStrings("one two", reply.text);
    try std.testing.expect(!reply.truncated);
}

test "RCON fake server maps auth id minus one to authentication failure" {
    // Server operations are Linux-only; this transport fixture requires POSIX poll.
    if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
    const io = std.testing.io;
    var address = std.Io.net.IpAddress.parse("127.0.0.1", 0) catch unreachable;
    var listener = address.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    var context: FakeServer = .{ .io = io, .listener = &listener, .behavior = .auth_failure };
    const thread = try std.Thread.spawn(.{}, FakeServer.serve, .{&context});
    defer thread.join();
    try std.testing.expectError(error.AuthenticationFailed, execute(std.testing.allocator, io, listener.socket.address.getPort(), "secret", "list", 1024));
    try std.testing.expect(!context.failed.load(.acquire));
}

fn expectFakeProtocolError(behavior: FakeServer.Behavior) !void {
    // Server operations are Linux-only; this transport fixture requires POSIX poll.
    if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
    const io = std.testing.io;
    var address = std.Io.net.IpAddress.parse("127.0.0.1", 0) catch unreachable;
    var listener = address.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    var context: FakeServer = .{ .io = io, .listener = &listener, .behavior = behavior };
    const thread = try std.Thread.spawn(.{}, FakeServer.serve, .{&context});
    defer thread.join();
    try std.testing.expectError(error.ProtocolError, execute(std.testing.allocator, io, listener.socket.address.getPort(), "secret", "list", 1024));
    try std.testing.expect(!context.failed.load(.acquire));
}

test "RCON fake server rejects wrong auth response type" {
    try expectFakeProtocolError(.wrong_auth_type);
}
test "RCON fake server rejects mismatched auth request id" {
    try expectFakeProtocolError(.wrong_auth_id);
}
test "RCON fake server rejects wrong command response type" {
    try expectFakeProtocolError(.wrong_command_type);
}
test "RCON fake server rejects mismatched command request id" {
    try expectFakeProtocolError(.wrong_command_id);
}

test "RCON fake server rejects non-truncated invalid UTF-8 reply" {
    try expectFakeProtocolError(.invalid_utf8);
}

test "RCON fake server rejects a non-empty sentinel response" {
    try expectFakeProtocolError(.nonempty_sentinel);
}

test "RCON fake server truncates at a valid UTF-8 prefix" {
    // Server operations are Linux-only; this transport fixture requires POSIX poll.
    if (comptime builtin.os.tag == .windows) return error.SkipZigTest;
    const io = std.testing.io;
    var address = std.Io.net.IpAddress.parse("127.0.0.1", 0) catch unreachable;
    var listener = address.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    var context: FakeServer = .{ .io = io, .listener = &listener, .behavior = .truncate_utf8 };
    const thread = try std.Thread.spawn(.{}, FakeServer.serve, .{&context});
    defer thread.join();
    var reply = try execute(std.testing.allocator, io, listener.socket.address.getPort(), "secret", "list", 2);
    defer reply.deinit();
    try std.testing.expectEqualStrings("a", reply.text);
    try std.testing.expect(reply.truncated);
    try std.testing.expect(!context.failed.load(.acquire));
}
