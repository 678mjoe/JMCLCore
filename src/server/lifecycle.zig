const std = @import("std");
const builtin = @import("builtin");
const hard_link = @import("../install/hard_link.zig");
const server_store = @import("server_store.zig");

pub const state_schema_version: u32 = 1;
pub const state_relative_path = ".jmcl/server.pid";
pub const fifo_relative_path = ".jmcl/console.fifo";
pub const lock_relative_path = ".jmcl/lifecycle.lock";
pub const log_relative_path = "logs/console.log";
pub const log_first_rotation = "logs/console.log.1";
pub const log_second_rotation = "logs/console.log.2";
pub const max_state_bytes: usize = 16 * 1024;
pub const max_command_bytes: usize = 4095;
pub const max_log_read_bytes: usize = 1024 * 1024;
pub const default_log_read_bytes: usize = 64 * 1024;
pub const max_log_file_bytes: u64 = 16 * 1024 * 1024;
pub const default_grace_ms: u32 = 10_000;
pub const max_grace_ms: u32 = 120_000;
const termination_grace_ms: u32 = 5_000;
const kill_wait_ms: u32 = 2_000;
const proc_cmdline_limit: usize = 64 * 1024;

pub const State = struct {
    schema_version: u32 = state_schema_version,
    pid: u32,
    supervisor_pid: u32,
    identity: []const u8,
    java_path: []const u8,
    started_at_ms: i64,
};

pub const Status = struct {
    allocator: ?std.mem.Allocator,
    running: bool,
    stale_state: bool,
    pid: ?u32,
    supervisor_pid: ?u32,
    java_path: ?[]u8,
    started_at_ms: ?i64,
    uptime_ms: ?i64,

    pub fn deinit(status_result: *Status) void {
        if (status_result.allocator) |allocator| {
            if (status_result.java_path) |java_path| allocator.free(java_path);
        }
        status_result.* = undefined;
    }
};

pub const StartResult = struct {
    pid: u32,
    supervisor_pid: u32,
    started_at_ms: i64,
};

pub const StopTermination = enum {
    graceful,
    terminated,
    killed,
    not_running,
};

pub const StopResult = struct {
    termination: StopTermination,
    pid: ?u32,
};

pub const Logs = struct {
    allocator: std.mem.Allocator,
    storage: []u8,
    text: []const u8,
    file_id: u64,
    start_cursor: u64,
    next_cursor: u64,
    eof: bool,
    reset: bool,
    truncated: bool,

    pub fn deinit(logs: *Logs) void {
        logs.allocator.free(logs.storage);
        logs.* = undefined;
    }
};

const StateDocument = std.json.Parsed(State);
pub const StartError = error{
    UnsupportedPlatform,
    InvalidJavaPath,
    ServerAlreadyRunning,
    ServerStartFailed,
};

pub const RestartError = error{
    UnsupportedPlatform,
    InvalidGracePeriod,
    InvalidJavaPath,
    ServerAlreadyRunning,
    ServerDidNotExit,
    ServerRestartFailed,
};

pub const StopError = error{
    UnsupportedPlatform,
    InvalidGracePeriod,
    ServerDidNotExit,
    ServerStopFailed,
};

pub const StatusError = error{
    UnsupportedPlatform,
    ServerStatusFailed,
};

pub const CommandError = error{
    UnsupportedPlatform,
    InvalidServerCommand,
    ServerNotRunning,
    ServerCommandUnavailable,
    ServerCommandFailed,
};

pub const ReadLogsError = error{
    UnsupportedPlatform,
    InvalidLogReadSize,
    ServerLogsNotFound,
    ServerLogsFailed,
};
pub const StoppedGuardError = error{
    UnsupportedPlatform,
    ServerAlreadyRunning,
    ServerStatusFailed,
};

pub fn buildLaunchArgv(
    allocator: std.mem.Allocator,
    java_path: []const u8,
    identity_argument: []const u8,
    mode: server_store.LaunchMode,
    launch_file: []const u8,
) error{OutOfMemory}![][]const u8 {
    switch (mode) {
        .jar => {
            const argv = try allocator.alloc([]const u8, 5);
            argv[0] = java_path;
            argv[1] = identity_argument;
            argv[2] = "-jar";
            argv[3] = launch_file;
            argv[4] = "nogui";
            return argv;
        },
        .args_file => {
            const argv = try allocator.alloc([]const u8, 4);
            argv[0] = java_path;
            argv[1] = identity_argument;
            argv[2] = try std.fmt.allocPrint(allocator, "@{s}", .{launch_file});
            argv[3] = "nogui";
            return argv;
        },
    }
}

pub const StoppedGuard = struct {
    io: std.Io,
    lock_file: std.Io.File,

    pub fn deinit(guard: *StoppedGuard) void {
        guard.lock_file.close(guard.io);
        guard.* = undefined;
    }
};

