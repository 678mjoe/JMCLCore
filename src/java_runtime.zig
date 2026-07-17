const std = @import("std");
const builtin = @import("builtin");
const install_plan = @import("install_plan.zig");
const version_metadata = @import("version_metadata.zig");

const max_probe_output = 256 * 1024;

pub const CandidateSource = enum {
    explicit,
    java_home,
    platform,
    path,
};

pub const Candidate = struct {
    executable: []u8,
    source: CandidateSource,
};

pub const CandidateList = struct {
    allocator: std.mem.Allocator,
    items: []Candidate,

    pub fn deinit(list: *CandidateList) void {
        for (list.items) |candidate| list.allocator.free(candidate.executable);
        list.allocator.free(list.items);
        list.* = undefined;
    }
};

pub const Runtime = struct {
    executable: []const u8,
    source: CandidateSource,
    major_version: u16,
    specification_version: []const u8,
    version: ?[]const u8,
    runtime_version: ?[]const u8,
    vendor: ?[]const u8,
    java_home: ?[]const u8,
    vm_name: ?[]const u8,
    os_arch: []const u8,
    os_version: ?[]const u8,
    architecture: ?install_plan.Architecture,
    data_model: ?u8,
};

pub const Failure = struct {
    executable: []const u8,
    source: CandidateSource,
    error_name: []const u8,
};

pub const Detection = struct {
    arena: *std.heap.ArenaAllocator,
    runtimes: []const Runtime,
    failures: []const Failure,

    pub fn deinit(detection: *Detection) void {
        const allocator = detection.arena.child_allocator;
        detection.arena.deinit();
        allocator.destroy(detection.arena);
        detection.* = undefined;
    }
};

const Properties = struct {
    specification_version: []const u8,
    version: ?[]const u8,
    runtime_version: ?[]const u8,
    vendor: ?[]const u8,
    java_home: ?[]const u8,
    vm_name: ?[]const u8,
    os_arch: []const u8,
    os_version: ?[]const u8,
    data_model: ?[]const u8,
};

pub fn requiredMajor(metadata: version_metadata.Metadata) !u16 {
    if (metadata.javaVersion) |requirement| {
        if (requirement.majorVersion == 0) return error.InvalidJavaRequirement;
        return requirement.majorVersion;
    }
    return 8;
}

pub fn parseMajor(version: []const u8) !u16 {
    const trimmed = std.mem.trim(u8, version, " \t\r\n\"");
    if (trimmed.len == 0) return error.InvalidJavaVersion;

    var start: usize = 0;
    if (std.mem.startsWith(u8, trimmed, "1.")) start = 2;
    var end = start;
    while (end < trimmed.len and std.ascii.isDigit(trimmed[end])) : (end += 1) {}
    if (end == start) return error.InvalidJavaVersion;
    const major = std.fmt.parseInt(u16, trimmed[start..end], 10) catch
        return error.InvalidJavaVersion;
    if (major == 0 or major > 1000) return error.InvalidJavaVersion;
    return major;
}

pub fn discoverCandidates(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    explicit: []const []const u8,
) !CandidateList {
    var candidates: std.ArrayList(Candidate) = .empty;
    errdefer {
        for (candidates.items) |candidate| allocator.free(candidate.executable);
        candidates.deinit(allocator);
    }
    var seen: std.StringHashMapUnmanaged(void) = .empty;
    defer seen.deinit(allocator);

    for (explicit) |path| try addCandidate(allocator, &candidates, &seen, path, .explicit, false, io);

    const executable_name = if (builtin.os.tag == .windows) "java.exe" else "java";
    if (environ_map.get("JAVA_HOME")) |java_home| {
        const path = try std.fs.path.join(allocator, &.{ java_home, "bin", executable_name });
        defer allocator.free(path);
        try addCandidate(allocator, &candidates, &seen, path, .java_home, true, io);
    }

    try discoverPlatformCandidates(allocator, io, environ_map, &candidates, &seen);

    if (environ_map.get("PATH")) |path_value| {
        var entries = std.mem.splitScalar(u8, path_value, std.fs.path.delimiter);
        while (entries.next()) |entry| {
            const directory = if (entry.len == 0) "." else entry;
            const path = try std.fs.path.join(allocator, &.{ directory, executable_name });
            defer allocator.free(path);
            try addCandidate(allocator, &candidates, &seen, path, .path, true, io);
        }
    }

    if (candidates.items.len == 0) return error.JavaNotFound;
    return .{
        .allocator = allocator,
        .items = try candidates.toOwnedSlice(allocator),
    };
}

