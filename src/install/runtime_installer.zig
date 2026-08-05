const std = @import("std");
const builtin = @import("builtin");
const installer = @import("installer.zig");
const java_runtime = @import("java_runtime.zig");
const store = @import("java_runtime_store.zig");
const https_fallback = @import("https_fallback.zig");

const Sha1 = std.crypto.hash.Sha1;
const Sha256 = std.crypto.hash.sha2.Sha256;

pub const catalog_url = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
pub const azul_api_base = "https://api.azul.com/metadata/v1/zulu/packages";
pub const catalog_ttl_seconds: u64 = 600;
const max_catalog_bytes: usize = 4 * 1024 * 1024;
const max_manifest_bytes: usize = 32 * 1024 * 1024;
const max_package_bytes: usize = 64 * 1024;
const max_archive_bytes: u64 = 512 * 1024 * 1024;

pub const known_components = [_]struct { name: []const u8, major: u16 }{
    .{ .name = "java-runtime-epsilon", .major = 21 },
    .{ .name = "java-runtime-delta", .major = 21 },
    .{ .name = "java-runtime-gamma", .major = 17 },
    .{ .name = "java-runtime-beta", .major = 17 },
    .{ .name = "java-runtime-alpha", .major = 16 },
    .{ .name = "jre-legacy", .major = 8 },
};

pub fn knownMajor(component: []const u8) ?u16 {
    if (std.mem.eql(u8, component, "java-runtime-gamma-snapshot")) return 17;
    for (known_components) |entry| {
        if (std.mem.eql(u8, entry.name, component)) return entry.major;
    }
    return null;
}

pub const FetchPolicy = union(enum) {
    cached: u64,
    immutable,
    fresh_only,
};

pub const Fetcher = struct {
    context: *anyopaque,
    fetch_fn: *const fn (
        context: *anyopaque,
        url: []const u8,
        max_bytes: usize,
        policy: FetchPolicy,
    ) anyerror![]u8,

    pub fn fetch(fetcher: Fetcher, url: []const u8, max_bytes: usize, policy: FetchPolicy) ![]u8 {
        return fetcher.fetch_fn(fetcher.context, url, max_bytes, policy);
    }
};

pub const Stage = enum {
    resolving,
    downloading,
    extracting,
    verifying,
};

pub const FileEvent = struct {
    path: []const u8,
    size: u64,
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
    bytes_processed: u64,
    bytes_total: u64,
    bytes_transferred: u64,
};

pub const Event = union(enum) {
    stage: Stage,
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

pub const Options = struct {
    workers: u8 = 8,
    retries: u8 = 2,
    sync_files: bool = true,
};

pub const Outcome = struct {
    arena: *std.heap.ArenaAllocator,
    receipt: store.Receipt,
    runtime_name: []const u8,
    already_installed: bool,

    pub fn deinit(outcome: *Outcome) void {
        const allocator = outcome.arena.child_allocator;
        outcome.arena.deinit();
        allocator.destroy(outcome.arena);
        outcome.* = undefined;
    }
};

pub const MojangPlan = struct {
    component: []const u8,
    manifest_sha1: []const u8,
    manifest_size: u64,
    manifest_url: []const u8,
};

const FileEntry = struct {
    path: []const u8,
    url: []const u8,
    sha1: []const u8,
    size: u64,
    executable: bool,
};

const LinkEntry = struct {
    path: []const u8,
    target: []const u8,
};

const ManifestEntries = struct {
    files: []FileEntry,
    links: []LinkEntry,
    directories: [][]const u8,
    bytes_total: u64,
};

fn createArena(allocator: std.mem.Allocator) !*std.heap.ArenaAllocator {
    const arena = try allocator.create(std.heap.ArenaAllocator);
    arena.* = std.heap.ArenaAllocator.init(allocator);
    return arena;
}

fn existingOutcome(
    allocator: std.mem.Allocator,
    io: std.Io,
    runtimes_dir: std.Io.Dir,
    runtime_name: []const u8,
) !?Outcome {
    var runtime_dir = runtimes_dir.openDir(io, runtime_name, .{}) catch |err| switch (err) {
        error.FileNotFound, error.NotDir => return null,
        else => |other| return other,
    };
    defer runtime_dir.close(io);

    const arena = try createArena(allocator);
    errdefer {
        arena.deinit();
        allocator.destroy(arena);
    }
    const result_allocator = arena.allocator();
    const bytes = runtime_dir.readFileAlloc(io, store.receipt_name, result_allocator, .limited(store.max_receipt_bytes + 1)) catch |err| switch (err) {
        error.FileNotFound => {
            arena.deinit();
            allocator.destroy(arena);
            return null;
        },
        else => |other| return other,
    };
    if (bytes.len > store.max_receipt_bytes) {
        arena.deinit();
        allocator.destroy(arena);
        return null;
    }
    const receipt = std.json.parseFromSliceLeaky(store.Receipt, result_allocator, bytes, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
    }) catch {
        arena.deinit();
        allocator.destroy(arena);
        return null;
    };
    store.validateReceipt(receipt) catch {
        arena.deinit();
        allocator.destroy(arena);
        return null;
    };
    return .{
        .arena = arena,
        .receipt = receipt,
        .runtime_name = try result_allocator.dupe(u8, runtime_name),
        .already_installed = true,
    };
}