pub fn lockStopped(
    allocator: std.mem.Allocator,
    io: std.Io,
    server_dir: std.Io.Dir,
) StoppedGuardError!StoppedGuard {
    if (comptime builtin.os.tag != .linux) return error.UnsupportedPlatform;
    const lock_file = acquireLock(io, server_dir) catch return error.ServerStatusFailed;
    errdefer lock_file.close(io);
    var current = statusUnlocked(allocator, io, server_dir) catch return error.ServerStatusFailed;
    defer current.deinit();
    if (current.running) return error.ServerAlreadyRunning;
    return .{ .io = io, .lock_file = lock_file };
}

pub fn start(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    server_dir: std.Io.Dir,
    java_path: []const u8,
) StartError!StartResult {
    if (comptime builtin.os.tag != .linux) return error.UnsupportedPlatform;
    if (java_path.len == 0 or java_path.len > 4096 or std.mem.indexOfScalar(u8, java_path, 0) != null) {
        return error.InvalidJavaPath;
    }
    const lifecycle_lock = acquireLock(io, server_dir) catch return error.ServerStartFailed;
    defer lifecycle_lock.close(io);
    return startUnlocked(allocator, io, environ_map, server_dir, java_path) catch |err| switch (err) {
        error.ServerAlreadyRunning => error.ServerAlreadyRunning,
        else => error.ServerStartFailed,
    };
}

pub fn stop(
    allocator: std.mem.Allocator,
    io: std.Io,
    server_dir: std.Io.Dir,
    grace_ms: u32,
) StopError!StopResult {
    if (comptime builtin.os.tag != .linux) return error.UnsupportedPlatform;
    if (grace_ms > max_grace_ms) return error.InvalidGracePeriod;
    const lifecycle_lock = acquireLock(io, server_dir) catch return error.ServerStopFailed;
    defer lifecycle_lock.close(io);
    return stopUnlocked(allocator, io, server_dir, grace_ms) catch |err| switch (err) {
        error.ServerDidNotExit => error.ServerDidNotExit,
        else => error.ServerStopFailed,
    };
}

pub fn restart(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    server_dir: std.Io.Dir,
    java_path: []const u8,
    grace_ms: u32,
) RestartError!StartResult {
    if (comptime builtin.os.tag != .linux) return error.UnsupportedPlatform;
    if (grace_ms > max_grace_ms) return error.InvalidGracePeriod;
    if (java_path.len == 0 or java_path.len > 4096 or std.mem.indexOfScalar(u8, java_path, 0) != null) {
        return error.InvalidJavaPath;
    }
    const lifecycle_lock = acquireLock(io, server_dir) catch return error.ServerRestartFailed;
    defer lifecycle_lock.close(io);
    _ = stopUnlocked(allocator, io, server_dir, grace_ms) catch |err| switch (err) {
        error.ServerDidNotExit => return error.ServerDidNotExit,
        else => return error.ServerRestartFailed,
    };
    return startUnlocked(allocator, io, environ_map, server_dir, java_path) catch |err| switch (err) {
        error.ServerAlreadyRunning => error.ServerAlreadyRunning,
        else => error.ServerRestartFailed,
    };
}

pub fn status(
    allocator: std.mem.Allocator,
    io: std.Io,
    server_dir: std.Io.Dir,
) StatusError!Status {
    if (comptime builtin.os.tag != .linux) return error.UnsupportedPlatform;
    const lifecycle_lock = acquireLock(io, server_dir) catch return error.ServerStatusFailed;
    defer lifecycle_lock.close(io);
    return statusUnlocked(allocator, io, server_dir) catch return error.ServerStatusFailed;
}

pub fn command(
    allocator: std.mem.Allocator,
    io: std.Io,
    server_dir: std.Io.Dir,
    value: []const u8,
) CommandError!u32 {
    if (comptime builtin.os.tag != .linux) return error.UnsupportedPlatform;
    validateCommand(value) catch return error.InvalidServerCommand;
    const lifecycle_lock = acquireLock(io, server_dir) catch return error.ServerCommandFailed;
    defer lifecycle_lock.close(io);
    var current = statusUnlocked(allocator, io, server_dir) catch return error.ServerCommandFailed;
    defer current.deinit();
    if (!current.running) return error.ServerNotRunning;
    writeFifo(io, server_dir, value) catch |err| return switch (err) {
        error.ServerNotRunning => error.ServerNotRunning,
        error.ServerCommandUnavailable => error.ServerCommandUnavailable,
        else => error.ServerCommandFailed,
    };
    return current.pid.?;
}

pub fn readLogs(
    allocator: std.mem.Allocator,
    io: std.Io,
    server_dir: std.Io.Dir,
    cursor: ?u64,
    file_id: ?u64,
    max_bytes: usize,
) ReadLogsError!Logs {
    if (comptime builtin.os.tag != .linux) return error.UnsupportedPlatform;
    if (max_bytes == 0 or max_bytes > max_log_read_bytes) return error.InvalidLogReadSize;
    return readLogsInner(allocator, io, server_dir, cursor, file_id, max_bytes) catch |err| switch (err) {
        error.ServerLogsNotFound => error.ServerLogsNotFound,
        else => error.ServerLogsFailed,
    };
}