pub fn detect(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    explicit: []const []const u8,
) !Detection {
    var candidates = try discoverCandidates(allocator, io, environ_map, explicit);
    defer candidates.deinit();

    const arena = try allocator.create(std.heap.ArenaAllocator);
    errdefer allocator.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(allocator);
    errdefer arena.deinit();
    const result_allocator = arena.allocator();
    var runtimes: std.ArrayList(Runtime) = .empty;
    var failures: std.ArrayList(Failure) = .empty;
    var runtime_homes: std.StringHashMapUnmanaged(void) = .empty;

    for (candidates.items) |candidate| {
        const runtime = probe(
            result_allocator,
            allocator,
            io,
            environ_map,
            candidate,
        ) catch |err| {
            try failures.append(result_allocator, .{
                .executable = try result_allocator.dupe(u8, candidate.executable),
                .source = candidate.source,
                .error_name = @errorName(err),
            });
            continue;
        };
        if (runtime.java_home) |home| {
            const result = try runtime_homes.getOrPut(result_allocator, home);
            if (result.found_existing) continue;
        }
        try runtimes.append(result_allocator, runtime);
    }

    return .{
        .arena = arena,
        .runtimes = try runtimes.toOwnedSlice(result_allocator),
        .failures = try failures.toOwnedSlice(result_allocator),
    };
}

fn discoverPlatformCandidates(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    candidates: *std.ArrayList(Candidate),
    seen: *std.StringHashMapUnmanaged(void),
) !void {
    switch (builtin.os.tag) {
        .macos => {
            const result = std.process.run(allocator, io, .{
                .argv = &.{ "/usr/libexec/java_home", "-V" },
                .environ_map = environ_map,
                .stdout_limit = .limited(64 * 1024),
                .stderr_limit = .limited(64 * 1024),
            }) catch return;
            defer allocator.free(result.stdout);
            defer allocator.free(result.stderr);

            var lines = std.mem.splitScalar(u8, result.stderr, '\n');
            while (lines.next()) |raw_line| {
                const line = std.mem.trim(u8, raw_line, " \t\r");
                if (line.len == 0) continue;
                const separator = std.mem.lastIndexOfScalar(u8, line, ' ');
                const home = if (separator) |position| line[position + 1 ..] else line;
                if (!std.fs.path.isAbsolute(home)) continue;
                const path = try std.fs.path.join(allocator, &.{ home, "bin", "java" });
                defer allocator.free(path);
                try addCandidate(allocator, candidates, seen, path, .platform, true, io);
            }
        },
        .linux => {
            try discoverJavaHomeDirectory(allocator, io, candidates, seen, "/usr/lib/jvm", "java");
            try discoverJavaHomeDirectory(allocator, io, candidates, seen, "/usr/java", "java");
        },
        else => {},
    }
}

fn discoverJavaHomeDirectory(
    allocator: std.mem.Allocator,
    io: std.Io,
    candidates: *std.ArrayList(Candidate),
    seen: *std.StringHashMapUnmanaged(void),
    base_path: []const u8,
    executable_name: []const u8,
) !void {
    const directory = std.Io.Dir.openDirAbsolute(io, base_path, .{ .iterate = true }) catch |err| switch (err) {
        error.FileNotFound, error.NotDir, error.AccessDenied, error.PermissionDenied => return,
        else => |other| return other,
    };
    defer directory.close(io);
    var iterator = directory.iterate();
    while (try iterator.next(io)) |entry| {
        if (std.mem.eql(u8, entry.name, ".") or std.mem.eql(u8, entry.name, "..")) continue;
        const path = try std.fs.path.join(allocator, &.{ base_path, entry.name, "bin", executable_name });
        defer allocator.free(path);
        try addCandidate(allocator, candidates, seen, path, .platform, true, io);
    }
}

pub fn selectCompatible(
    detection: Detection,
    required_major: u16,
    architecture: install_plan.Architecture,
) !Runtime {
    for (detection.runtimes) |runtime| {
        if (runtime.major_version != required_major) continue;
        if (runtime.architecture == null or runtime.architecture.? != architecture) continue;
        return runtime;
    }
    return error.NoCompatibleJava;
}

