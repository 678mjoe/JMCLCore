const std = @import("std");
const artifact_store = @import("artifact_store.zig");
const hard_link = @import("hard_link.zig");
const https_fallback = @import("https_fallback.zig");
const install_plan = @import("plan.zig");

const Sha1 = std.crypto.hash.Sha1;

pub const progress_byte_interval: u64 = 256 * 1024;
pub const progress_time_interval_ms: i64 = 100;

pub const Options = struct {
    workers: u8 = 8,
    retries: u8 = 2,
    sync_files: bool = true,
};

pub const FileStatus = enum {
    cached,
    downloaded,
};

pub const FileEvent = struct {
    path: []const u8,
    status: FileStatus,
    placement: artifact_store.Placement,
    size: u64,
    attempts: u8,
};

pub const RetryEvent = struct {
    path: []const u8,
    attempt: u8,
    max_attempts: u8,
    error_name: []const u8,
};

pub const Progress = struct {
    files_completed: usize,
    files_total: usize,
    bytes_verified: u64,
    bytes_processed: u64,
    bytes_total: u64,
    bytes_transferred: u64,
};

pub const Event = union(enum) {
    file: FileEvent,
    retry: RetryEvent,
    progress: Progress,
};

pub const Reporter = struct {
    context: *anyopaque,
    callback: *const fn (context: *anyopaque, event: Event) void,

    fn emit(reporter: ?Reporter, event: Event) void {
        if (reporter) |value| value.callback(value.context, event);
    }
};

pub const Summary = struct {
    files_total: usize,
    files_downloaded: usize,
    files_cached: usize,
    files_hard_linked: usize,
    files_copied: usize,
    bytes_verified: u64,
    bytes_transferred: u64,
};

const SinkFailure = enum {
    too_long,
    downstream,
};

pub const ByteProgressReporter = struct {
    context: *anyopaque,
    callback: *const fn (context: *anyopaque, received: u64) void,
    received: u64 = 0,
    last_reported: u64 = 0,

    pub fn add(reporter: *ByteProgressReporter, count: usize) void {
        reporter.received += count;
        if (reporter.received - reporter.last_reported < progress_byte_interval) return;
        reporter.last_reported = reporter.received;
        reporter.callback(reporter.context, reporter.received);
    }
};

pub const LimitedWriter = struct {
    out: *std.Io.Writer,
    remaining: usize,
    progress: *ByteProgressReporter,
    failure: ?SinkFailure = null,
    interface: std.Io.Writer,

    pub fn init(
        out: *std.Io.Writer,
        backing: []u8,
        max_bytes: usize,
        progress: *ByteProgressReporter,
    ) LimitedWriter {
        std.debug.assert(backing.len != 0);
        return .{
            .out = out,
            .remaining = max_bytes,
            .progress = progress,
            .interface = .{
                .vtable = &.{ .drain = drain },
                .buffer = backing[0..@min(backing.len, max_bytes +| 1)],
            },
        };
    }

    fn consumeBuffered(writer: *std.Io.Writer, count: usize) void {
        const remaining = writer.end - count;
        if (remaining != 0) @memmove(writer.buffer[0..remaining], writer.buffer[count..writer.end]);
        writer.end = remaining;
    }

    fn drain(
        io_writer: *std.Io.Writer,
        data: []const []const u8,
        splat: usize,
    ) std.Io.Writer.Error!usize {
        const self: *LimitedWriter = @alignCast(@fieldParentPtr("interface", io_writer));

        while (io_writer.end != 0) {
            const allowed = @min(io_writer.end, self.remaining);
            if (allowed == 0) {
                self.failure = .too_long;
                return error.WriteFailed;
            }
            const written = self.out.write(io_writer.buffer[0..allowed]) catch {
                self.failure = .downstream;
                return error.WriteFailed;
            };
            consumeBuffered(io_writer, written);
            self.remaining -= written;
            self.progress.add(written);
            if (written < allowed) return 0;
            if (io_writer.end != 0) {
                self.failure = .too_long;
                return error.WriteFailed;
            }
        }

        for (data[0 .. data.len - 1]) |bytes| {
            if (bytes.len != 0) return self.writeData(bytes);
        }
        const pattern = data[data.len - 1];
        if (splat == 0 or pattern.len == 0) return 0;
        return self.writeData(pattern);
    }

    fn writeData(self: *LimitedWriter, bytes: []const u8) std.Io.Writer.Error!usize {
        const allowed = @min(bytes.len, self.remaining);
        if (allowed == 0) {
            self.failure = .too_long;
            return error.WriteFailed;
        }
        const written = self.out.write(bytes[0..allowed]) catch {
            self.failure = .downstream;
            return error.WriteFailed;
        };
        self.remaining -= written;
        self.progress.add(written);
        if (written == allowed and allowed < bytes.len) {
            self.failure = .too_long;
            return error.WriteFailed;
        }
        return written;
    }
};