fn readLogsInner(
    allocator: std.mem.Allocator,
    io: std.Io,
    server_dir: std.Io.Dir,
    cursor: ?u64,
    file_id: ?u64,
    max_bytes: usize,
) !Logs {
    const fd = std.posix.openat(server_dir.handle, log_relative_path, .{
        .ACCMODE = .RDONLY,
        .NONBLOCK = true,
        .CLOEXEC = true,
        .NOFOLLOW = true,
    }, 0) catch |err| switch (err) {
        error.FileNotFound => return error.ServerLogsNotFound,
        else => |other| return other,
    };
    const file: std.Io.File = .{ .handle = fd, .flags = .{ .nonblocking = true } };
    defer file.close(io);
    const stat = try file.stat(io);
    if (stat.kind != .file) return error.InvalidServerLog;
    const inode: u64 = @intCast(stat.inode);
    const invalid_cursor = cursor != null and
        (cursor.? > stat.size or file_id == null or file_id.? != inode);
    const tail = cursor == null or invalid_cursor;
    const raw_start_cursor = if (tail) stat.size - @min(stat.size, max_bytes) else cursor.?;
    const available: usize = @intCast(@min(stat.size - raw_start_cursor, max_bytes));
    const storage = try allocator.alloc(u8, available);
    errdefer allocator.free(storage);
    var reader_buffer: [4096]u8 = undefined;
    var reader: std.Io.File.Reader = .init(file, io, &reader_buffer);
    try reader.seekTo(raw_start_cursor);
    var read: usize = 0;
    while (read < storage.len) {
        const count = reader.interface.readSliceShort(storage[read..]) catch return reader.err.?;
        if (count == 0) break;
        read += count;
    }
    if (read != storage.len) return error.ServerLogTruncated;
    const utf8_range = try logUtf8Range(storage, raw_start_cursor != 0);
    const text = storage[utf8_range.start..utf8_range.end];
    return .{
        .allocator = allocator,
        .storage = storage,
        .text = text,
        .file_id = inode,
        .start_cursor = raw_start_cursor + utf8_range.start,
        .next_cursor = raw_start_cursor + utf8_range.end,
        .eof = raw_start_cursor + utf8_range.end >= stat.size,
        .reset = invalid_cursor,
        .truncated = tail and raw_start_cursor != 0,
    };
}

const Utf8Range = struct {
    start: usize,
    end: usize,
};

fn logUtf8Range(bytes: []const u8, may_start_with_partial: bool) !Utf8Range {
    var aligned_start: usize = 0;
    if (may_start_with_partial) {
        while (aligned_start < bytes.len and bytes[aligned_start] & 0xc0 == 0x80) aligned_start += 1;
    }
    const aligned = bytes[aligned_start..];
    if (std.unicode.utf8ValidateSlice(aligned)) return .{ .start = aligned_start, .end = bytes.len };
    if (aligned.len == 0) return .{ .start = aligned_start, .end = bytes.len };
    var sequence_start = aligned.len - 1;
    while (sequence_start > 0 and aligned[sequence_start] & 0xc0 == 0x80 and
        aligned.len - sequence_start < 4)
    {
        sequence_start -= 1;
    }
    if (!std.unicode.utf8ValidateSlice(aligned[0..sequence_start])) {
        return error.InvalidServerLogEncoding;
    }
    const sequence_length = std.unicode.utf8ByteSequenceLength(aligned[sequence_start]) catch {
        return error.InvalidServerLogEncoding;
    };
    if (sequence_length <= aligned.len - sequence_start) return error.InvalidServerLogEncoding;
    return .{ .start = aligned_start, .end = aligned_start + sequence_start };
}

pub fn validateCommand(value: []const u8) !void {
    if (value.len == 0 or value.len > max_command_bytes or !std.unicode.utf8ValidateSlice(value)) {
        return error.InvalidServerCommand;
    }
    if (std.mem.indexOfAny(u8, value, "\x00\r\n") != null) return error.InvalidServerCommand;
}

pub fn runBootstrap(init: std.process.Init, args: []const []const u8) u8 {
    if (comptime builtin.os.tag != .linux) return 1;
    runBootstrapInner(init, args) catch |err| {
        writeHiddenError(err);
        return 1;
    };
    return 0;
}

pub fn runSupervisor(init: std.process.Init, args: []const []const u8) u8 {
    if (comptime builtin.os.tag != .linux) return 1;
    runSupervisorInner(init, args) catch |err| {
        writeHiddenError(err);
        return 1;
    };
    return 0;
}

