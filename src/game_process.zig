const std = @import("std");
const builtin = @import("builtin");

const max_log_payload = 16 * 1024;

pub const Stream = enum {
    stdout,
    stderr,
};

pub const RecordEnd = enum {
    newline,
    eof,
    more,
};

pub const LogEvent = struct {
    stream: Stream,
    bytes: []const u8,
    end: RecordEnd,
};

pub const StartedEvent = struct {
    pid: u64,
};

pub const Event = union(enum) {
    started: StartedEvent,
    log: LogEvent,
};

pub const Reporter = struct {
    context: *anyopaque,
    callback: *const fn (context: *anyopaque, event: Event) void,

    fn emit(reporter: Reporter, event: Event) void {
        reporter.callback(reporter.context, event);
    }
};

pub const Termination = enum {
    exited,
    signal,
    stopped,
    unknown,
};

pub const Result = struct {
    pid: u64,
    termination: Termination,
    exit_code: ?u8,
    signal: ?u32,
    status: ?u32,
    duration_ms: i64,
    max_rss: ?usize,
};

const Framer = struct {
    stream: Stream,
    buffer: [max_log_payload]u8 = undefined,
    len: usize = 0,

    fn feed(framer: *Framer, bytes: []const u8, reporter: Reporter) void {
        for (bytes) |byte| {
            if (byte == '\n') {
                reporter.emit(.{ .log = .{
                    .stream = framer.stream,
                    .bytes = trimCarriageReturn(framer.buffer[0..framer.len]),
                    .end = .newline,
                } });
                framer.len = 0;
                continue;
            }
            if (framer.len == framer.buffer.len) {
                reporter.emit(.{ .log = .{
                    .stream = framer.stream,
                    .bytes = framer.buffer[0..framer.len],
                    .end = .more,
                } });
                framer.len = 0;
            }
            framer.buffer[framer.len] = byte;
            framer.len += 1;
        }
    }

    fn finish(framer: *Framer, reporter: Reporter) void {
        if (framer.len == 0) return;
        reporter.emit(.{ .log = .{
            .stream = framer.stream,
            .bytes = trimCarriageReturn(framer.buffer[0..framer.len]),
            .end = .eof,
        } });
        framer.len = 0;
    }
};

const Redactor = struct {
    allocator: std.mem.Allocator,
    pending: std.ArrayList(u8) = .empty,
    secrets: []const []const u8,
    max_secret_len: usize,

    fn init(allocator: std.mem.Allocator, secrets: []const []const u8) Redactor {
        var maximum: usize = 1;
        for (secrets) |secret| maximum = @max(maximum, secret.len);
        return .{
            .allocator = allocator,
            .secrets = secrets,
            .max_secret_len = maximum,
        };
    }

    fn deinit(redactor: *Redactor) void {
        redactor.pending.deinit(redactor.allocator);
    }

    fn feed(redactor: *Redactor, bytes: []const u8, framer: *Framer, reporter: Reporter) !void {
        try redactor.pending.appendSlice(redactor.allocator, bytes);
        redactor.process(false, framer, reporter);
    }

    fn finish(redactor: *Redactor, framer: *Framer, reporter: Reporter) void {
        redactor.process(true, framer, reporter);
        framer.finish(reporter);
    }

    fn process(redactor: *Redactor, final: bool, framer: *Framer, reporter: Reporter) void {
        while (redactor.pending.items.len != 0) {
            const match = redactor.findSecret();
            if (match) |found| {
                if (found.start != 0) framer.feed(redactor.pending.items[0..found.start], reporter);
                framer.feed("<redacted>", reporter);
                redactor.consume(found.start + found.len);
                continue;
            }

            const keep = if (final) 0 else @min(redactor.pending.items.len, redactor.max_secret_len - 1);
            const emit_len = redactor.pending.items.len - keep;
            if (emit_len == 0) return;
            framer.feed(redactor.pending.items[0..emit_len], reporter);
            redactor.consume(emit_len);
        }
    }

    const Match = struct { start: usize, len: usize };

    fn findSecret(redactor: Redactor) ?Match {
        var best: ?Match = null;
        for (redactor.secrets) |secret| {
            if (secret.len == 0) continue;
            const start = std.mem.indexOf(u8, redactor.pending.items, secret) orelse continue;
            if (best == null or start < best.?.start or
                (start == best.?.start and secret.len > best.?.len))
            {
                best = .{ .start = start, .len = secret.len };
            }
        }
        return best;
    }

    fn consume(redactor: *Redactor, count: usize) void {
        const remaining = redactor.pending.items.len - count;
        if (remaining != 0) {
            @memmove(redactor.pending.items[0..remaining], redactor.pending.items[count..]);
        }
        redactor.pending.items.len = remaining;
    }
};