const Shared = struct {
    mutex: std.Io.Mutex = .init,
    progress_mutex: std.Io.Mutex = .init,
    next_index: usize = 0,
    failure: ?anyerror = null,
    summary: Summary,
    progress: Progress,
    file_progress: []u64,
    last_stream_report: ?std.Io.Timestamp = null,
    allocator: std.mem.Allocator,
    io: std.Io,
    root: std.Io.Dir,
    store: std.Io.Dir,
    environ_map: ?*const std.process.Environ.Map,
    files: []const install_plan.File,
    options: Options,
    reporter: ?Reporter,

    fn takeJob(shared: *Shared) ?usize {
        shared.mutex.lockUncancelable(shared.io);
        defer shared.mutex.unlock(shared.io);
        if (shared.failure != null or shared.next_index == shared.files.len) return null;
        defer shared.next_index += 1;
        return shared.next_index;
    }

    fn fail(shared: *Shared, failure: anyerror) void {
        shared.mutex.lockUncancelable(shared.io);
        defer shared.mutex.unlock(shared.io);
        if (shared.failure == null) shared.failure = failure;
    }

    fn reportBytes(shared: *Shared, index: usize, received: u64) void {
        shared.progress_mutex.lockUncancelable(shared.io);
        defer shared.progress_mutex.unlock(shared.io);
        shared.mutex.lockUncancelable(shared.io);
        const advanced = advanceFileProgress(
            &shared.file_progress[index],
            &shared.progress.bytes_processed,
            received,
        );
        const progress = shared.progress;
        shared.mutex.unlock(shared.io);
        if (!advanced) return;

        const now = std.Io.Clock.awake.now(shared.io);
        if (shared.last_stream_report) |last| {
            if (last.durationTo(now).nanoseconds <
                @as(i96, progress_time_interval_ms) * std.time.ns_per_ms) return;
        }
        shared.last_stream_report = now;
        Reporter.emit(shared.reporter, .{ .progress = progress });
    }

    fn complete(
        shared: *Shared,
        index: usize,
        file: install_plan.File,
        status: FileStatus,
        placement: artifact_store.Placement,
        attempts: u8,
    ) void {
        shared.progress_mutex.lockUncancelable(shared.io);
        defer shared.progress_mutex.unlock(shared.io);
        shared.mutex.lockUncancelable(shared.io);
        _ = advanceFileProgress(
            &shared.file_progress[index],
            &shared.progress.bytes_processed,
            file.size,
        );
        if (status == .cached) {
            shared.summary.files_cached += 1;
        } else {
            shared.summary.files_downloaded += 1;
            shared.summary.bytes_transferred += file.size;
            shared.progress.bytes_transferred += file.size;
        }
        switch (placement) {
            .hard_link => shared.summary.files_hard_linked += 1,
            .copy => shared.summary.files_copied += 1,
        }
        shared.summary.bytes_verified += file.size;
        shared.progress.files_completed += 1;
        shared.progress.bytes_verified += file.size;
        const progress = shared.progress;
        shared.mutex.unlock(shared.io);

        shared.last_stream_report = std.Io.Clock.awake.now(shared.io);
        Reporter.emit(shared.reporter, .{ .file = .{
            .path = file.path,
            .status = status,
            .placement = placement,
            .size = file.size,
            .attempts = attempts,
        } });
        Reporter.emit(shared.reporter, .{ .progress = progress });
    }
};

