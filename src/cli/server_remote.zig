const std = @import("std");
const jmcl = @import("jmcl_core");

pub fn runRcon(service: *jmcl.Service, args: []const []const u8, stdout: *std.Io.Writer, stderr: *std.Io.Writer) !u8 {
    if (args.len == 0 or !std.mem.eql(u8, args[0], "command")) return usageRcon(stderr);
    if (args.len < 2 or args[1].len == 0) return usageRcon(stderr);
    jmcl.server_store.validateId(args[1]) catch return usageRcon(stderr);
    var command: ?[]const u8 = null;
    var password_file: ?[]const u8 = null;
    var directory: []const u8 = jmcl.server_store.default_root;
    var max_bytes: usize = jmcl.server_rcon.max_reply_bytes;
    var index: usize = 2;
    while (index < args.len) {
        if (index + 1 >= args.len or args[index + 1].len == 0) return usageRcon(stderr);
        const value = args[index + 1];
        if (std.mem.eql(u8, args[index], "--command")) command = value else if (std.mem.eql(u8, args[index], "--password-file")) password_file = value else if (std.mem.eql(u8, args[index], "--servers-directory")) directory = value else if (std.mem.eql(u8, args[index], "--max-bytes")) max_bytes = std.fmt.parseInt(usize, value, 10) catch return usageRcon(stderr) else return usageRcon(stderr);
        index += 2;
    }
    const text = command orelse return usageRcon(stderr);
    const secret_path = password_file orelse return usageRcon(stderr);
    if (max_bytes == 0 or max_bytes > jmcl.server_rcon.max_reply_bytes) return usageRcon(stderr);
    if (text.len == 0 or text.len > jmcl.server_rcon.max_command_bytes or std.mem.indexOfScalar(u8, text, 0) != null or !std.unicode.utf8ValidateSlice(text)) return usageRcon(stderr);
    const password = readPasswordFile(service, secret_path) catch {
        try stderr.writeAll("password file is invalid\n");
        return 2;
    };
    defer {
        std.crypto.secureZero(u8, password.storage);
        service.allocator.free(password.storage);
    }
    var reply = service.executeServerRcon(directory, args[1], password.value, text, max_bytes) catch |err| {
        try writeError(stderr, err);
        return 1;
    };
    defer reply.deinit();
    try stdout.writeAll(reply.text);
    if (reply.truncated) try stderr.writeAll("RCON reply truncated at requested limit\n");
    return 0;
}

pub fn runQuery(service: *jmcl.Service, args: []const []const u8, stdout: *std.Io.Writer, stderr: *std.Io.Writer) !u8 {
    if (args.len == 0 or args[0].len == 0) return usageQuery(stderr);
    jmcl.server_store.validateId(args[0]) catch return usageQuery(stderr);
    var mode: jmcl.server_query.Mode = .full;
    var directory: []const u8 = jmcl.server_store.default_root;
    var json_output = false;
    var index: usize = 1;
    while (index < args.len) {
        if (std.mem.eql(u8, args[index], "--json")) {
            json_output = true;
            index += 1;
            continue;
        }
        if (index + 1 >= args.len or args[index + 1].len == 0) return usageQuery(stderr);
        const value = args[index + 1];
        if (std.mem.eql(u8, args[index], "--mode")) {
            mode = if (std.mem.eql(u8, value, "basic")) .basic else if (std.mem.eql(u8, value, "full")) .full else return usageQuery(stderr);
        } else if (std.mem.eql(u8, args[index], "--servers-directory")) directory = value else return usageQuery(stderr);
        index += 2;
    }
    var result = service.queryServer(directory, args[0], mode) catch |err| {
        try writeError(stderr, err);
        return 1;
    };
    defer result.deinit();
    if (json_output) {
        var json: std.json.Stringify = .{ .writer = stdout };
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
        try stdout.writeByte('\n');
    } else {
        try stdout.print("hostname\t{s}\ngame_type\t{s}\nmap\t{s}\nversion\t{s}\nplayers\t{d}/{d}\nhost\t{s}:{d}\n", .{ result.hostname, result.game_type, result.map, result.version orelse "", result.players, result.max_players, result.host_ip, result.host_port });
        if (result.plugins) |plugins| try stdout.print("plugins\t{s}\n", .{plugins});
        for (result.player_names) |name| try stdout.print("player\t{s}\n", .{name});
    }
    return 0;
}

const PasswordFile = struct { storage: []u8, value: []u8 };