pub fn run(
    allocator: std.mem.Allocator,
    io: std.Io,
    parent_environment: *const std.process.Environ.Map,
    argv: []const []const u8,
    working_directory: []const u8,
    secrets: []const []const u8,
    reporter: Reporter,
) !Result {
    if (argv.len == 0) return error.EmptyArgv;
    if (working_directory.len == 0) return error.EmptyWorkingDirectory;
    for (secrets) |secret| {
        if (secret.len > 16 * 1024) return error.SecretTooLong;
    }

    var child_environment = try buildEnvironment(allocator, parent_environment);
    defer child_environment.deinit();
    var child = try std.process.spawn(io, .{
        .argv = argv,
        .cwd = .{ .path = working_directory },
        .environ_map = &child_environment,
        .stdin = .ignore,
        .stdout = .pipe,
        .stderr = .pipe,
        .request_resource_usage_statistics = true,
        .create_no_window = true,
    });
    defer child.kill(io);

    const pid = try numericPid(&child);
    reporter.emit(.{ .started = .{ .pid = pid } });
    const start = std.Io.Clock.awake.now(io);

    var streams_buffer: std.Io.File.MultiReader.Buffer(2) = undefined;
    var streams: std.Io.File.MultiReader = undefined;
    streams.init(allocator, io, streams_buffer.toStreams(), &.{ child.stdout.?, child.stderr.? });
    defer streams.deinit();
    const stdout_reader = streams.reader(0);
    const stderr_reader = streams.reader(1);
    var stdout_framer: Framer = .{ .stream = .stdout };
    var stderr_framer: Framer = .{ .stream = .stderr };
    var stdout_redactor = Redactor.init(allocator, secrets);
    defer stdout_redactor.deinit();
    var stderr_redactor = Redactor.init(allocator, secrets);
    defer stderr_redactor.deinit();

    while (true) {
        streams.fill(4096, .none) catch |err| switch (err) {
            error.EndOfStream => break,
            else => |other| return other,
        };
        try drainReader(stdout_reader, &stdout_redactor, &stdout_framer, reporter);
        try drainReader(stderr_reader, &stderr_redactor, &stderr_framer, reporter);
    }
    try drainReader(stdout_reader, &stdout_redactor, &stdout_framer, reporter);
    try drainReader(stderr_reader, &stderr_redactor, &stderr_framer, reporter);
    try streams.checkAnyError();
    stdout_redactor.finish(&stdout_framer, reporter);
    stderr_redactor.finish(&stderr_framer, reporter);

    const term = try child.wait(io);
    const duration = start.durationTo(std.Io.Clock.awake.now(io));
    const base: Result = .{
        .pid = pid,
        .termination = .unknown,
        .exit_code = null,
        .signal = null,
        .status = null,
        .duration_ms = duration.toMilliseconds(),
        .max_rss = child.resource_usage_statistics.getMaxRss(),
    };
    return switch (term) {
        .exited => |code| result: {
            var result = base;
            result.termination = .exited;
            result.exit_code = code;
            break :result result;
        },
        .signal => |signal| result: {
            var result = base;
            result.termination = .signal;
            result.signal = @intFromEnum(signal);
            break :result result;
        },
        .stopped => |signal| result: {
            var result = base;
            result.termination = .stopped;
            result.signal = @intFromEnum(signal);
            break :result result;
        },
        .unknown => |status| result: {
            var result = base;
            result.status = status;
            break :result result;
        },
    };
}

fn drainReader(
    reader: *std.Io.Reader,
    redactor: *Redactor,
    framer: *Framer,
    reporter: Reporter,
) !void {
    try redactor.feed(reader.buffered(), framer, reporter);
    reader.tossBuffered();
}

