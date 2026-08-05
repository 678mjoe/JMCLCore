const std = @import("std");
const artifact_store = @import("../install/artifact_store.zig");
const install_executor = @import("../install/installer.zig");
const install_plan = @import("../install/plan.zig");
const java_runtime = @import("java_runtime.zig");
const java_runtime_store = @import("java_runtime_store.zig");
const runtime_installer = @import("runtime_installer.zig");

pub const ProviderChoice = enum {
    auto,
    mojang,
    zulu,

    pub fn parse(value: []const u8) ?ProviderChoice {
        inline for (std.meta.fields(ProviderChoice)) |field| {
            if (std.mem.eql(u8, value, field.name)) return @enumFromInt(field.value);
        }
        return null;
    }
};

pub const InstallParams = struct {
    component: ?[]const u8 = null,
    major: ?u16 = null,
    provider: ProviderChoice = .auto,
    platform: ?[]const u8 = null,
    store_directory: []const u8 = artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};

pub fn list(
    allocator: std.mem.Allocator,
    io: std.Io,
    store_directory: []const u8,
) !java_runtime_store.List {
    const store_dir = try install_executor.openRoot(io, store_directory);
    defer store_dir.close(io);
    var runtimes_dir = try java_runtime_store.openRuntimesDir(io, store_dir);
    defer runtimes_dir.close(io);
    java_runtime_store.cleanupStaging(io, runtimes_dir);
    return java_runtime_store.list(allocator, io, runtimes_dir);
}

pub fn remove(io: std.Io, store_directory: []const u8, runtime_name: []const u8) !void {
    const store_dir = try install_executor.openRoot(io, store_directory);
    defer store_dir.close(io);
    var runtimes_dir = try java_runtime_store.openRuntimesDir(io, store_dir);
    defer runtimes_dir.close(io);
    try java_runtime_store.remove(io, runtimes_dir, runtime_name);
}

pub fn install(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: ?*const std.process.Environ.Map,
    fetcher: runtime_installer.Fetcher,
    params: InstallParams,
    reporter: ?runtime_installer.Reporter,
) !runtime_installer.Outcome {
    if (params.component != null and params.major != null) return error.InvalidInput;
    if (params.component == null and params.major == null) return error.InvalidInput;
    if (params.component != null and params.provider == .zulu) return error.InvalidInput;

    const platform = params.platform orelse blk: {
        const host = install_plan.Target.host() catch return error.UnsupportedPlatform;
        break :blk java_runtime_store.platformName(host) orelse return error.UnsupportedPlatform;
    };
    if (java_runtime_store.parsePlatform(platform) == null) return error.InvalidRuntimePlatform;

    const store_dir = try install_executor.openRoot(io, params.store_directory);
    defer store_dir.close(io);
    var runtimes_dir = try java_runtime_store.openRuntimesDir(io, store_dir);
    defer runtimes_dir.close(io);

    const options: runtime_installer.Options = .{
        .workers = params.workers,
        .retries = params.retries,
    };
    const environ = environ_map orelse return error.EnvironmentUnavailable;

    if (params.component) |component| {
        return runtime_installer.installMojang(
            allocator,
            io,
            environ,
            fetcher,
            runtimes_dir,
            component,
            platform,
            options,
            reporter,
        );
    }

    const major = params.major.?;
    if (params.provider != .zulu) {
        for (runtime_installer.known_components) |entry| {
            if (entry.major != major) continue;
            const outcome = runtime_installer.installMojang(
                allocator,
                io,
                environ,
                fetcher,
                runtimes_dir,
                entry.name,
                platform,
                options,
                reporter,
            ) catch |err| switch (err) {
                error.RuntimeComponentUnavailable => continue,
                else => |other| return other,
            };
            return outcome;
        }
        if (params.provider == .mojang) return error.RuntimeComponentUnavailable;
    }
    return runtime_installer.installZulu(
        allocator,
        io,
        environ,
        fetcher,
        runtimes_dir,
        major,
        platform,
        options,
        reporter,
    );
}

pub const Policy = enum {
    auto,
    local,
    managed,

    pub fn parse(value: []const u8) ?Policy {
        inline for (std.meta.fields(Policy)) |field| {
            if (std.mem.eql(u8, value, field.name)) return @enumFromInt(field.value);
        }
        return null;
    }
};

pub const Request = struct {
    required_major: u16,
    component: ?[]const u8 = null,
    arch: install_plan.Architecture,
    policy: Policy = .auto,
    java_paths: []const []const u8 = &.{},
    java_override: ?[]const u8 = null,
    store_directory: []const u8 = artifact_store.default_directory,
    allow_download: bool = false,
};

pub const Source = enum {
    local,
    managed,
    forced,
};

pub const Result = struct {
    arena: *std.heap.ArenaAllocator,
    runtime: java_runtime.Runtime,
    source: Source,

    pub fn forced(self: Result) bool {
        return self.source == .forced;
    }

    pub fn deinit(self: *Result) void {
        const allocator = self.arena.child_allocator;
        self.arena.deinit();
        allocator.destroy(self.arena);
        self.* = undefined;
    }
};

pub const ResolveError = error{
    InvalidInput,
    EnvironmentUnavailable,
    JavaNotFound,
    JavaIncompatible,
    JavaDownloadFailed,
    OutOfMemory,
    Canceled,
};