fn parseCatalogComponent(
    catalog: std.json.Value,
    platform: []const u8,
    component: []const u8,
) ?MojangPlan {
    const root = switch (catalog) {
        .object => |object| object,
        else => return null,
    };
    const platform_value = root.get(platform) orelse return null;
    const platform_object = switch (platform_value) {
        .object => |object| object,
        else => return null,
    };
    const component_value = platform_object.get(component) orelse return null;
    const entries = switch (component_value) {
        .array => |array| array,
        else => return null,
    };
    if (entries.items.len == 0) return null;
    const entry = switch (entries.items[0]) {
        .object => |object| object,
        else => return null,
    };
    const manifest_value = entry.get("manifest") orelse return null;
    const manifest = switch (manifest_value) {
        .object => |object| object,
        else => return null,
    };
    const sha1 = switch (manifest.get("sha1") orelse return null) {
        .string => |value| value,
        else => return null,
    };
    const size = switch (manifest.get("size") orelse return null) {
        .integer => |value| value,
        else => return null,
    };
    const url = switch (manifest.get("url") orelse return null) {
        .string => |value| value,
        else => return null,
    };
    if (sha1.len != 40 or size <= 0 or !std.mem.startsWith(u8, url, "https://")) return null;
    return .{
        .component = component,
        .manifest_sha1 = sha1,
        .manifest_size = @intCast(size),
        .manifest_url = url,
    };
}

fn parseManifestFiles(allocator: std.mem.Allocator, manifest: std.json.Value) !ManifestEntries {
    const root = switch (manifest) {
        .object => |object| object,
        else => return error.InvalidRuntimeManifest,
    };
    const files_value = root.get("files") orelse return error.InvalidRuntimeManifest;
    const files_object = switch (files_value) {
        .object => |object| object,
        else => return error.InvalidRuntimeManifest,
    };

    var files: std.ArrayList(FileEntry) = .empty;
    errdefer files.deinit(allocator);
    var links: std.ArrayList(LinkEntry) = .empty;
    errdefer links.deinit(allocator);
    var directories: std.ArrayList([]const u8) = .empty;
    errdefer directories.deinit(allocator);
    var bytes_total: u64 = 0;

    var iterator = files_object.iterator();
    while (iterator.next()) |field| {
        const path = field.key_ptr.*;
        store.validateRelativePath(path) catch return error.InvalidRuntimeManifest;
        const entry = switch (field.value_ptr.*) {
            .object => |object| object,
            else => return error.InvalidRuntimeManifest,
        };
        const entry_type = switch (entry.get("type") orelse return error.InvalidRuntimeManifest) {
            .string => |value| value,
            else => return error.InvalidRuntimeManifest,
        };
        if (std.mem.eql(u8, entry_type, "directory")) {
            try directories.append(allocator, path);
            continue;
        }
        if (std.mem.eql(u8, entry_type, "link")) {
            const target = switch (entry.get("target") orelse return error.InvalidRuntimeManifest) {
                .string => |value| value,
                else => return error.InvalidRuntimeManifest,
            };
            validateLinkTarget(path, target) catch return error.InvalidRuntimeManifest;
            try links.append(allocator, .{ .path = path, .target = target });
            continue;
        }
        if (!std.mem.eql(u8, entry_type, "file")) return error.InvalidRuntimeManifest;

        const downloads = switch (entry.get("downloads") orelse return error.InvalidRuntimeManifest) {
            .object => |object| object,
            else => return error.InvalidRuntimeManifest,
        };
        const raw = switch (downloads.get("raw") orelse return error.InvalidRuntimeManifest) {
            .object => |object| object,
            else => return error.InvalidRuntimeManifest,
        };
        const sha1 = switch (raw.get("sha1") orelse return error.InvalidRuntimeManifest) {
            .string => |value| value,
            else => return error.InvalidRuntimeManifest,
        };
        const size = switch (raw.get("size") orelse return error.InvalidRuntimeManifest) {
            .integer => |value| value,
            else => return error.InvalidRuntimeManifest,
        };
        const url = switch (raw.get("url") orelse return error.InvalidRuntimeManifest) {
            .string => |value| value,
            else => return error.InvalidRuntimeManifest,
        };
        if (sha1.len != 40 or size < 0 or !std.mem.startsWith(u8, url, "https://")) {
            return error.InvalidRuntimeManifest;
        }
        var executable = false;
        if (entry.get("executable")) |executable_value| {
            executable = switch (executable_value) {
                .bool => |value| value,
                else => return error.InvalidRuntimeManifest,
            };
        }
        bytes_total = std.math.add(u64, bytes_total, @intCast(size)) catch return error.InvalidRuntimeManifest;
        try files.append(allocator, .{
            .path = path,
            .url = url,
            .sha1 = sha1,
            .size = @intCast(size),
            .executable = executable,
        });
    }
    return .{
        .files = try files.toOwnedSlice(allocator),
        .links = try links.toOwnedSlice(allocator),
        .directories = try directories.toOwnedSlice(allocator),
        .bytes_total = bytes_total,
    };
}