fn buildEnvironment(
    allocator: std.mem.Allocator,
    parent: *const std.process.Environ.Map,
) !std.process.Environ.Map {
    var environment = std.process.Environ.Map.init(allocator);
    errdefer environment.deinit();
    const allowed = [_][]const u8{
        "HOME",         "USER",                    "LOGNAME",     "PATH",    "TMPDIR",       "TMP",         "TEMP",    "LANG",            "LC_ALL",     "LC_CTYPE",        "TZ",
        "SystemRoot",   "WINDIR",                  "USERPROFILE", "APPDATA", "LOCALAPPDATA", "PROGRAMDATA", "DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS",
        "PULSE_SERVER", "__CF_USER_TEXT_ENCODING",
    };
    for (allowed) |name| {
        if (parent.get(name)) |value| try environment.put(name, value);
    }
    return environment;
}

fn numericPid(child: *const std.process.Child) !u64 {
    return switch (builtin.os.tag) {
        .windows => pid: {
            const windows = std.os.windows;
            var info: windows.PROCESS.BASIC_INFORMATION = undefined;
            const status = windows.ntdll.NtQueryInformationProcess(
                child.id.?,
                .BasicInformation,
                &info,
                @sizeOf(windows.PROCESS.BASIC_INFORMATION),
                null,
            );
            if (status != .SUCCESS) return error.PidQueryFailed;
            break :pid @intCast(info.UniqueProcessId);
        },
        .wasi => error.OperationUnsupported,
        else => @intCast(child.id.?),
    };
}

fn trimCarriageReturn(bytes: []const u8) []const u8 {
    return if (bytes.len != 0 and bytes[bytes.len - 1] == '\r') bytes[0 .. bytes.len - 1] else bytes;
}

test "redacts secrets split across input chunks" {
    const Collector = struct {
        output: std.ArrayList(u8) = .empty,

        fn callback(raw: *anyopaque, event: Event) void {
            const collector: *@This() = @ptrCast(@alignCast(raw));
            switch (event) {
                .log => |log| collector.output.appendSlice(std.testing.allocator, log.bytes) catch unreachable,
                .started => {},
            }
        }
    };
    var collector: Collector = .{};
    defer collector.output.deinit(std.testing.allocator);
    const reporter: Reporter = .{ .context = &collector, .callback = Collector.callback };
    var framer: Framer = .{ .stream = .stdout };
    var redactor = Redactor.init(std.testing.allocator, &.{"secret-token"});
    defer redactor.deinit();
    try redactor.feed("prefix secret-", &framer, reporter);
    try redactor.feed("token suffix", &framer, reporter);
    redactor.finish(&framer, reporter);
    try std.testing.expectEqualStrings("prefix <redacted> suffix", collector.output.items);
}

test "runs a child and captures both output streams" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;
    const Collector = struct {
        started: bool = false,
        stdout_seen: bool = false,
        stderr_seen: bool = false,

        fn callback(raw: *anyopaque, event: Event) void {
            const collector: *@This() = @ptrCast(@alignCast(raw));
            switch (event) {
                .started => collector.started = true,
                .log => |log| {
                    if (log.stream == .stdout and std.mem.eql(u8, log.bytes, "out")) collector.stdout_seen = true;
                    if (log.stream == .stderr and std.mem.eql(u8, log.bytes, "err <redacted>")) collector.stderr_seen = true;
                },
            }
        }
    };
    var environment = std.process.Environ.Map.init(std.testing.allocator);
    defer environment.deinit();
    var collector: Collector = .{};
    const result = try run(
        std.testing.allocator,
        std.testing.io,
        &environment,
        &.{ "/bin/sh", "-c", "echo out; echo 'err token' >&2; exit 7" },
        "/tmp",
        &.{"token"},
        .{ .context = &collector, .callback = Collector.callback },
    );
    try std.testing.expect(collector.started);
    try std.testing.expect(collector.stdout_seen);
    try std.testing.expect(collector.stderr_seen);
    try std.testing.expectEqual(Termination.exited, result.termination);
    try std.testing.expectEqual(@as(?u8, 7), result.exit_code);
}