fn advanceFileProgress(current: *u64, aggregate: *u64, observed: u64) bool {
    if (observed <= current.*) return false;
    aggregate.* += observed - current.*;
    current.* = observed;
    return true;
}

pub fn openRoot(io: std.Io, path: []const u8) !std.Io.Dir {
    if (path.len == 0) return error.EmptyInstallRoot;
    return std.Io.Dir.cwd().createDirPathOpen(io, path, .{});
}

pub fn execute(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: ?*const std.process.Environ.Map,
    root: std.Io.Dir,
    store: std.Io.Dir,
    plan: install_plan.Data,
    options: Options,
    reporter: ?Reporter,
) !Summary {
    if (options.workers == 0 or options.workers > 32) return error.InvalidWorkerCount;
    if (options.retries > 5) return error.InvalidRetryCount;
    if (plan.files.len == 0) return .{
        .files_total = 0,
        .files_downloaded = 0,
        .files_cached = 0,
        .files_hard_linked = 0,
        .files_copied = 0,
        .bytes_verified = 0,
        .bytes_transferred = 0,
    };

    const file_progress = try allocator.alloc(u64, plan.files.len);
    defer allocator.free(file_progress);
    @memset(file_progress, 0);

    var shared: Shared = .{
        .summary = .{
            .files_total = plan.files.len,
            .files_downloaded = 0,
            .files_cached = 0,
            .files_hard_linked = 0,
            .files_copied = 0,
            .bytes_verified = 0,
            .bytes_transferred = 0,
        },
        .progress = .{
            .files_completed = 0,
            .files_total = plan.files.len,
            .bytes_verified = 0,
            .bytes_processed = 0,
            .bytes_total = plan.total_download_size,
            .bytes_transferred = 0,
        },
        .file_progress = file_progress,
        .allocator = allocator,
        .io = io,
        .root = root,
        .store = store,
        .environ_map = environ_map,
        .files = plan.files,
        .options = options,
        .reporter = reporter,
    };

    const worker_count = @min(@as(usize, options.workers), plan.files.len);
    if (worker_count == 1) {
        worker(&shared);
        if (shared.failure) |failure| return failure;
        return shared.summary;
    }
    const threads = try allocator.alloc(std.Thread, worker_count);
    defer allocator.free(threads);

    var spawned: usize = 0;
    errdefer for (threads[0..spawned]) |thread| thread.join();
    while (spawned < worker_count) : (spawned += 1) {
        threads[spawned] = try std.Thread.spawn(.{}, worker, .{&shared});
    }
    for (threads) |thread| thread.join();
    spawned = 0;

    if (shared.failure) |failure| return failure;
    return shared.summary;
}

const DownloadProgressContext = struct {
    shared: *Shared,
    index: usize,

    fn callback(raw_context: *anyopaque, received: u64) void {
        const context: *DownloadProgressContext = @ptrCast(@alignCast(raw_context));
        context.shared.reportBytes(context.index, received);
    }
};