pub fn validateLinkTarget(link_path: []const u8, target: []const u8) !void {
    if (target.len == 0 or target.len > 1024) return error.InvalidLinkTarget;
    if (std.fs.path.isAbsolute(target)) return error.InvalidLinkTarget;
    if (std.mem.indexOfAny(u8, target, "\\:") != null) return error.InvalidLinkTarget;
    if (std.mem.indexOfScalar(u8, target, 0) != null) return error.InvalidLinkTarget;

    var depth: i64 = 0;
    if (std.fs.path.dirname(link_path)) |parent| {
        var parts = std.mem.splitScalar(u8, parent, '/');
        while (parts.next()) |part| {
            if (part.len != 0) depth += 1;
        }
    }
    var parts = std.mem.splitScalar(u8, target, '/');
    while (parts.next()) |part| {
        if (part.len == 0 or std.mem.eql(u8, part, ".")) continue;
        if (std.mem.eql(u8, part, "..")) {
            depth -= 1;
            if (depth < 0) return error.InvalidLinkTarget;
        } else {
            depth += 1;
        }
    }
}

fn digestHexMatches(comptime Hasher: type, digest: [Hasher.digest_length]u8, expected_hex: []const u8) bool {
    if (expected_hex.len != Hasher.digest_length * 2) return false;
    const actual = std.fmt.bytesToHex(digest, .lower);
    return std.mem.eql(u8, &actual, expected_hex);
}

fn downloadVerified(
    comptime Hasher: type,
    client: *std.http.Client,
    root: std.Io.Dir,
    sub_path: []const u8,
    url: []const u8,
    expected_size: u64,
    expected_hex: []const u8,
    executable: bool,
    sync_file: bool,
    progress: *installer.ByteProgressReporter,
) !void {
    var atomic_file = try root.createFileAtomic(client.io, sub_path, .{
        .make_path = true,
        .replace = true,
    });
    defer atomic_file.deinit(client.io);

    var file_buffer: [64 * 1024]u8 = undefined;
    var file_writer: std.Io.File.Writer = .init(atomic_file.file, client.io, &file_buffer);
    var hashed = std.Io.Writer.Hashed(Hasher).initHasher(
        &file_writer.interface,
        Hasher.init(.{}),
        &.{},
    );
    var limit_buffer: [64 * 1024]u8 = undefined;
    const maximum = std.math.cast(usize, expected_size) orelse return error.FileTooLarge;
    var limited = installer.LimitedWriter.init(&hashed.writer, &limit_buffer, maximum, progress);

    const status: u16 = blk: {
        const result = client.fetch(.{
            .location = .{ .url = url },
            .response_writer = &limited.interface,
            .redirect_behavior = std.http.Client.Request.RedirectBehavior.init(5),
        }) catch |err| switch (err) {
            error.WriteFailed => {
                if (limited.failure) |failure| switch (failure) {
                    .too_long => return error.ResponseTooLarge,
                    .downstream => return error.FileWriteFailed,
                };
                return error.FileWriteFailed;
            },
            error.TlsInitializationFailed => {
                const fallback_status = try https_fallback.fetch(
                    client.allocator,
                    client.io,
                    url,
                    &limited.interface,
                    null,
                );
                break :blk fallback_status;
            },
            else => |other| return other,
        };
        break :blk @intFromEnum(result.status);
    };
    limited.interface.flush() catch {
        if (limited.failure) |failure| switch (failure) {
            .too_long => return error.ResponseTooLarge,
            .downstream => return error.FileWriteFailed,
        };
        return error.FileWriteFailed;
    };
    file_writer.flush() catch return file_writer.err orelse error.FileWriteFailed;

    if (status < 200 or status >= 300) return error.UnsuccessfulHttpStatus;
    const written = maximum - limited.remaining;
    if (written != maximum) return error.DownloadSizeMismatch;
    if (!digestHexMatches(Hasher, hashed.hasher.finalResult(), expected_hex)) return error.DownloadHashMismatch;
    if (executable) try atomic_file.file.setPermissions(client.io, .executable_file);
    if (sync_file) try atomic_file.file.sync(client.io);
    try atomic_file.replace(client.io);
}