fn startUnlocked(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    server_dir: std.Io.Dir,
    java_path: []const u8,
) !StartResult {
    var current = try statusUnlocked(allocator, io, server_dir);
    defer current.deinit();
    if (current.running) return error.ServerAlreadyRunning;
    try ensureFifo(io, server_dir);
    try server_dir.createDirPath(io, "logs");
    var random: [16]u8 = undefined;
    try std.Io.randomSecure(io, &random);
    const identity_array = std.fmt.bytesToHex(random, .lower);
    const identity: []const u8 = &identity_array;
    const server_absolute = try hard_link.absolutePathAlloc(allocator, io, server_dir, ".");
    defer allocator.free(server_absolute);
    const executable = try std.process.executablePathAlloc(io, allocator);
    defer allocator.free(executable);
    const argv = [_][]const u8{ executable, "__server_bootstrap", server_absolute, java_path, identity };
    const result = try std.process.run(allocator, io, .{
        .argv = &argv,
        .environ_map = environ_map,
        .stdout_limit = .limited(512),
        .stderr_limit = .limited(4096),
        .reserve_amount = 128,
    });
    defer allocator.free(result.stdout);
    defer allocator.free(result.stderr);
    switch (result.term) {
        .exited => |code| if (code != 0) return error.ServerBootstrapFailed,
        else => return error.ServerBootstrapFailed,
    }
    const response = std.mem.trim(u8, result.stdout, " \t\r\n");
    if (!std.mem.startsWith(u8, response, "OK ")) return error.ServerBootstrapFailed;
    const pid = std.fmt.parseInt(u32, response[3..], 10) catch return error.ServerBootstrapFailed;
    var state = (try loadState(allocator, io, server_dir)) orelse return error.ServerExitedDuringStart;
    defer state.deinit();
    if (state.value.pid != pid or !std.mem.eql(u8, state.value.identity, identity) or
        !try processMatches(allocator, io, state.value))
    {
        return error.ServerExitedDuringStart;
    }
    return .{ .pid = state.value.pid, .supervisor_pid = state.value.supervisor_pid, .started_at_ms = state.value.started_at_ms };
}

fn stopUnlocked(
    allocator: std.mem.Allocator,
    io: std.Io,
    server_dir: std.Io.Dir,
    grace_ms: u32,
) !StopResult {
    var state = (try loadState(allocator, io, server_dir)) orelse return .{ .termination = .not_running, .pid = null };
    defer state.deinit();
    if (!try processMatches(allocator, io, state.value)) {
        try deleteState(io, server_dir);
        return .{ .termination = .not_running, .pid = null };
    }
    const pid = state.value.pid;
    writeFifo(io, server_dir, "stop") catch {};
    if (try waitForExit(allocator, io, state.value, grace_ms)) {
        try deleteStateIfMatches(allocator, io, server_dir, state.value);
        return .{ .termination = .graceful, .pid = pid };
    }
    std.posix.kill(@intCast(pid), .TERM) catch |err| switch (err) {
        error.ProcessNotFound => {
            try deleteStateIfMatches(allocator, io, server_dir, state.value);
            return .{ .termination = .graceful, .pid = pid };
        },
        else => |other| return other,
    };
    if (try waitForExit(allocator, io, state.value, termination_grace_ms)) {
        try deleteStateIfMatches(allocator, io, server_dir, state.value);
        return .{ .termination = .terminated, .pid = pid };
    }
    std.posix.kill(@intCast(pid), .KILL) catch |err| switch (err) {
        error.ProcessNotFound => {},
        else => |other| return other,
    };
    if (!try waitForExit(allocator, io, state.value, kill_wait_ms)) return error.ServerDidNotExit;
    try deleteStateIfMatches(allocator, io, server_dir, state.value);
    return .{ .termination = .killed, .pid = pid };
}

fn statusUnlocked(allocator: std.mem.Allocator, io: std.Io, server_dir: std.Io.Dir) !Status {
    var document = loadState(allocator, io, server_dir) catch |err| switch (err) {
        error.InvalidLifecycleState => {
            try deleteState(io, server_dir);
            return stoppedStatus(true);
        },
        else => |other| return other,
    };
    if (document == null) return stoppedStatus(false);
    defer document.?.deinit();
    const state = document.?.value;
    if (!try processMatches(allocator, io, state)) {
        try deleteState(io, server_dir);
        return stoppedStatus(true);
    }
    const java_path = try allocator.dupe(u8, state.java_path);
    errdefer allocator.free(java_path);
    const now = std.Io.Timestamp.now(io, .real).toMilliseconds();
    return .{
        .allocator = allocator,
        .running = true,
        .stale_state = false,
        .pid = state.pid,
        .supervisor_pid = state.supervisor_pid,
        .java_path = java_path,
        .started_at_ms = state.started_at_ms,
        .uptime_ms = @max(0, now - state.started_at_ms),
    };
}

fn stoppedStatus(stale: bool) Status {
    return .{
        .allocator = null,
        .running = false,
        .stale_state = stale,
        .pid = null,
        .supervisor_pid = null,
        .java_path = null,
        .started_at_ms = null,
        .uptime_ms = null,
    };
}

