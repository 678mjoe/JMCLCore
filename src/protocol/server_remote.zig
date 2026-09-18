const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");
const server = @import("server.zig");

const RconParams = struct {
    directory: []const u8,
    id: []const u8,
    password: []const u8,
    command: []const u8,
    max_bytes: usize = core.server_rcon.max_reply_bytes,
};

const QueryParams = struct {
    directory: []const u8,
    id: []const u8,
    mode: []const u8 = "full",
};

pub fn writeRconResult(service: *core.Service, writer: *std.Io.Writer, request_id: []const u8, params_value: ?std.json.Value) std.Io.Writer.Error!void {
    if (!try server.requireLinux(writer, request_id)) return;
    const value = params_value orelse return protocol.writeError(writer, request_id, "INVALID_PARAMS", "server RCON requires directory, id, password, and command");
    var wiping = protocol.WipingAllocator{ .child = service.allocator };
    var params = std.json.parseFromValue(RconParams, wiping.allocator(), value, .{ .ignore_unknown_fields = true }) catch
        return protocol.writeError(writer, request_id, "INVALID_PARAMS", "server RCON parameters are invalid");
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0 or input.password.len == 0 or input.password.len > core.server_rcon.max_command_bytes or input.command.len == 0 or input.command.len > core.server_rcon.max_command_bytes or input.max_bytes == 0 or input.max_bytes > core.server_rcon.max_reply_bytes or std.mem.indexOfScalar(u8, input.password, 0) != null or std.mem.indexOfScalar(u8, input.command, 0) != null or !std.unicode.utf8ValidateSlice(input.password) or !std.unicode.utf8ValidateSlice(input.command)) {
        return protocol.writeError(writer, request_id, "INVALID_PARAMS", "server RCON parameters are out of range");
    }
    core.server_store.validateId(input.id) catch return protocol.writeError(writer, request_id, "INVALID_PARAMS", "the server id is invalid");
    var reply = service.executeServerRcon(input.directory, input.id, input.password, input.command, input.max_bytes) catch |err| {
        try writeRemoteError(writer, request_id, err);
        return;
    };
    defer reply.deinit();
    var json: std.json.Stringify = .{ .writer = writer };
    try protocol.writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.rcon.command");
    try json.objectField("text");
    try json.write(reply.text);
    try json.objectField("truncated");
    try json.write(reply.truncated);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeQueryResult(service: *core.Service, writer: *std.Io.Writer, request_id: []const u8, params_value: ?std.json.Value) std.Io.Writer.Error!void {
    if (!try server.requireLinux(writer, request_id)) return;
    const value = params_value orelse return protocol.writeError(writer, request_id, "INVALID_PARAMS", "server query requires directory and id");
    var params = std.json.parseFromValue(QueryParams, service.allocator, value, .{ .ignore_unknown_fields = true }) catch
        return protocol.writeError(writer, request_id, "INVALID_PARAMS", "server query parameters are invalid");
    defer params.deinit();
    const input = params.value;
    if (input.directory.len == 0 or input.id.len == 0) return protocol.writeError(writer, request_id, "INVALID_PARAMS", "server query parameters are out of range");
    core.server_store.validateId(input.id) catch return protocol.writeError(writer, request_id, "INVALID_PARAMS", "the server id is invalid");
    const mode: core.server_query.Mode = if (std.mem.eql(u8, input.mode, "basic")) .basic else if (std.mem.eql(u8, input.mode, "full")) .full else return protocol.writeError(writer, request_id, "INVALID_PARAMS", "server query mode must be basic or full");
    var result = service.queryServer(input.directory, input.id, mode) catch |err| {
        try writeRemoteError(writer, request_id, err);
        return;
    };
    defer result.deinit();
    var json: std.json.Stringify = .{ .writer = writer };
    try protocol.writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("server.query");
    try json.objectField("mode");
    try json.write(@tagName(result.mode));
    try json.objectField("hostname");
    try json.write(result.hostname);
    try json.objectField("game_type");
    try json.write(result.game_type);
    try json.objectField("map");
    try json.write(result.map);
    try json.objectField("version");
    try json.write(result.version);
    try json.objectField("players");
    try json.write(result.players);
    try json.objectField("max_players");
    try json.write(result.max_players);
    try json.objectField("host_port");
    try json.write(result.host_port);
    try json.objectField("host_ip");
    try json.write(result.host_ip);
    try json.objectField("plugins");
    try json.write(result.plugins);
    try json.objectField("player_names");
    try json.write(result.player_names);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeRemoteError(writer: *std.Io.Writer, request_id: []const u8, err: core.Service.ServerRemoteError) std.Io.Writer.Error!void {
    const code, const message = switch (err) {
        error.ServerNotRunning => .{ "SERVER_NOT_RUNNING", "The server is not running" },
        error.ServerPropertiesInvalid => .{ "SERVER_PROPERTIES_INVALID", "Server properties are invalid" },
        error.ServerRconDisabled => .{ "SERVER_RCON_DISABLED", "RCON is not enabled for this server" },
        error.ServerRconAuthenticationFailed => .{ "SERVER_RCON_AUTH_FAILED", "RCON authentication failed" },
        error.ServerRconUnavailable => .{ "SERVER_RCON_UNAVAILABLE", "RCON is unavailable" },
        error.ServerRconProtocolError => .{ "SERVER_RCON_PROTOCOL_ERROR", "RCON returned an invalid response" },
        error.ServerQueryDisabled => .{ "SERVER_QUERY_DISABLED", "Query is not enabled for this server" },
        error.ServerQueryUnavailable => .{ "SERVER_QUERY_UNAVAILABLE", "Query is unavailable" },
        error.ServerQueryProtocolError => .{ "SERVER_QUERY_PROTOCOL_ERROR", "Query returned an invalid response" },
        error.ServerNotFound => .{ "SERVER_NOT_FOUND", "No server matches the requested id" },
        error.InvalidServerRemoteInput => .{ "INVALID_PARAMS", "server RCON command or password is invalid" },
        error.UnsupportedPlatform => .{ "UNSUPPORTED_PLATFORM", "Server management is currently supported only on Linux" },
        else => .{ "SERVER_REMOTE_FAILED", "The server remote operation failed" },
    };
    try protocol.writeError(writer, request_id, code, message);
}

test "remote invalid input maps to INVALID_PARAMS without secret reflection" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try writeRemoteError(&output.writer, "request", error.InvalidServerRemoteInput);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "secret-marker") == null);
}