const Shared = struct {
    mutex: std.Io.Mutex = .init,
    progress_mutex: std.Io.Mutex = .init,
    next_index: usize = 0,
    failure: ?anyerror = null,
    progress: Progress,
    file_progress: []u64,
    last_stream_report: ?std.Io.Timestamp = null,
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: ?*const std.process.Environ.Map,
    staging: std.Io.Dir,
    files: []const FileEntry,
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
                @as(i96, installer.progress_time_interval_ms) * std.time.ns_per_ms) return;
        }
        shared.last_stream_report = now;
        Reporter.emit(shared.reporter, .{ .progress = progress });
    }

    fn complete(shared: *Shared, index: usize, file: FileEntry) void {
        shared.progress_mutex.lockUncancelable(shared.io);
        defer shared.progress_mutex.unlock(shared.io);
        shared.mutex.lockUncancelable(shared.io);
        _ = advanceFileProgress(
            &shared.file_progress[index],
            &shared.progress.bytes_processed,
            file.size,
        );
        shared.progress.files_completed += 1;
        shared.progress.bytes_transferred += file.size;
        const progress = shared.progress;
        shared.mutex.unlock(shared.io);

        shared.last_stream_report = std.Io.Clock.awake.now(shared.io);
        Reporter.emit(shared.reporter, .{ .file = .{ .path = file.path, .size = file.size } });
        Reporter.emit(shared.reporter, .{ .progress = progress });
    }
};

fn advanceFileProgress(current: *u64, aggregate: *u64, observed: u64) bool {
    if (observed <= current.*) return false;
    aggregate.* += observed - current.*;
    current.* = observed;
    return true;
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
        const max_attempts: u8 = shared.options.retries + 1;
        var attempt: u8 = 0;
        var succeeded = false;
        while (attempt < max_attempts) : (attempt += 1) {
            var progress_context: DownloadProgressContext = .{
                .shared = shared,
                .index = index,
            };
            var byte_reporter: installer.ByteProgressReporter = .{
                .context = &progress_context,
                .callback = DownloadProgressContext.callback,
            };
            downloadVerified(
                Sha1,
                &client,
                shared.staging,
                file.path,
                file.url,
                file.size,
                file.sha1,
                file.executable,
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
                break;
            };
            succeeded = true;
            break;
        }
        if (!succeeded) {
            if (shared.failure == null) shared.fail(error.DownloadFailed);
            return;
        }
        shared.complete(index, file);
    }
}