fn worker(shared: *Shared) void {
    var proxy_arena = std.heap.ArenaAllocator.init(shared.allocator);
    defer proxy_arena.deinit();
    var client: std.http.Client = .{
        .allocator = shared.allocator,
        .io = shared.io,
    };
    defer client.deinit();
    if (shared.environ_map) |environment| {
        client.initDefaultProxies(proxy_arena.allocator(), environment) catch |err| {
            shared.fail(err);
            return;
        };
    }

    while (shared.takeJob()) |index| {
        const file = shared.files[index];
        const destination_valid = artifact_store.verify(shared.root, shared.io, file.path, file.sha1, file.size) catch |err| {
            shared.fail(err);
            break;
        };
        var object_path_buffer: [64]u8 = undefined;
        const object_path = artifact_store.objectPath(file.sha1, &object_path_buffer) catch |err| {
            shared.fail(err);
            break;
        };
        var object_lock = artifact_store.lockObject(shared.io, shared.store, file.sha1) catch |err| {
            shared.fail(err);
            break;
        };
        defer object_lock.deinit();

        var downloaded = false;
        var attempts: u8 = 0;
        const object_valid = artifact_store.verify(shared.store, shared.io, object_path, file.sha1, file.size) catch |err| {
            shared.fail(err);
            break;
        };
        if (!object_valid) {
            shared.store.deleteFile(shared.io, object_path) catch |err| switch (err) {
                error.FileNotFound => {},
                else => {
                    shared.fail(err);
                    break;
                },
            };
            if (destination_valid) {
                _ = artifact_store.importObject(
                    shared.allocator,
                    shared.io,
                    shared.store,
                    object_path,
                    shared.root,
                    file.path,
                ) catch |err| {
                    shared.fail(err);
                    break;
                };
            } else {
                const max_attempts: u8 = shared.options.retries + 1;
                var attempt: u8 = 0;
                var acquisition_failed = false;
                while (attempt < max_attempts) : (attempt += 1) {
                    const url = if (file.fallback_url != null and max_attempts > 1 and attempt + 1 == max_attempts)
                        file.fallback_url.?
                    else
                        file.url;
                    var progress_context: DownloadProgressContext = .{
                        .shared = shared,
                        .index = index,
                    };
                    var byte_reporter: ByteProgressReporter = .{
                        .context = &progress_context,
                        .callback = DownloadProgressContext.callback,
                    };
                    downloadOnce(
                        &client,
                        shared.store,
                        object_path,
                        file,
                        url,
                        shared.options.sync_files,
                        &byte_reporter,
                    ) catch |err| {
                        if (attempt + 1 < max_attempts) {
                            Reporter.emit(shared.reporter, .{ .retry = .{
                                .path = file.path,
                                .attempt = attempt + 1,
                                .max_attempts = max_attempts,
                                .error_name = @errorName(err),
                            } });
                            std.Io.sleep(shared.io, .fromMilliseconds(200 * (@as(i64, attempt) + 1)), .awake) catch {};
                            continue;
                        }
                        shared.fail(err);
                        acquisition_failed = true;
                        break;
                    };
                    artifact_store.setReadOnly(shared.allocator, shared.store, shared.io, object_path) catch |err| {
                        shared.fail(err);
                        acquisition_failed = true;
                        break;
                    };
                    downloaded = true;
                    attempts = attempt + 1;
                    break;
                }
                if (acquisition_failed) break;
            }
        }

        const placement = artifact_store.materialize(
            shared.allocator,
            shared.io,
            shared.store,
            object_path,
            shared.root,
            file.path,
            destination_valid,
        ) catch |err| {
            shared.fail(err);
            break;
        };
        shared.complete(index, file, if (downloaded) .downloaded else .cached, placement, attempts);
    }
}

fn downloadOnce(
    client: *std.http.Client,
    root: std.Io.Dir,
    destination_path: []const u8,
    expected: install_plan.File,
    url: []const u8,
    sync_file: bool,
    progress: *ByteProgressReporter,
) !void {
    var atomic_file = try root.createFileAtomic(client.io, destination_path, .{
        .make_path = true,
        .replace = true,
    });
    defer atomic_file.deinit(client.io);

    var file_buffer: [64 * 1024]u8 = undefined;
    var file_writer: std.Io.File.Writer = .init(atomic_file.file, client.io, &file_buffer);
    var hashed = std.Io.Writer.Hashed(Sha1).initHasher(
        &file_writer.interface,
        Sha1.init(.{}),
        &.{},
    );
    var limit_buffer: [64 * 1024]u8 = undefined;
    const maximum = std.math.cast(usize, expected.size) orelse return error.FileTooLarge;
    var limited = LimitedWriter.init(&hashed.writer, &limit_buffer, maximum, progress);

    const status: std.http.Status = blk: {
        const result = client.fetch(.{
            .location = .{ .url = url },
            .response_writer = &limited.interface,
            .redirect_behavior = std.http.Client.Request.RedirectBehavior.init(5),
        }) catch |err| switch (err) {
            error.WriteFailed => return diagnosedWriteError(&limited, &file_writer),
            error.TlsInitializationFailed => {
                const fallback_status = try https_fallback.fetch(
                    client.allocator,
                    client.io,
                    url,
                    &limited.interface,
                    null,
                );
                break :blk @enumFromInt(fallback_status);
            },
            else => |other| return other,
        };
        break :blk result.status;
    };
    limited.interface.flush() catch return diagnosedWriteError(&limited, &file_writer);
    file_writer.flush() catch return file_writer.err orelse error.FileWriteFailed;

    if (status.class() != .success) return error.UnsuccessfulHttpStatus;
    const written = maximum - limited.remaining;
    if (written != maximum) return error.DownloadSizeMismatch;
    if (!digestMatches(hashed.hasher.finalResult(), expected.sha1)) return error.DownloadHashMismatch;
    if (sync_file) try hard_link.syncFile(client.io, atomic_file.file);
    try atomic_file.replace(client.io);
}