fn dupeRuntime(allocator: std.mem.Allocator, runtime: java_runtime.Runtime) !java_runtime.Runtime {
    var copy = runtime;
    copy.executable = try allocator.dupe(u8, runtime.executable);
    copy.specification_version = try allocator.dupe(u8, runtime.specification_version);
    copy.os_arch = try allocator.dupe(u8, runtime.os_arch);
    if (runtime.version) |value| copy.version = try allocator.dupe(u8, value);
    if (runtime.runtime_version) |value| copy.runtime_version = try allocator.dupe(u8, value);
    if (runtime.vendor) |value| copy.vendor = try allocator.dupe(u8, value);
    if (runtime.java_home) |value| copy.java_home = try allocator.dupe(u8, value);
    if (runtime.vm_name) |value| copy.vm_name = try allocator.dupe(u8, value);
    if (runtime.os_version) |value| copy.os_version = try allocator.dupe(u8, value);
    return copy;
}

const LocalMatch = struct {
    runtime: ?java_runtime.Runtime,
    probed: usize,
};

fn tryLocal(
    backing: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    arena: std.mem.Allocator,
    request: Request,
) ResolveError!LocalMatch {
    var detection = java_runtime.detect(backing, io, environ_map, request.java_paths) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return .{ .runtime = null, .probed = 0 },
    };
    defer detection.deinit();
    const runtime = java_runtime.selectCompatible(detection, request.required_major, request.arch) catch |err| switch (err) {
        error.NoCompatibleJava => return .{ .runtime = null, .probed = detection.runtimes.len },
    };
    return .{ .runtime = try dupeRuntime(arena, runtime), .probed = detection.runtimes.len };
}

fn managedCandidates(
    backing: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    arena: std.mem.Allocator,
    request: Request,
) ResolveError!?java_runtime.Runtime {
    var runtimes = list(backing, io, request.store_directory) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return null,
    };
    defer runtimes.deinit();
    const host = install_plan.Target.host() catch return null;

    var pass: usize = 0;
    while (pass < 2) : (pass += 1) {
        for (runtimes.items) |document| {
            const receipt = document.value();
            if (receipt.major_version != request.required_major) continue;
            const platform = java_runtime_store.parsePlatform(receipt.platform) orelse continue;
            if (platform.os != host.os or platform.arch != request.arch) continue;
            if (pass == 0) {
                const component = request.component orelse break;
                const receipt_component = receipt.component orelse continue;
                if (!std.mem.eql(u8, receipt_component, component)) continue;
            }
            const executable = std.fs.path.join(arena, &.{
                request.store_directory,
                java_runtime_store.directory_name,
                document.name,
                receipt.executable,
            }) catch return error.OutOfMemory;
            const runtime = java_runtime.probeExecutable(
                arena,
                backing,
                io,
                environ_map,
                executable,
                .managed,
            ) catch continue;
            return runtime;
        }
    }
    return null;
}

fn download(
    backing: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    fetcher: runtime_installer.Fetcher,
    arena: std.mem.Allocator,
    request: Request,
    reporter: ?runtime_installer.Reporter,
) ResolveError!?java_runtime.Runtime {
    var outcome = install(backing, io, environ_map, fetcher, .{
        .major = request.required_major,
        .store_directory = request.store_directory,
    }, reporter) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        error.Canceled => return error.Canceled,
        else => return error.JavaDownloadFailed,
    };
    defer outcome.deinit();
    const executable = std.fs.path.join(arena, &.{
        request.store_directory,
        java_runtime_store.directory_name,
        outcome.runtime_name,
        outcome.receipt.executable,
    }) catch return error.OutOfMemory;
    const runtime = java_runtime.probeExecutable(
        arena,
        backing,
        io,
        environ_map,
        executable,
        .managed,
    ) catch return error.JavaIncompatible;
    return runtime;
}

pub fn resolve(
    backing: std.mem.Allocator,
    io: std.Io,
    environ_map: ?*const std.process.Environ.Map,
    fetcher: runtime_installer.Fetcher,
    request: Request,
    reporter: ?runtime_installer.Reporter,
) ResolveError!Result {
    if (request.required_major == 0) return error.InvalidInput;
    const arena = try backing.create(std.heap.ArenaAllocator);
    errdefer backing.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(backing);
    errdefer arena.deinit();
    const result_allocator = arena.allocator();

    if (request.java_override) |override| {
        if (request.java_paths.len != 0) return error.InvalidInput;
        if (!std.fs.path.isAbsolute(override)) return error.InvalidInput;
        const environ = environ_map orelse return error.EnvironmentUnavailable;
        const runtime = java_runtime.probeExecutable(
            result_allocator,
            backing,
            io,
            environ,
            override,
            .explicit,
        ) catch return error.JavaNotFound;
        return .{ .arena = arena, .runtime = runtime, .source = .forced };
    }

    const environ = environ_map orelse return error.EnvironmentUnavailable;
    var probed: usize = 0;
    if (request.policy != .managed) {
        const local = try tryLocal(backing, io, environ, result_allocator, request);
        probed += local.probed;
        if (local.runtime) |runtime| {
            return .{ .arena = arena, .runtime = runtime, .source = .local };
        }
    }
    if (request.policy != .local) {
        if (try managedCandidates(backing, io, environ, result_allocator, request)) |runtime| {
            return .{ .arena = arena, .runtime = runtime, .source = .managed };
        }
    }
    if (request.policy == .managed) {
        const local = try tryLocal(backing, io, environ, result_allocator, request);
        probed += local.probed;
        if (local.runtime) |runtime| {
            return .{ .arena = arena, .runtime = runtime, .source = .local };
        }
    }
    if (request.allow_download and request.policy != .local) {
        if (try download(backing, io, environ, fetcher, result_allocator, request, reporter)) |runtime| {
            return .{ .arena = arena, .runtime = runtime, .source = .managed };
        }
    }
    return if (probed > 0) error.JavaIncompatible else error.JavaNotFound;
}