fn runDownloadPool(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: ?*const std.process.Environ.Map,
    staging: std.Io.Dir,
    files: []const FileEntry,
    bytes_total: u64,
    options: Options,
    reporter: ?Reporter,
) !void {
    if (files.len == 0) return;
    const file_progress = try allocator.alloc(u64, files.len);
    defer allocator.free(file_progress);
    @memset(file_progress, 0);

    var shared: Shared = .{
        .progress = .{
            .files_completed = 0,
            .files_total = files.len,
            .bytes_processed = 0,
            .bytes_total = bytes_total,
            .bytes_transferred = 0,
        },
        .file_progress = file_progress,
        .allocator = allocator,
        .io = io,
        .environ_map = environ_map,
        .staging = staging,
        .files = files,
        .options = options,
        .reporter = reporter,
    };

    const worker_count = @min(@as(usize, options.workers), files.len);
    if (worker_count == 1) {
        worker(&shared);
        if (shared.failure) |failure| return failure;
        return;
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
}

fn finalizeInstall(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    runtimes_dir: std.Io.Dir,
    staging_dir: std.Io.Dir,
    staging_name: []const u8,
    runtime_name: []const u8,
    expected_major: ?u16,
    receipt_template: store.Receipt,
    reporter: ?Reporter,
) !store.Receipt {
    Reporter.emit(reporter, .{ .stage = .verifying });

    var discovery = try store.findJavaHome(allocator, io, staging_dir);
    defer discovery.deinit(allocator);

    var staging_path_buffer: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const staging_path_len = try staging_dir.realPath(io, &staging_path_buffer);
    const executable_absolute = try std.fmt.allocPrint(
        allocator,
        "{s}/{s}",
        .{ staging_path_buffer[0..staging_path_len], discovery.executable },
    );
    defer allocator.free(executable_absolute);

    const probe_arena = try createArena(allocator);
    defer {
        probe_arena.deinit();
        allocator.destroy(probe_arena);
    }
    const probed = java_runtime.probeExecutable(
        probe_arena.allocator(),
        allocator,
        io,
        environ_map,
        executable_absolute,
        .managed,
    ) catch return error.ManagedRuntimeProbeFailed;

    if (expected_major) |major| {
        if (probed.major_version != major) return error.ManagedRuntimeMajorMismatch;
    }

    var receipt = receipt_template;
    receipt.major_version = probed.major_version;
    receipt.java_home = discovery.java_home;
    receipt.executable = discovery.executable;
    receipt.version = probed.version;
    receipt.vendor = probed.vendor;

    try store.writeReceipt(io, staging_dir, receipt);
    runtimes_dir.deleteTree(io, runtime_name) catch {};
    runtimes_dir.rename(staging_name, runtimes_dir, runtime_name, io) catch |err| switch (err) {
        else => |other| return other,
    };
    receipt.java_home = try allocator.dupe(u8, discovery.java_home);
    receipt.executable = try allocator.dupe(u8, discovery.executable);
    if (receipt.version) |value| receipt.version = try allocator.dupe(u8, value);
    if (receipt.vendor) |value| receipt.vendor = try allocator.dupe(u8, value);
    return receipt;
}

pub fn installMojang(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    fetcher: Fetcher,
    runtimes_dir: std.Io.Dir,
    component: []const u8,
    platform: []const u8,
    options: Options,
    reporter: ?Reporter,
) !Outcome {
    if (options.workers == 0 or options.workers > 32) return error.InvalidWorkerCount;
    if (options.retries > 5) return error.InvalidRetryCount;
    try store.validateId(component);
    if (store.parsePlatform(platform) == null) return error.InvalidRuntimePlatform;

    const arena = try createArena(allocator);
    errdefer {
        arena.deinit();
        allocator.destroy(arena);
    }
    const result_allocator = arena.allocator();

    var name_buffer: [store.max_runtime_name_bytes]u8 = undefined;
    const runtime_name = try result_allocator.dupe(
        u8,
        try store.runtimeDirName(&name_buffer, .mojang, component, platform),
    );
    if (try existingOutcome(allocator, io, runtimes_dir, runtime_name)) |existing| {
        arena.deinit();
        allocator.destroy(arena);
        return existing;
    }

    Reporter.emit(reporter, .{ .stage = .resolving });

    const catalog_body = try fetcher.fetch(catalog_url, max_catalog_bytes, .{ .cached = catalog_ttl_seconds });
    defer allocator.free(catalog_body);
    const catalog_parsed = try std.json.parseFromSlice(std.json.Value, result_allocator, catalog_body, .{
        .max_value_len = max_catalog_bytes,
    });
    const plan = parseCatalogComponent(catalog_parsed.value, platform, component) orelse
        return error.RuntimeComponentUnavailable;

    var manifest_body = try fetcher.fetch(plan.manifest_url, max_manifest_bytes, .immutable);
    defer allocator.free(manifest_body);
    {
        var hasher = Sha1.init(.{});
        hasher.update(manifest_body);
        if (!digestHexMatches(Sha1, hasher.finalResult(), plan.manifest_sha1) or
            manifest_body.len != plan.manifest_size)
        {
            allocator.free(manifest_body);
            manifest_body = try fetcher.fetch(plan.manifest_url, max_manifest_bytes, .fresh_only);
            var retry_hasher = Sha1.init(.{});
            retry_hasher.update(manifest_body);
            if (!digestHexMatches(Sha1, retry_hasher.finalResult(), plan.manifest_sha1) or
                manifest_body.len != plan.manifest_size)
            {
                return error.RuntimeManifestMismatch;
            }
        }
    }
    const manifest_parsed = try std.json.parseFromSlice(std.json.Value, result_allocator, manifest_body, .{
        .max_value_len = max_manifest_bytes,
    });
    const entries = try parseManifestFiles(result_allocator, manifest_parsed.value);

    store.cleanupStaging(io, runtimes_dir);
    var staging_name_buffer: [store.max_runtime_name_bytes + 32]u8 = undefined;
    var random_bytes: [8]u8 = undefined;
    try io.randomSecure(&random_bytes);
    const staging_name = try result_allocator.dupe(
        u8,
        try store.stagingName(&staging_name_buffer, runtime_name, std.mem.readInt(u64, &random_bytes, .little)),
    );
    runtimes_dir.deleteTree(io, staging_name) catch {};
    var staging_dir = try runtimes_dir.createDirPathOpen(io, staging_name, .{});
    defer staging_dir.close(io);
    errdefer runtimes_dir.deleteTree(io, staging_name) catch {};

    Reporter.emit(reporter, .{ .stage = .downloading });
    try runDownloadPool(
        allocator,
        io,
        environ_map,
        staging_dir,
        entries.files,
        entries.bytes_total,
        options,
        reporter,
    );

    for (entries.directories) |directory| {
        try staging_dir.createDirPath(io, directory);
    }
    for (entries.links) |link| {
        staging_dir.symLink(io, link.target, link.path, .{}) catch |err| switch (err) {
            error.PathAlreadyExists => return error.RuntimeManifestConflict,
            else => |other| return other,
        };
    }

    const receipt = try finalizeInstall(
        result_allocator,
        io,
        environ_map,
        runtimes_dir,
        staging_dir,
        staging_name,
        runtime_name,
        knownMajor(component),
        .{
            .provider = .mojang,
            .id = try result_allocator.dupe(u8, component),
            .platform = try result_allocator.dupe(u8, platform),
            .major_version = 0,
            .java_home = "",
            .executable = "",
            .component = try result_allocator.dupe(u8, component),
            .source_url = try result_allocator.dupe(u8, plan.manifest_url),
            .files_total = @intCast(entries.files.len + entries.links.len),
            .bytes_total = entries.bytes_total,
            .installed_at_ms = std.Io.Timestamp.now(io, .real).toMilliseconds(),
        },
        reporter,
    );

    return .{
        .arena = arena,
        .receipt = receipt,
        .runtime_name = runtime_name,
        .already_installed = false,
    };
}

pub fn installZulu(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    fetcher: Fetcher,
    runtimes_dir: std.Io.Dir,
    major: u16,
    platform: []const u8,
    options: Options,
    reporter: ?Reporter,
) !Outcome {
    if (major == 0) return error.InvalidJavaRequirement;
    if (options.retries > 5) return error.InvalidRetryCount;
    const target = store.parsePlatform(platform) orelse return error.InvalidRuntimePlatform;

    const arena = try createArena(allocator);
    errdefer {
        arena.deinit();
        allocator.destroy(arena);
    }
    const result_allocator = arena.allocator();

    const id = try std.fmt.allocPrint(result_allocator, "{d}", .{major});
    var name_buffer: [store.max_runtime_name_bytes]u8 = undefined;
    const runtime_name = try result_allocator.dupe(
        u8,
        try store.runtimeDirName(&name_buffer, .zulu, id, platform),
    );
    if (try existingOutcome(allocator, io, runtimes_dir, runtime_name)) |existing| {
        arena.deinit();
        allocator.destroy(arena);
        return existing;
    }

    Reporter.emit(reporter, .{ .stage = .resolving });

    const os_name: []const u8 = switch (target.os) {
        .windows => "windows",
        .osx => "macos",
        .linux => "linux",
    };
    const arch_name: []const u8 = switch (target.arch) {
        .x86, .x86_64 => "x86",
        .arm, .arm64 => "arm",
    };
    const bitness: []const u8 = switch (target.arch) {
        .x86, .arm => "32",
        .x86_64, .arm64 => "64",
    };
    const query_url = try std.fmt.allocPrint(
        result_allocator,
        "{s}/?java_version={d}&os={s}&arch={s}&hw_bitness={s}&archive_type=tar.gz&java_package_type=jre&javafx_bundled=false&release_status=ga&latest=true&page_size=1",
        .{ azul_api_base, major, os_name, arch_name, bitness },
    );
    const list_body = try fetcher.fetch(query_url, max_package_bytes, .{ .cached = catalog_ttl_seconds });
    defer allocator.free(list_body);

    const ListItem = struct {
        package_uuid: []const u8,
        name: []const u8,
    };
    const list_parsed = std.json.parseFromSlice([]ListItem, result_allocator, list_body, .{
        .ignore_unknown_fields = true,
        .max_value_len = max_package_bytes,
    }) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return error.RuntimePackageUnavailable,
    };
    if (list_parsed.value.len == 0) return error.RuntimePackageUnavailable;
    const package_uuid = list_parsed.value[0].package_uuid;

    const detail_url = try std.fmt.allocPrint(result_allocator, "{s}/{s}", .{ azul_api_base, package_uuid });
    const detail_body = try fetcher.fetch(detail_url, max_package_bytes, .immutable);
    defer allocator.free(detail_body);

    const Detail = struct {
        package_uuid: []const u8,
        name: []const u8,
        download_url: []const u8,
        sha256_hash: []const u8,
        size: u64,
    };
    const detail_parsed = std.json.parseFromSlice(Detail, result_allocator, detail_body, .{
        .ignore_unknown_fields = true,
        .max_value_len = max_package_bytes,
    }) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return error.RuntimePackageUnavailable,
    };
    const detail = detail_parsed.value;
    if (!std.mem.eql(u8, detail.package_uuid, package_uuid) or
        !std.mem.startsWith(u8, detail.download_url, "https://") or
        detail.sha256_hash.len != 64 or detail.size == 0 or detail.size > max_archive_bytes)
    {
        return error.RuntimePackageUnavailable;
    }

    store.cleanupStaging(io, runtimes_dir);
    var staging_name_buffer: [store.max_runtime_name_bytes + 32]u8 = undefined;
    var random_bytes: [8]u8 = undefined;
    try io.randomSecure(&random_bytes);
    const staging_name = try result_allocator.dupe(
        u8,
        try store.stagingName(&staging_name_buffer, runtime_name, std.mem.readInt(u64, &random_bytes, .little)),
    );
    runtimes_dir.deleteTree(io, staging_name) catch {};
    var staging_dir = try runtimes_dir.createDirPathOpen(io, staging_name, .{});
    defer staging_dir.close(io);
    errdefer runtimes_dir.deleteTree(io, staging_name) catch {};

    Reporter.emit(reporter, .{ .stage = .downloading });
    {
        var proxy_arena = std.heap.ArenaAllocator.init(allocator);
        defer proxy_arena.deinit();
        var client: std.http.Client = .{
            .allocator = allocator,
            .io = io,
        };
        defer client.deinit();
        client.initDefaultProxies(proxy_arena.allocator(), environ_map) catch return error.NetworkError;

        const archive_entry: FileEntry = .{
            .path = "archive.tar.gz",
            .url = detail.download_url,
            .sha1 = detail.sha256_hash,
            .size = detail.size,
            .executable = false,
        };
        var file_progress = [_]u64{0};
        var shared: Shared = .{
            .progress = .{
                .files_completed = 0,
                .files_total = 1,
                .bytes_processed = 0,
                .bytes_total = detail.size,
                .bytes_transferred = 0,
            },
            .file_progress = &file_progress,
            .allocator = allocator,
            .io = io,
            .environ_map = environ_map,
            .staging = staging_dir,
            .files = &.{},
            .options = options,
            .reporter = reporter,
        };
        const max_attempts: u8 = options.retries + 1;
        var attempt: u8 = 0;
        var succeeded = false;
        while (attempt < max_attempts) : (attempt += 1) {
            var progress_context: DownloadProgressContext = .{ .shared = &shared, .index = 0 };
            var byte_reporter: installer.ByteProgressReporter = .{
                .context = &progress_context,
                .callback = DownloadProgressContext.callback,
            };
            downloadVerified(
                Sha256,
                &client,
                staging_dir,
                archive_entry.path,
                archive_entry.url,
                archive_entry.size,
                archive_entry.sha1,
                false,
                options.sync_files,
                &byte_reporter,
            ) catch |err| {
                if (attempt + 1 < max_attempts) {
                    Reporter.emit(reporter, .{ .retry = .{
                        .path = archive_entry.path,
                        .attempt = attempt + 1,
                        .max_attempts = max_attempts,
                        .error_name = @errorName(err),
                    } });
                    std.Io.sleep(io, .fromMilliseconds(200 * (@as(i64, attempt) + 1)), .awake) catch {};
                    continue;
                }
                return err;
            };
            succeeded = true;
            break;
        }
        if (!succeeded) return error.DownloadFailed;
        _ = advanceFileProgress(&shared.file_progress[0], &shared.progress.bytes_processed, detail.size);
        shared.progress.files_completed = 1;
        shared.progress.bytes_transferred = detail.size;
        Reporter.emit(reporter, .{ .file = .{ .path = archive_entry.path, .size = detail.size } });
        Reporter.emit(reporter, .{ .progress = shared.progress });
    }

    Reporter.emit(reporter, .{ .stage = .extracting });
    {
        const archive_file = try staging_dir.openFile(io, "archive.tar.gz", .{});
        defer archive_file.close(io);
        var read_buffer: [64 * 1024]u8 = undefined;
        var file_reader: std.Io.File.Reader = .init(archive_file, io, &read_buffer);
        var decompress_buffer: [64 * 1024]u8 = undefined;
        var decompress = std.compress.flate.Decompress.init(&file_reader.interface, .gzip, &decompress_buffer);
        try std.tar.extract(io, staging_dir, &decompress.reader, .{
            .strip_components = 1,
            .mode_mode = .executable_bit_only,
        });
    }
    staging_dir.deleteFile(io, "archive.tar.gz") catch {};

    const receipt = try finalizeInstall(
        result_allocator,
        io,
        environ_map,
        runtimes_dir,
        staging_dir,
        staging_name,
        runtime_name,
        major,
        .{
            .provider = .zulu,
            .id = id,
            .platform = try result_allocator.dupe(u8, platform),
            .major_version = 0,
            .java_home = "",
            .executable = "",
            .package_uuid = try result_allocator.dupe(u8, detail.package_uuid),
            .package_name = try result_allocator.dupe(u8, detail.name),
            .source_url = try result_allocator.dupe(u8, detail.download_url),
            .files_total = 1,
            .bytes_total = detail.size,
            .installed_at_ms = std.Io.Timestamp.now(io, .real).toMilliseconds(),
        },
        reporter,
    );

    return .{
        .arena = arena,
        .receipt = receipt,
        .runtime_name = runtime_name,
        .already_installed = false,
    };
}