fn acquireLock(io: std.Io, server_dir: std.Io.Dir) !std.Io.File {
    try server_dir.createDirPath(io, ".jmcl");
    return server_dir.createFile(io, lock_relative_path, .{
        .truncate = false,
        .lock = .exclusive,
        .permissions = .fromMode(0o600),
    });
}

fn ensureFifo(io: std.Io, server_dir: std.Io.Dir) !void {
    const stat = server_dir.statFile(io, fifo_relative_path, .{}) catch |err| switch (err) {
        error.FileNotFound => null,
        else => |other| return other,
    };
    if (stat) |existing| {
        if (existing.kind != .named_pipe) return error.InvalidConsoleFifo;
        return;
    }
    const fifo_z = fifo_relative_path ++ "\x00";
    const rc = std.os.linux.mknodat(server_dir.handle, fifo_z, std.os.linux.S.IFIFO | 0o600, 0);
    switch (std.posix.errno(rc)) {
        .SUCCESS => {},
        .EXIST => {
            const existing = try server_dir.statFile(io, fifo_relative_path, .{});
            if (existing.kind != .named_pipe) return error.InvalidConsoleFifo;
        },
        .ACCES, .PERM => return error.AccessDenied,
        .NOSPC => return error.NoSpaceLeft,
        else => return error.Unexpected,
    }
}

fn writeFifo(io: std.Io, server_dir: std.Io.Dir, value: []const u8) !void {
    var flags: std.posix.O = .{
        .ACCMODE = .WRONLY,
        .NONBLOCK = true,
        .CLOEXEC = true,
    };
    if (@hasField(std.posix.O, "NOFOLLOW")) flags.NOFOLLOW = true;
    const fd = std.posix.openat(server_dir.handle, fifo_relative_path, flags, 0) catch |err| switch (err) {
        error.FileNotFound, error.NoDevice, error.WouldBlock => return error.ServerNotRunning,
        else => |other| return other,
    };
    const file: std.Io.File = .{ .handle = fd, .flags = .{ .nonblocking = true } };
    defer file.close(io);
    const stat = try file.stat(io);
    if (stat.kind != .named_pipe) return error.InvalidConsoleFifo;
    var buffer: [max_command_bytes + 1]u8 = undefined;
    @memcpy(buffer[0..value.len], value);
    buffer[value.len] = '\n';
    writeFdAll(fd, buffer[0 .. value.len + 1]) catch return error.ServerCommandUnavailable;
}

fn loadState(allocator: std.mem.Allocator, io: std.Io, server_dir: std.Io.Dir) !?StateDocument {
    const bytes = server_dir.readFileAlloc(io, state_relative_path, allocator, .limited(max_state_bytes + 1)) catch |err| switch (err) {
        error.FileNotFound => return null,
        error.StreamTooLong => return error.InvalidLifecycleState,
        else => |other| return other,
    };
    defer allocator.free(bytes);
    if (bytes.len > max_state_bytes) return error.InvalidLifecycleState;
    var document = std.json.parseFromSlice(State, allocator, bytes, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_state_bytes,
    }) catch return error.InvalidLifecycleState;
    errdefer document.deinit();
    try validateState(document.value);
    return document;
}

fn validateState(state: State) !void {
    if (state.schema_version != state_schema_version or state.pid <= 1 or state.supervisor_pid <= 1 or
        state.identity.len != 32 or state.java_path.len == 0 or state.java_path.len > 4096 or
        state.started_at_ms <= 0 or !std.unicode.utf8ValidateSlice(state.java_path))
    {
        return error.InvalidLifecycleState;
    }
    for (state.identity) |byte| if (!std.ascii.isHex(byte)) return error.InvalidLifecycleState;
}

fn writeState(io: std.Io, server_dir: std.Io.Dir, state: State) !void {
    try validateState(state);
    var atomic = try server_dir.createFileAtomic(io, state_relative_path, .{ .make_path = true, .replace = true });
    defer atomic.deinit(io);
    var buffer: [4096]u8 = undefined;
    var writer: std.Io.File.Writer = .init(atomic.file, io, &buffer);
    var json: std.json.Stringify = .{ .writer = &writer.interface };
    try json.write(state);
    try writer.interface.writeByte('\n');
    try writer.interface.flush();
    try atomic.file.sync(io);
    try atomic.replace(io);
}

fn deleteState(io: std.Io, server_dir: std.Io.Dir) !void {
    server_dir.deleteFile(io, state_relative_path) catch |err| switch (err) {
        error.FileNotFound => {},
        else => |other| return other,
    };
}

fn deleteStateIfMatches(
    allocator: std.mem.Allocator,
    io: std.Io,
    server_dir: std.Io.Dir,
    expected: State,
) !void {
    var current = (try loadState(allocator, io, server_dir)) orelse return;
    defer current.deinit();
    if (current.value.pid == expected.pid and std.mem.eql(u8, current.value.identity, expected.identity)) {
        try deleteState(io, server_dir);
    }
}