fn probe(
    result_allocator: std.mem.Allocator,
    temporary_allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    candidate: Candidate,
) !Runtime {
    var clean_environment = try environ_map.clone(temporary_allocator);
    defer clean_environment.deinit();
    _ = clean_environment.swapRemove("JAVA_TOOL_OPTIONS");
    _ = clean_environment.swapRemove("_JAVA_OPTIONS");
    _ = clean_environment.swapRemove("JDK_JAVA_OPTIONS");

    const deadline = std.Io.Clock.Timestamp.fromNow(io, .{
        .raw = .fromSeconds(10),
        .clock = .awake,
    });
    const result = try std.process.run(temporary_allocator, io, .{
        .argv = &.{ candidate.executable, "-XshowSettings:properties", "-version" },
        .environ_map = &clean_environment,
        .stdout_limit = .limited(max_probe_output),
        .stderr_limit = .limited(max_probe_output),
        .reserve_amount = 4096,
        .timeout = .{ .deadline = deadline },
        .create_no_window = true,
    });
    defer temporary_allocator.free(result.stdout);
    defer temporary_allocator.free(result.stderr);
    switch (result.term) {
        .exited => |code| if (code != 0) return error.JavaExitedNonZero,
        else => return error.JavaAbnormalTermination,
    }

    const properties = try parseProperties(result.stderr, result.stdout);
    const major = try parseMajor(properties.specification_version);
    const architecture = install_plan.Architecture.parse(properties.os_arch);
    const data_model: ?u8 = if (properties.data_model) |value|
        std.fmt.parseInt(u8, value, 10) catch return error.InvalidJavaDataModel
    else
        null;
    if (data_model) |bits| {
        if (bits != 32 and bits != 64) return error.InvalidJavaDataModel;
        if (architecture) |arch| {
            if (architectureBits(arch) != bits) return error.JavaArchitectureConflict;
        }
    }

    return .{
        .executable = try result_allocator.dupe(u8, candidate.executable),
        .source = candidate.source,
        .major_version = major,
        .specification_version = try result_allocator.dupe(u8, properties.specification_version),
        .version = try duplicateOptional(result_allocator, properties.version),
        .runtime_version = try duplicateOptional(result_allocator, properties.runtime_version),
        .vendor = try duplicateOptional(result_allocator, properties.vendor),
        .java_home = try duplicateOptional(result_allocator, properties.java_home),
        .vm_name = try duplicateOptional(result_allocator, properties.vm_name),
        .os_arch = try result_allocator.dupe(u8, properties.os_arch),
        .os_version = try duplicateOptional(result_allocator, properties.os_version),
        .architecture = architecture,
        .data_model = data_model,
    };
}

fn parseProperties(primary: []const u8, secondary: []const u8) !Properties {
    const specification_version = findProperty(primary, "java.specification.version") orelse
        findProperty(secondary, "java.specification.version") orelse
        findProperty(primary, "java.version") orelse
        findProperty(secondary, "java.version") orelse
        return error.MissingJavaVersion;
    const os_arch = findProperty(primary, "os.arch") orelse
        findProperty(secondary, "os.arch") orelse
        return error.MissingJavaArchitecture;
    return .{
        .specification_version = specification_version,
        .version = findEither(primary, secondary, "java.version"),
        .runtime_version = findEither(primary, secondary, "java.runtime.version"),
        .vendor = findEither(primary, secondary, "java.vendor"),
        .java_home = findEither(primary, secondary, "java.home"),
        .vm_name = findEither(primary, secondary, "java.vm.name"),
        .os_arch = os_arch,
        .os_version = findEither(primary, secondary, "os.version"),
        .data_model = findEither(primary, secondary, "sun.arch.data.model"),
    };
}

fn findEither(primary: []const u8, secondary: []const u8, key: []const u8) ?[]const u8 {
    return findProperty(primary, key) orelse findProperty(secondary, key);
}