test "known component majors cover the documented mapping" {
    try std.testing.expectEqual(@as(u16, 8), knownMajor("jre-legacy").?);
    try std.testing.expectEqual(@as(u16, 16), knownMajor("java-runtime-alpha").?);
    try std.testing.expectEqual(@as(u16, 17), knownMajor("java-runtime-gamma").?);
    try std.testing.expectEqual(@as(u16, 21), knownMajor("java-runtime-delta").?);
    try std.testing.expect(knownMajor("java-runtime-unknown") == null);
}

test "catalog component parsing finds the manifest entry" {
    const allocator = std.testing.allocator;
    const body =
        \\{"mac-os-arm64":{"java-runtime-gamma":[{"availability":{"group":1,"progress":100},"manifest":{"sha1":"0123456789abcdef0123456789abcdef01234567","size":123,"url":"https://piston-meta.mojang.com/v1/packages/abc/manifest.json"},"version":{"name":"17.0.8+7","released":"2024-01-01T00:00:00Z"}}],"jre-legacy":[]},"linux":{}}
    ;
    var parsed = try std.json.parseFromSlice(std.json.Value, allocator, body, .{});
    defer parsed.deinit();
    const plan = parseCatalogComponent(parsed.value, "mac-os-arm64", "java-runtime-gamma").?;
    try std.testing.expectEqualStrings("0123456789abcdef0123456789abcdef01234567", plan.manifest_sha1);
    try std.testing.expectEqual(@as(u64, 123), plan.manifest_size);
    try std.testing.expect(parseCatalogComponent(parsed.value, "mac-os-arm64", "jre-legacy") == null);
    try std.testing.expect(parseCatalogComponent(parsed.value, "linux-arm64", "java-runtime-gamma") == null);
}