fn diagnosedWriteError(limited: *const LimitedWriter, file_writer: *const std.Io.File.Writer) anyerror {
    if (limited.failure) |failure| switch (failure) {
        .too_long => return error.ResponseTooLarge,
        .downstream => {},
    };
    return file_writer.err orelse error.FileWriteFailed;
}

fn digestMatches(digest: [Sha1.digest_length]u8, expected: []const u8) bool {
    if (expected.len != Sha1.digest_length * 2) return false;
    const actual = std.fmt.bytesToHex(digest, .lower);
    return std.ascii.eqlIgnoreCase(&actual, expected);
}

test "valid cached files are verified without network access" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try temporary.dir.createDir(io, "instance", .default_dir);
    try temporary.dir.createDir(io, "store", .default_dir);
    const instance = try temporary.dir.openDir(io, "instance", .{});
    defer instance.close(io);
    const store = try temporary.dir.openDir(io, "store", .{});
    defer store.close(io);
    try instance.createDirPath(io, "assets/objects/a9");
    const file = try instance.createFile(io, "assets/objects/a9/test", .{});
    try std.Io.File.writeStreamingAll(file, io, "abc");
    file.close(io);

    const files = [_]install_plan.File{.{
        .path = "assets/objects/a9/test",
        .url = "https://example.invalid/test",
        .fallback_url = null,
        .sha1 = "a9993e364706816aba3e25717850c26c9cd0d89d",
        .size = 3,
        .usage = .{ .asset_object = true },
    }};
    const plan: install_plan.Data = .{
        .version_id = "test",
        .source = .official,
        .target = .{ .os = .linux, .arch = .x86_64 },
        .files = &files,
        .classpath = &.{},
        .native_extractions = &.{},
        .asset_projections = &.{},
        .total_download_size = 3,
    };

    const summary = try execute(
        std.testing.allocator,
        io,
        null,
        instance,
        store,
        plan,
        .{ .workers = 2 },
        null,
    );
    try std.testing.expectEqual(@as(usize, 1), summary.files_cached);
    try std.testing.expectEqual(@as(usize, 0), summary.files_downloaded);
    try std.testing.expectEqual(@as(usize, 1), summary.files_hard_linked);
    try std.testing.expectEqual(@as(u64, 3), summary.bytes_verified);
    var object_path_buffer: [64]u8 = undefined;
    const object_path = try artifact_store.objectPath(files[0].sha1, &object_path_buffer);
    const object_stat = try store.statFile(io, object_path, .{});
    const destination_stat = try instance.statFile(io, files[0].path, .{});
    try std.testing.expectEqual(object_stat.inode, destination_stat.inode);
}

test "digest comparison rejects changed content" {
    var digest: [Sha1.digest_length]u8 = undefined;
    Sha1.hash("abc", &digest, .{});
    try std.testing.expect(digestMatches(digest, "a9993e364706816aba3e25717850c26c9cd0d89d"));
    try std.testing.expect(!digestMatches(digest, "0000000000000000000000000000000000000000"));
}

const TestHttpServer = struct {
    io: std.Io,
    server: *std.Io.net.Server,
    failed: std.atomic.Value(bool) = .init(false),
    body: []const u8 = "abc",

    fn serveOnce(context: *TestHttpServer) void {
        var stream = context.server.accept(context.io) catch {
            context.failed.store(true, .release);
            return;
        };
        defer stream.close(context.io);

        var read_buffer: [4096]u8 = undefined;
        var write_buffer: [4096]u8 = undefined;
        var reader = stream.reader(context.io, &read_buffer);
        var writer = stream.writer(context.io, &write_buffer);
        var server: std.http.Server = .init(&reader.interface, &writer.interface);
        var request = server.receiveHead() catch {
            context.failed.store(true, .release);
            return;
        };
        request.respond(context.body, .{ .keep_alive = false }) catch {
            context.failed.store(true, .release);
        };
    }
};