fn findProperty(output: []const u8, key: []const u8) ?[]const u8 {
    var lines = std.mem.splitScalar(u8, output, '\n');
    while (lines.next()) |raw_line| {
        const line = std.mem.trim(u8, raw_line, " \t\r");
        const separator = std.mem.indexOfScalar(u8, line, '=') orelse continue;
        const property_name = std.mem.trim(u8, line[0..separator], " \t\r");
        if (!std.mem.eql(u8, property_name, key)) continue;
        return std.mem.trim(u8, line[separator + 1 ..], " \t\r");
    }
    return null;
}

fn addCandidate(
    allocator: std.mem.Allocator,
    candidates: *std.ArrayList(Candidate),
    seen: *std.StringHashMapUnmanaged(void),
    path: []const u8,
    source: CandidateSource,
    require_executable: bool,
    io: std.Io,
) !void {
    if (path.len == 0 or seen.contains(path)) return;
    if (require_executable and !try candidateIsExecutable(io, path)) return;
    const owned = try allocator.dupe(u8, path);
    errdefer allocator.free(owned);
    try seen.put(allocator, owned, {});
    try candidates.append(allocator, .{ .executable = owned, .source = source });
}

fn candidateIsExecutable(io: std.Io, path: []const u8) !bool {
    const cwd = std.Io.Dir.cwd();
    const stat = cwd.statFile(io, path, .{}) catch |err| switch (err) {
        error.FileNotFound, error.NotDir, error.AccessDenied, error.PermissionDenied => return false,
        else => |other| return other,
    };
    if (stat.kind != .file) return false;
    if (builtin.os.tag == .windows) return true;
    cwd.access(io, path, .{ .execute = true }) catch |err| switch (err) {
        error.FileNotFound, error.AccessDenied, error.PermissionDenied => return false,
        else => |other| return other,
    };
    return true;
}

fn architectureBits(architecture: install_plan.Architecture) u8 {
    return switch (architecture) {
        .x86, .arm => 32,
        .x86_64, .arm64 => 64,
    };
}

fn duplicateOptional(allocator: std.mem.Allocator, value: ?[]const u8) !?[]const u8 {
    return if (value) |slice| try allocator.dupe(u8, slice) else null;
}

test "parses legacy and modern Java version strings" {
    try std.testing.expectEqual(@as(u16, 8), try parseMajor("1.8.0_402"));
    try std.testing.expectEqual(@as(u16, 17), try parseMajor("17.0.12+7"));
    try std.testing.expectEqual(@as(u16, 21), try parseMajor("21-ea"));
    try std.testing.expectError(error.InvalidJavaVersion, parseMajor("not-java"));
}

test "parses Java properties independent of indentation" {
    const properties = try parseProperties(
        \\Property settings:
        \\    java.specification.version = 21
        \\    java.version = 21.0.7
        \\    java.vendor = Example Vendor
        \\    os.arch = aarch64
        \\    sun.arch.data.model = 64
    , "");
    try std.testing.expectEqualStrings("21", properties.specification_version);
    try std.testing.expectEqualStrings("aarch64", properties.os_arch);
    try std.testing.expectEqualStrings("Example Vendor", properties.vendor.?);
}

test "probes a Java-compatible executable" {
    if (builtin.os.tag == .windows) return error.SkipZigTest;

    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    const script = try temporary.dir.createFile(io, "java", .{
        .permissions = .fromMode(0o755),
    });
    try std.Io.File.writeStreamingAll(script, io,
        \\#!/bin/sh
        \\cat >&2 <<'EOF'
        \\    java.specification.version = 17
        \\    java.version = 17.0.12
        \\    java.vendor = Test Vendor
        \\    java.home = /test/java
        \\    java.vm.name = Test VM
        \\    os.arch = amd64
        \\    sun.arch.data.model = 64
        \\EOF
        \\exit 0
        \\
    );
    script.close(io);
    const executable = try temporary.dir.realPathFileAlloc(io, "java", std.testing.allocator);
    defer std.testing.allocator.free(executable);

    var environment = std.process.Environ.Map.init(std.testing.allocator);
    defer environment.deinit();
    var detection = try detect(
        std.testing.allocator,
        io,
        &environment,
        &.{executable},
    );
    defer detection.deinit();

    try std.testing.expect(detection.runtimes.len >= 1);
    const selected = try selectCompatible(detection, 17, .x86_64);
    try std.testing.expectEqualStrings(executable, selected.executable);
    try std.testing.expectEqual(install_plan.Architecture.x86_64, selected.architecture.?);
}