fn processMatches(_: std.mem.Allocator, _: std.Io, state: State) !bool {
    var path_buffer: [64]u8 = undefined;
    const path = std.fmt.bufPrint(&path_buffer, "/proc/{d}/cmdline", .{state.pid}) catch unreachable;
    const fd = std.posix.openat(std.Io.Dir.cwd().handle, path, .{
        .ACCMODE = .RDONLY,
        .CLOEXEC = true,
    }, 0) catch |err| switch (err) {
        error.FileNotFound => return false,
        else => |other| return other,
    };
    defer _ = std.os.linux.close(fd);
    var bytes: [proc_cmdline_limit]u8 = undefined;
    var length: usize = 0;
    while (length < bytes.len) {
        const result = std.os.linux.read(fd, bytes[length..].ptr, bytes.len - length);
        switch (std.posix.errno(result)) {
            .SUCCESS => {
                if (result == 0) break;
                length += result;
            },
            .INTR => continue,
            else => return error.ReadProcessIdentityFailed,
        }
    }
    if (length == 0 or length == bytes.len) return false;
    var expected_buffer: [64]u8 = undefined;
    const expected = std.fmt.bufPrint(&expected_buffer, "-Djmcl.server.identity={s}", .{state.identity}) catch unreachable;
    var arguments = std.mem.splitScalar(u8, bytes[0..length], 0);
    while (arguments.next()) |argument| {
        if (std.mem.eql(u8, argument, expected)) return true;
    }
    return false;
}

fn waitForExit(
    allocator: std.mem.Allocator,
    io: std.Io,
    state: State,
    timeout_ms: u32,
) !bool {
    if (!try processMatches(allocator, io, state)) return true;
    const start_time = std.Io.Clock.Timestamp.now(io, .awake);
    while (start_time.durationTo(std.Io.Clock.Timestamp.now(io, .awake)).raw.toMilliseconds() < timeout_ms) {
        try std.Io.sleep(io, .fromMilliseconds(100), .awake);
        if (!try processMatches(allocator, io, state)) return true;
    }
    return false;
}

fn runBootstrapInner(init: std.process.Init, args: []const []const u8) !void {
    if (args.len != 3) return error.InvalidBootstrapArguments;
    if (std.posix.errno(std.os.linux.setsid()) != .SUCCESS) return error.SetSessionFailed;
    const executable = try std.process.executablePathAlloc(init.io, init.arena.allocator());
    const argv = [_][]const u8{ executable, "__server_supervisor", args[0], args[1], args[2] };
    var child = try std.process.spawn(init.io, .{
        .argv = &argv,
        .environ_map = init.environ_map,
        .stdin = .ignore,
        .stdout = .pipe,
        .stderr = .ignore,
        .create_no_window = true,
    });
    var buffer: [512]u8 = undefined;
    var reader: std.Io.File.Reader = .init(child.stdout.?, init.io, &buffer);
    const response = (try reader.interface.takeDelimiter('\n')) orelse return error.SupervisorHandshakeFailed;
    try writeFdAll(std.posix.STDOUT_FILENO, response);
    try writeFdAll(std.posix.STDOUT_FILENO, "\n");
    child.stdout.?.close(init.io);
    child.stdout = null;
    if (!std.mem.startsWith(u8, response, "OK ")) {
        _ = try child.wait(init.io);
        return error.SupervisorHandshakeFailed;
    }
}