test "streams a download through hash verification and atomic publication" {
    const io = std.testing.io;
    var address = try std.Io.net.IpAddress.parse("127.0.0.1", 0);
    var server = try address.listen(io, .{ .reuse_address = true });
    defer server.deinit(io);

    var server_context: TestHttpServer = .{ .io = io, .server = &server };
    const server_thread = try std.Thread.spawn(.{}, TestHttpServer.serveOnce, .{&server_context});

    const url = try std.fmt.allocPrint(
        std.testing.allocator,
        "http://127.0.0.1:{d}/file",
        .{server.socket.address.getPort()},
    );
    defer std.testing.allocator.free(url);

    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try temporary.dir.createDir(io, "instance", .default_dir);
    try temporary.dir.createDir(io, "store", .default_dir);
    const instance = try temporary.dir.openDir(io, "instance", .{});
    defer instance.close(io);
    const store = try temporary.dir.openDir(io, "store", .{});
    defer store.close(io);
    const files = [_]install_plan.File{.{
        .path = "libraries/example/test.jar",
        .url = url,
        .fallback_url = null,
        .sha1 = "a9993e364706816aba3e25717850c26c9cd0d89d",
        .size = 3,
        .usage = .{ .library = true },
    }};
    const plan: install_plan.Data = .{
        .version_id = "test",
        .source = .official,
        .target = .{ .os = .linux, .arch = .x86_64 },
        .files = &files,
        .classpath = &.{},
        .native_extractions = &.{},
        .asset_projections = &.{},
        .total_download_size = 3,
    };

    const summary = try execute(
        std.testing.allocator,
        io,
        null,
        instance,
        store,
        plan,
        .{ .workers = 1, .retries = 0 },
        null,
    );
    server_thread.join();

    try std.testing.expect(!server_context.failed.load(.acquire));
    try std.testing.expectEqual(@as(usize, 1), summary.files_downloaded);
    try std.testing.expectEqual(@as(usize, 1), summary.files_hard_linked);
    try std.testing.expect(try artifact_store.verify(instance, io, files[0].path, files[0].sha1, files[0].size));

    try temporary.dir.createDir(io, "second", .default_dir);
    const second = try temporary.dir.openDir(io, "second", .{});
    defer second.close(io);
    const reused = try execute(
        std.testing.allocator,
        io,
        null,
        second,
        store,
        plan,
        .{ .workers = 1, .retries = 0 },
        null,
    );
    try std.testing.expectEqual(@as(usize, 0), reused.files_downloaded);
    try std.testing.expectEqual(@as(usize, 1), reused.files_cached);
    try std.testing.expectEqual(@as(usize, 1), reused.files_hard_linked);
    var object_path_buffer: [64]u8 = undefined;
    const object_path = try artifact_store.objectPath(files[0].sha1, &object_path_buffer);
    const object_stat = try store.statFile(io, object_path, .{});
    const first_stat = try instance.statFile(io, files[0].path, .{});
    const second_stat = try second.statFile(io, files[0].path, .{});
    try std.testing.expectEqual(object_stat.inode, first_stat.inode);
    try std.testing.expectEqual(object_stat.inode, second_stat.inode);
    try std.testing.expect(object_stat.nlink >= 3);
}