test "manifest file parsing validates entries and link targets" {
    const allocator = std.testing.allocator;
    const body =
        \\{"files":{
        \\"bin/java":{"downloads":{"raw":{"sha1":"0123456789abcdef0123456789abcdef01234567","size":10,"url":"https://piston-data.mojang.com/v1/objects/abc/java"}},"executable":true,"type":"file"},
        \\"legal":{"type":"directory"},
        \\"legal/java.base/LICENSE":{"target":"../java.base/LICENSE","type":"link"}
        \\}}
    ;
    var parsed = try std.json.parseFromSlice(std.json.Value, allocator, body, .{});
    defer parsed.deinit();
    const entries = try parseManifestFiles(allocator, parsed.value);
    defer allocator.free(entries.files);
    defer allocator.free(entries.links);
    defer allocator.free(entries.directories);
    try std.testing.expectEqual(@as(usize, 1), entries.files.len);
    try std.testing.expectEqual(@as(usize, 1), entries.links.len);
    try std.testing.expectEqual(@as(usize, 1), entries.directories.len);
    try std.testing.expect(entries.files[0].executable);
    try std.testing.expectEqual(@as(u64, 10), entries.bytes_total);
}

test "manifest parsing rejects traversal paths and unsafe links" {
    const allocator = std.testing.allocator;
    const traversal =
        \\{"files":{"../evil":{"downloads":{"raw":{"sha1":"0123456789abcdef0123456789abcdef01234567","size":1,"url":"https://example.invalid/x"}},"type":"file"}}}
    ;
    var parsed = try std.json.parseFromSlice(std.json.Value, allocator, traversal, .{});
    defer parsed.deinit();
    try std.testing.expectError(error.InvalidRuntimeManifest, parseManifestFiles(allocator, parsed.value));

    const unsafe_link =
        \\{"files":{"a/b/link":{"target":"../../../escape","type":"link"}}}
    ;
    var link_parsed = try std.json.parseFromSlice(std.json.Value, allocator, unsafe_link, .{});
    defer link_parsed.deinit();
    try std.testing.expectError(error.InvalidRuntimeManifest, parseManifestFiles(allocator, link_parsed.value));
}

test "link target validation resolves against the link parent" {
    try validateLinkTarget("legal/java.compiler/LICENSE", "../java.base/LICENSE");
    try validateLinkTarget("a/b/c/d", "../../x");
    try std.testing.expectError(error.InvalidLinkTarget, validateLinkTarget("a/b", "../../escape"));
    try std.testing.expectError(error.InvalidLinkTarget, validateLinkTarget("link", "/absolute"));
    try std.testing.expectError(error.InvalidLinkTarget, validateLinkTarget("link", "a\\b"));
}

test "digest hex comparison is exact and lowercase" {
    var digest: [Sha1.digest_length]u8 = undefined;
    Sha1.hash("payload", &digest, .{});
    var hasher = Sha1.init(.{});
    hasher.update("payload");
    try std.testing.expect(digestHexMatches(Sha1, hasher.finalResult(), "f07e5a815613c5abeddc4b682247a4c42d8a95df"));
    try std.testing.expect(!digestHexMatches(Sha1, hasher.finalResult(), "F07E5A815613C5ABEDDC4B682247A4C42D8A95DF"));
    try std.testing.expect(!digestHexMatches(Sha1, hasher.finalResult(), "abc"));
    _ = &digest;
}