fn runSupervisorInner(init: std.process.Init, args: []const []const u8) !void {
    if (args.len != 3) return error.InvalidSupervisorArguments;
    const server_absolute = args[0];
    const java_path = args[1];
    const identity = args[2];
    if (!std.fs.path.isAbsolute(server_absolute)) return error.InvalidSupervisorArguments;
    var server_dir = try std.Io.Dir.openDirAbsolute(init.io, server_absolute, .{ .iterate = true, .follow_symlinks = false });
    defer server_dir.close(init.io);
    const fifo_fd = try std.posix.openat(server_dir.handle, fifo_relative_path, .{
        .ACCMODE = .RDWR,
        .CLOEXEC = true,
        .NOFOLLOW = true,
    }, 0);
    const fifo: std.Io.File = .{ .handle = fifo_fd, .flags = .{ .nonblocking = false } };
    defer fifo.close(init.io);
    const identity_argument = try std.fmt.allocPrint(init.arena.allocator(), "-Djmcl.server.identity={s}", .{identity});
    var descriptor_arena = std.heap.ArenaAllocator.init(init.gpa);
    defer descriptor_arena.deinit();
    const installation = (server_store.loadInstallation(
        descriptor_arena.allocator(),
        init.io,
        server_dir,
    ) catch return error.LaunchDescriptorReadFailed) orelse return error.InvalidLaunchDescriptor;
    const argv = try buildLaunchArgv(
        descriptor_arena.allocator(),
        java_path,
        identity_argument,
        installation.launch_mode,
        installation.launch_file,
    );
    var child = try std.process.spawn(init.io, .{
        .argv = argv,
        .cwd = .{ .dir = server_dir },
        .environ_map = init.environ_map,
        .stdin = .{ .file = fifo },
        .stdout = .pipe,
        .stderr = .pipe,
        .create_no_window = true,
    });
    defer child.kill(init.io);
    const pid: u32 = @intCast(child.id.?);
    const state: State = .{
        .pid = pid,
        .supervisor_pid = @intCast(std.os.linux.getpid()),
        .identity = identity,
        .java_path = java_path,
        .started_at_ms = std.Io.Timestamp.now(init.io, .real).toMilliseconds(),
    };
    var log = try openLog(init.io, server_dir);
    defer log.file.close(init.io);
    var streams_buffer: std.Io.File.MultiReader.Buffer(2) = undefined;
    var streams: std.Io.File.MultiReader = undefined;
    streams.init(init.gpa, init.io, streams_buffer.toStreams(), &.{ child.stdout.?, child.stderr.? });
    defer streams.deinit();
    const stdout_reader = streams.reader(0);
    const stderr_reader = streams.reader(1);
    try writeState(init.io, server_dir, state);
    var cleanup_needed = true;
    defer if (cleanup_needed) cleanupSupervisorState(init, server_dir, state);
    var handshake_buffer: [64]u8 = undefined;
    const handshake = try std.fmt.bufPrint(&handshake_buffer, "OK {d}\n", .{pid});
    try writeFdAll(std.posix.STDOUT_FILENO, handshake);
    _ = std.os.linux.close(std.posix.STDOUT_FILENO);

    while (true) {
        streams.fill(4096, .none) catch |err| switch (err) {
            error.EndOfStream => break,
            else => |other| return other,
        };
        try drainToLog(init.io, server_dir, &log, stdout_reader);
        try drainToLog(init.io, server_dir, &log, stderr_reader);
    }
    try drainToLog(init.io, server_dir, &log, stdout_reader);
    try drainToLog(init.io, server_dir, &log, stderr_reader);
    try streams.checkAnyError();
    _ = try child.wait(init.io);
    try cleanupSupervisorStateFallible(init, server_dir, state);
    cleanup_needed = false;
}

const Log = struct {
    file: std.Io.File,
    size: u64,
};

fn openLog(io: std.Io, server_dir: std.Io.Dir) !Log {
    var flags: std.posix.O = .{
        .ACCMODE = .WRONLY,
        .APPEND = true,
        .CREAT = true,
        .CLOEXEC = true,
        .NONBLOCK = true,
    };
    if (@hasField(std.posix.O, "NOFOLLOW")) flags.NOFOLLOW = true;
    const fd = try std.posix.openat(server_dir.handle, log_relative_path, flags, 0o600);
    const file: std.Io.File = .{ .handle = fd, .flags = .{ .nonblocking = true } };
    errdefer file.close(io);
    const stat = try file.stat(io);
    if (stat.kind != .file) return error.InvalidServerLog;
    return .{ .file = file, .size = stat.size };
}

fn drainToLog(
    io: std.Io,
    server_dir: std.Io.Dir,
    log: *Log,
    reader: *std.Io.Reader,
) !void {
    const bytes = reader.buffered();
    if (bytes.len == 0) return;
    if (log.size >= max_log_file_bytes or bytes.len > max_log_file_bytes - log.size) {
        try rotateLogs(io, server_dir);
        const replacement = try openLog(io, server_dir);
        log.file.close(io);
        log.* = replacement;
    }
    try log.file.writeStreamingAll(io, bytes);
    log.size = std.math.add(u64, log.size, bytes.len) catch return error.ServerLogTooLarge;
    reader.tossBuffered();
}

fn rotateLogs(io: std.Io, server_dir: std.Io.Dir) !void {
    server_dir.deleteFile(io, log_second_rotation) catch |err| switch (err) {
        error.FileNotFound => {},
        else => |other| return other,
    };
    server_dir.rename(log_first_rotation, server_dir, log_second_rotation, io) catch |err| switch (err) {
        error.FileNotFound => {},
        else => |other| return other,
    };
    server_dir.rename(log_relative_path, server_dir, log_first_rotation, io) catch |err| switch (err) {
        error.FileNotFound => {},
        else => |other| return other,
    };
}

fn cleanupSupervisorState(init: std.process.Init, server_dir: std.Io.Dir, state: State) void {
    cleanupSupervisorStateFallible(init, server_dir, state) catch {};
}

fn cleanupSupervisorStateFallible(init: std.process.Init, server_dir: std.Io.Dir, state: State) !void {
    const lifecycle_lock = try acquireLock(init.io, server_dir);
    defer lifecycle_lock.close(init.io);
    try deleteStateIfMatches(init.gpa, init.io, server_dir, state);
}