test "emits byte-level progress before a large file completes" {
    const io = std.testing.io;
    const allocator = std.testing.allocator;
    const body_size: usize = @intCast(progress_byte_interval * 2 + 1234);
    const body = try allocator.alloc(u8, body_size);
    defer allocator.free(body);
    @memset(body, 'x');
    var digest: [Sha1.digest_length]u8 = undefined;
    Sha1.hash(body, &digest, .{});
    const sha1 = std.fmt.bytesToHex(digest, .lower);

    var address = try std.Io.net.IpAddress.parse("127.0.0.1", 0);
    var server = try address.listen(io, .{ .reuse_address = true });
    defer server.deinit(io);
    var server_context: TestHttpServer = .{
        .io = io,
        .server = &server,
        .body = body,
    };
    const server_thread = try std.Thread.spawn(.{}, TestHttpServer.serveOnce, .{&server_context});
    const url = try std.fmt.allocPrint(
        allocator,
        "http://127.0.0.1:{d}/file",
        .{server.socket.address.getPort()},
    );
    defer allocator.free(url);

    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try temporary.dir.createDir(io, "instance", .default_dir);
    try temporary.dir.createDir(io, "store", .default_dir);
    const instance = try temporary.dir.openDir(io, "instance", .{});
    defer instance.close(io);
    const store = try temporary.dir.openDir(io, "store", .{});
    defer store.close(io);
    const files = [_]install_plan.File{.{
        .path = "libraries/example/large.jar",
        .url = url,
        .fallback_url = null,
        .sha1 = &sha1,
        .size = body.len,
        .usage = .{ .library = true },
    }};
    const plan: install_plan.Data = .{
        .version_id = "test",
        .source = .official,
        .target = .{ .os = .linux, .arch = .x86_64 },
        .files = &files,
        .classpath = &.{},
        .native_extractions = &.{},
        .asset_projections = &.{},
        .total_download_size = body.len,
    };
    const Collector = struct {
        previous: u64 = 0,
        monotonic: bool = true,
        saw_streaming: bool = false,
        saw_complete: bool = false,

        fn callback(raw_context: *anyopaque, event: Event) void {
            const collector: *@This() = @ptrCast(@alignCast(raw_context));
            switch (event) {
                .progress => |progress| {
                    if (progress.bytes_processed < collector.previous) collector.monotonic = false;
                    collector.previous = progress.bytes_processed;
                    if (progress.files_completed == 0 and
                        progress.bytes_processed >= progress_byte_interval and
                        progress.bytes_processed < progress.bytes_total)
                    {
                        collector.saw_streaming = true;
                    }
                    if (progress.files_completed == 1 and
                        progress.bytes_processed == progress.bytes_total)
                    {
                        collector.saw_complete = true;
                    }
                },
                else => {},
            }
        }
    };
    var collector: Collector = .{};
    const summary = try execute(
        allocator,
        io,
        null,
        instance,
        store,
        plan,
        .{ .workers = 1, .retries = 0 },
        .{ .context = &collector, .callback = Collector.callback },
    );
    server_thread.join();

    try std.testing.expect(!server_context.failed.load(.acquire));
    try std.testing.expectEqual(@as(usize, 1), summary.files_downloaded);
    try std.testing.expect(collector.monotonic);
    try std.testing.expect(collector.saw_streaming);
    try std.testing.expect(collector.saw_complete);
}

test "byte reporter and retry progress are thresholded and monotonic" {
    const Collector = struct {
        reports: [4]u64 = undefined,
        len: usize = 0,

        fn callback(raw_context: *anyopaque, received: u64) void {
            const collector: *@This() = @ptrCast(@alignCast(raw_context));
            collector.reports[collector.len] = received;
            collector.len += 1;
        }
    };

    var collector: Collector = .{};
    var reporter: ByteProgressReporter = .{
        .context = &collector,
        .callback = Collector.callback,
    };
    reporter.add(@intCast(progress_byte_interval - 1));
    try std.testing.expectEqual(@as(usize, 0), collector.len);
    reporter.add(1);
    reporter.add(@intCast(progress_byte_interval / 2));
    try std.testing.expectEqual(@as(usize, 1), collector.len);
    reporter.add(@intCast(progress_byte_interval / 2));
    try std.testing.expectEqual(@as(usize, 2), collector.len);
    try std.testing.expectEqual(progress_byte_interval, collector.reports[0]);
    try std.testing.expectEqual(progress_byte_interval * 2, collector.reports[1]);

    var current: u64 = 0;
    var aggregate: u64 = 0;
    try std.testing.expect(advanceFileProgress(&current, &aggregate, 700_000));
    try std.testing.expect(!advanceFileProgress(&current, &aggregate, 300_000));
    try std.testing.expectEqual(@as(u64, 700_000), aggregate);
    try std.testing.expect(advanceFileProgress(&current, &aggregate, 900_000));
    try std.testing.expectEqual(@as(u64, 900_000), aggregate);
}