fn readPasswordFile(service: *jmcl.Service, path: []const u8) !PasswordFile {
    const file = try std.Io.Dir.cwd().openFile(service.io, path, .{
        .mode = .read_only,
        .allow_directory = false,
        .follow_symlinks = false,
    });
    defer file.close(service.io);
    const stat = try file.stat(service.io);
    if (stat.kind != .file or stat.size > jmcl.server_rcon.max_command_bytes + 2) return error.InvalidPasswordFile;
    const size: usize = std.math.cast(usize, stat.size) orelse return error.InvalidPasswordFile;
    const storage = try service.allocator.alloc(u8, size);
    errdefer {
        std.crypto.secureZero(u8, storage);
        service.allocator.free(storage);
    }
    var offset: usize = 0;
    while (offset < storage.len) {
        const destinations = [_][]u8{storage[offset..]};
        const read = file.readStreaming(service.io, &destinations) catch return error.InvalidPasswordFile;
        if (read == 0) return error.InvalidPasswordFile;
        offset += read;
    }
    var extra_byte: [1]u8 = undefined;
    defer std.crypto.secureZero(u8, &extra_byte);
    const extra_destinations = [_][]u8{&extra_byte};
    const extra_read = file.readStreaming(service.io, &extra_destinations) catch |err| switch (err) {
        error.EndOfStream => 0,
        else => return error.InvalidPasswordFile,
    };
    if (extra_read != 0) return error.InvalidPasswordFile;
    var value = storage;
    if (value.len > jmcl.server_rcon.max_command_bytes + 2) return error.InvalidPasswordFile;
    if (std.mem.endsWith(u8, value, "\r\n")) value = value[0 .. value.len - 2] else if (std.mem.endsWith(u8, value, "\n")) value = value[0 .. value.len - 1];
    if (value.len == 0 or value.len > jmcl.server_rcon.max_command_bytes or std.mem.indexOfAny(u8, value, "\r\n\x00") != null or !std.unicode.utf8ValidateSlice(value)) return error.InvalidPasswordFile;
    return .{ .storage = storage, .value = value };
}

fn usageRcon(stderr: *std.Io.Writer) !u8 {
    try stderr.writeAll("usage: jmcl-core server rcon command ID --command TEXT --password-file PATH [--servers-directory PATH] [--max-bytes N]\n");
    return 2;
}
fn usageQuery(stderr: *std.Io.Writer) !u8 {
    try stderr.writeAll("usage: jmcl-core server query ID [--mode basic|full] [--servers-directory PATH] [--json]\n");
    return 2;
}
fn writeError(stderr: *std.Io.Writer, err: jmcl.Service.ServerRemoteError) std.Io.Writer.Error!void {
    const message = switch (err) {
        error.ServerNotRunning => "the server is not running",
        error.ServerRconDisabled => "RCON is not enabled for this server",
        error.ServerRconAuthenticationFailed => "RCON authentication failed",
        error.ServerRconUnavailable => "RCON is unavailable",
        error.ServerRconProtocolError => "RCON returned an invalid response",
        error.ServerQueryDisabled => "Query is not enabled for this server",
        error.ServerQueryUnavailable => "Query is unavailable",
        error.ServerQueryProtocolError => "Query returned an invalid response",
        error.ServerPropertiesInvalid => "server properties are invalid",
        error.InvalidServerRemoteInput => "server RCON command or password is invalid",
        else => "server remote operation failed",
    };
    try stderr.print("{s}\n", .{message});
}

test "RCON password files accept one LF or CRLF line and reject bad content" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    var path_buffer: [std.fs.max_path_bytes]u8 = undefined;
    const root_len = try temporary.dir.realPath(io, &path_buffer);
    const service = jmcl.Service.init(std.testing.allocator, io, null);
    inline for (.{ .{ "lf", "secret\n", "secret" }, .{ "crlf", "secret\r\n", "secret" } }) |case| {
        try temporary.dir.writeFile(io, .{ .sub_path = case[0], .data = case[1] });
        const path = try std.fmt.allocPrint(std.testing.allocator, "{s}/{s}", .{ path_buffer[0..root_len], case[0] });
        defer std.testing.allocator.free(path);
        const password = try readPasswordFile(&service, path);
        defer {
            std.crypto.secureZero(u8, password.storage);
            std.testing.allocator.free(password.storage);
        }
        try std.testing.expectEqualStrings(case[2], password.value);
    }
    inline for (.{ .{ "empty", "" }, .{ "multi", "one\ntwo\n" } }) |case| {
        try temporary.dir.writeFile(io, .{ .sub_path = case[0], .data = case[1] });
        const path = try std.fmt.allocPrint(std.testing.allocator, "{s}/{s}", .{ path_buffer[0..root_len], case[0] });
        defer std.testing.allocator.free(path);
        try std.testing.expectError(error.InvalidPasswordFile, readPasswordFile(&service, path));
    }
    const oversized = try std.testing.allocator.alloc(u8, jmcl.server_rcon.max_command_bytes + 3);
    defer std.testing.allocator.free(oversized);
    @memset(oversized, 'x');
    try temporary.dir.writeFile(io, .{ .sub_path = "oversized", .data = oversized });
    const oversized_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/oversized", .{path_buffer[0..root_len]});
    defer std.testing.allocator.free(oversized_path);
    try std.testing.expectError(error.InvalidPasswordFile, readPasswordFile(&service, oversized_path));
    if (@import("builtin").os.tag == .linux) {
        try temporary.dir.symLink(io, "lf", "password-link", .{});
        const link_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/password-link", .{path_buffer[0..root_len]});
        defer std.testing.allocator.free(link_path);
        try std.testing.expectError(error.InvalidPasswordFile, readPasswordFile(&service, link_path));
        try temporary.dir.createDir(io, "password-dir", .default_dir);
        const directory_path = try std.fmt.allocPrint(std.testing.allocator, "{s}/password-dir", .{path_buffer[0..root_len]});
        defer std.testing.allocator.free(directory_path);
        try std.testing.expectError(error.InvalidPasswordFile, readPasswordFile(&service, directory_path));
    }
}