fn writeHiddenError(err: anyerror) void {
    var buffer: [128]u8 = undefined;
    const line = std.fmt.bufPrint(&buffer, "ERR {s}\n", .{@errorName(err)}) catch return;
    writeFdAll(std.posix.STDOUT_FILENO, line) catch {};
}

fn writeFdAll(fd: std.posix.fd_t, bytes: []const u8) !void {
    var offset: usize = 0;
    while (offset < bytes.len) {
        const result = std.os.linux.write(fd, bytes[offset..].ptr, bytes.len - offset);
        switch (std.posix.errno(result)) {
            .SUCCESS => offset += result,
            .INTR => continue,
            .AGAIN => return error.WouldBlock,
            .PIPE => return error.BrokenPipe,
            else => return error.WriteFailed,
        }
    }
}
test "buildLaunchArgv places identity after java for jar mode" {
    const identity_argument = "-Djmcl.server.identity=0123456789abcdef0123456789abcdef";
    const argv = try buildLaunchArgv(
        std.testing.allocator,
        "/usr/bin/java",
        identity_argument,
        .jar,
        "server.jar",
    );
    defer std.testing.allocator.free(argv);
    try std.testing.expectEqual(@as(usize, 5), argv.len);
    try std.testing.expectEqualStrings("/usr/bin/java", argv[0]);
    try std.testing.expectEqualStrings(identity_argument, argv[1]);
    try std.testing.expectEqualStrings("-jar", argv[2]);
    try std.testing.expectEqualStrings("server.jar", argv[3]);
    try std.testing.expectEqualStrings("nogui", argv[4]);
}

test "buildLaunchArgv prefixes the args file token for args_file mode" {
    const identity_argument = "-Djmcl.server.identity=fedcba9876543210fedcba9876543210";
    const argv = try buildLaunchArgv(
        std.testing.allocator,
        "/usr/bin/java",
        identity_argument,
        .args_file,
        "libraries/net/neoforged/neoforge/21.1.133/unix_args.txt",
    );
    defer {
        std.testing.allocator.free(argv[2]);
        std.testing.allocator.free(argv);
    }
    try std.testing.expectEqual(@as(usize, 4), argv.len);
    try std.testing.expectEqualStrings("/usr/bin/java", argv[0]);
    try std.testing.expectEqualStrings(identity_argument, argv[1]);
    try std.testing.expectEqualStrings("@libraries/net/neoforged/neoforge/21.1.133/unix_args.txt", argv[2]);
    try std.testing.expectEqualStrings("nogui", argv[3]);
}

test "server command validation accepts one bounded UTF-8 line" {
    try validateCommand("say hello");
    try validateCommand("say 你好");
    try std.testing.expectError(error.InvalidServerCommand, validateCommand(""));
    try std.testing.expectError(error.InvalidServerCommand, validateCommand("list\nstop"));
    try std.testing.expectError(error.InvalidServerCommand, validateCommand("list\r"));
    try std.testing.expectError(error.InvalidServerCommand, validateCommand("bad\x00command"));
    var oversized: [max_command_bytes + 1]u8 = @splat('a');
    try std.testing.expectError(error.InvalidServerCommand, validateCommand(&oversized));
}

test "log cursors preserve UTF-8 boundaries" {
    const valid = "你a";
    try std.testing.expectEqual(Utf8Range{ .start = 0, .end = valid.len }, try logUtf8Range(valid, false));
    try std.testing.expectEqual(Utf8Range{ .start = 2, .end = 3 }, try logUtf8Range(valid[1..], true));
    try std.testing.expectEqual(Utf8Range{ .start = 0, .end = 2 }, try logUtf8Range("ok\xe4\xbd", false));
    try std.testing.expectError(error.InvalidServerLogEncoding, logUtf8Range("ok\xff", false));
}

test "process identity requires the exact lifecycle token" {
    if (comptime builtin.os.tag != .linux) return error.SkipZigTest;
    const identity = "0123456789abcdef0123456789abcdef";
    const identity_argument = "-Djmcl.server.identity=" ++ identity;
    var child = try std.process.spawn(std.testing.io, .{
        .argv = &.{ "/bin/sh", "-c", "while :; do sleep 1; done", identity_argument },
        .stdin = .ignore,
        .stdout = .ignore,
        .stderr = .ignore,
    });
    defer child.kill(std.testing.io);
    const state: State = .{
        .pid = @intCast(child.id.?),
        .supervisor_pid = @intCast(child.id.?),
        .identity = identity,
        .java_path = "/bin/sh",
        .started_at_ms = 1,
    };
    try std.testing.expect(try processMatches(std.testing.allocator, std.testing.io, state));
    var changed = state;
    changed.identity = "fedcba9876543210fedcba9876543210";
    try std.testing.expect(!try processMatches(std.testing.allocator, std.testing.io, changed));
}
