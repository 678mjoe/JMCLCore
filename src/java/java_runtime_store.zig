const std = @import("std");
const builtin = @import("builtin");
const install_plan = @import("../install/plan.zig");

pub const directory_name = "runtimes";
pub const receipt_name = ".jmcl-runtime.json";
pub const schema_version: u32 = 1;
pub const max_receipt_bytes: usize = 16 * 1024;
pub const max_runtime_name_bytes: usize = 128;
pub const max_runtimes: usize = 256;

pub const Provider = enum {
    mojang,
    zulu,

    pub fn parse(value: []const u8) ?Provider {
        inline for (std.meta.fields(Provider)) |field| {
            if (std.mem.eql(u8, value, field.name)) return @enumFromInt(field.value);
        }
        return null;
    }
};

pub const Receipt = struct {
    schema_version: u32 = schema_version,
    provider: Provider,
    id: []const u8,
    platform: []const u8,
    major_version: u16,
    java_home: []const u8,
    executable: []const u8,
    version: ?[]const u8 = null,
    vendor: ?[]const u8 = null,
    component: ?[]const u8 = null,
    package_uuid: ?[]const u8 = null,
    package_name: ?[]const u8 = null,
    source_url: []const u8,
    files_total: u32 = 0,
    bytes_total: u64 = 0,
    installed_at_ms: i64,
};

pub const Document = struct {
    allocator: std.mem.Allocator,
    name: []u8,
    parsed: std.json.Parsed(Receipt),

    pub fn deinit(document: *Document) void {
        document.allocator.free(document.name);
        document.parsed.deinit();
        document.* = undefined;
    }

    pub fn value(document: Document) Receipt {
        return document.parsed.value;
    }
};

pub const List = struct {
    allocator: std.mem.Allocator,
    items: []Document,

    pub fn deinit(list_self: *List) void {
        for (list_self.items) |*document| document.deinit();
        list_self.allocator.free(list_self.items);
        list_self.* = undefined;
    }
};

pub fn platformName(target: install_plan.Target) ?[]const u8 {
    return switch (target.os) {
        .windows => switch (target.arch) {
            .x86_64 => "windows-x64",
            .x86 => "windows-x86",
            .arm64 => "windows-arm64",
            else => null,
        },
        .osx => switch (target.arch) {
            .x86_64 => "mac-os",
            .arm64 => "mac-os-arm64",
            else => null,
        },
        .linux => switch (target.arch) {
            .x86_64 => "linux",
            .x86 => "linux-i386",
            .arm64 => "linux-arm64",
            else => null,
        },
    };
}

pub fn parsePlatform(value: []const u8) ?install_plan.Target {
    const entries = [_]struct { name: []const u8, target: install_plan.Target }{
        .{ .name = "windows-x64", .target = .{ .os = .windows, .arch = .x86_64 } },
        .{ .name = "windows-x86", .target = .{ .os = .windows, .arch = .x86 } },
        .{ .name = "windows-arm64", .target = .{ .os = .windows, .arch = .arm64 } },
        .{ .name = "mac-os", .target = .{ .os = .osx, .arch = .x86_64 } },
        .{ .name = "mac-os-arm64", .target = .{ .os = .osx, .arch = .arm64 } },
        .{ .name = "linux", .target = .{ .os = .linux, .arch = .x86_64 } },
        .{ .name = "linux-i386", .target = .{ .os = .linux, .arch = .x86 } },
        .{ .name = "linux-arm64", .target = .{ .os = .linux, .arch = .arm64 } },
    };
    for (entries) |entry| {
        if (std.mem.eql(u8, value, entry.name)) return entry.target;
    }
    return null;
}

pub fn validateId(id: []const u8) !void {
    if (id.len == 0 or id.len > 64) return error.InvalidRuntimeId;
    for (id) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '-' and byte != '_' and byte != '.') {
            return error.InvalidRuntimeId;
        }
    }
}

pub fn runtimeDirName(buffer: []u8, provider: Provider, id: []const u8, platform: []const u8) ![]const u8 {
    try validateId(id);
    if (parsePlatform(platform) == null) return error.InvalidRuntimePlatform;
    return std.fmt.bufPrint(buffer, "{s}-{s}-{s}", .{ @tagName(provider), id, platform }) catch
        return error.InvalidRuntimeId;
}

pub fn validateRuntimeName(name: []const u8) !void {
    if (name.len == 0 or name.len > max_runtime_name_bytes) return error.InvalidRuntimeName;
    for (name) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '-' and byte != '_' and byte != '.') {
            return error.InvalidRuntimeName;
        }
    }
    if (name[0] == '.') return error.InvalidRuntimeName;
}

pub fn stagingName(buffer: []u8, name: []const u8, random: u64) ![]const u8 {
    return std.fmt.bufPrint(buffer, "{s}.staging-{x:0>16}", .{ name, random }) catch
        return error.InvalidRuntimeName;
}

pub fn isStagingName(name: []const u8) bool {
    return std.mem.indexOf(u8, name, ".staging-") != null;
}

pub fn validateReceipt(receipt: Receipt) !void {
    if (receipt.schema_version != schema_version) return error.UnsupportedReceiptSchema;
    try validateId(receipt.id);
    if (parsePlatform(receipt.platform) == null) return error.InvalidRuntimePlatform;
    if (receipt.major_version == 0) return error.InvalidRuntimeReceipt;
    try validateRelativePath(receipt.executable);
    if (receipt.java_home.len != 0) try validateRelativePath(receipt.java_home);
    if (receipt.source_url.len == 0 or receipt.source_url.len > 2048) return error.InvalidRuntimeReceipt;
    if (!std.mem.startsWith(u8, receipt.source_url, "https://")) return error.InvalidRuntimeReceipt;
}

pub fn validateRelativePath(path: []const u8) !void {
    if (path.len == 0 or path.len > 1024) return error.InvalidRelativePath;
    if (std.fs.path.isAbsolute(path)) return error.InvalidRelativePath;
    var parts = std.mem.splitScalar(u8, path, '/');
    while (parts.next()) |part| {
        if (part.len == 0 or std.mem.eql(u8, part, ".") or std.mem.eql(u8, part, "..")) {
            return error.InvalidRelativePath;
        }
        if (std.mem.indexOfAny(u8, part, "\\:") != null) return error.InvalidRelativePath;
        if (std.mem.indexOfScalar(u8, part, 0) != null) return error.InvalidRelativePath;
    }
}

pub fn loadReceipt(
    allocator: std.mem.Allocator,
    io: std.Io,
    runtime_dir: std.Io.Dir,
) !std.json.Parsed(Receipt) {
    const bytes = runtime_dir.readFileAlloc(io, receipt_name, allocator, .limited(max_receipt_bytes + 1)) catch |err| switch (err) {
        error.FileNotFound => return error.ReceiptNotFound,
        else => |other| return other,
    };
    defer allocator.free(bytes);
    if (bytes.len > max_receipt_bytes) return error.InvalidRuntimeReceipt;
    const parsed = std.json.parseFromSlice(Receipt, allocator, bytes, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
    }) catch return error.InvalidRuntimeReceipt;
    validateReceipt(parsed.value) catch {
        parsed.deinit();
        return error.InvalidRuntimeReceipt;
    };
    return parsed;
}

pub fn writeReceipt(io: std.Io, runtime_dir: std.Io.Dir, receipt: Receipt) !void {
    try validateReceipt(receipt);
    var atomic_file = try runtime_dir.createFileAtomic(io, receipt_name, .{ .replace = true });
    defer atomic_file.deinit(io);
    var buffer: [4096]u8 = undefined;
    var writer: std.Io.File.Writer = .init(atomic_file.file, io, &buffer);
    var json: std.json.Stringify = .{ .writer = &writer.interface };
    try json.write(receipt);
    try writer.interface.flush();
    try atomic_file.file.sync(io);
    try atomic_file.replace(io);
}

pub fn openRuntimesDir(io: std.Io, store: std.Io.Dir) !std.Io.Dir {
    return store.createDirPathOpen(io, directory_name, .{
        .open_options = .{ .iterate = true },
    });
}

pub fn list(allocator: std.mem.Allocator, io: std.Io, runtimes_dir: std.Io.Dir) !List {
    var documents: std.ArrayList(Document) = .empty;
    errdefer {
        for (documents.items) |*document| document.deinit();
        documents.deinit(allocator);
    }

    var iterator = runtimes_dir.iterate();
    while (try iterator.next(io)) |entry| {
        if (entry.kind != .directory) continue;
        if (isStagingName(entry.name)) continue;
        validateRuntimeName(entry.name) catch continue;
        if (documents.items.len >= max_runtimes) return error.TooManyRuntimes;

        var runtime_dir = runtimes_dir.openDir(io, entry.name, .{}) catch continue;
        defer runtime_dir.close(io);
        const receipt = loadReceipt(allocator, io, runtime_dir) catch continue;

        const name_copy = try allocator.dupe(u8, entry.name);
        errdefer allocator.free(name_copy);
        try documents.append(allocator, .{
            .allocator = allocator,
            .name = name_copy,
            .parsed = receipt,
        });
    }

    const items = try documents.toOwnedSlice(allocator);
    std.mem.sort(Document, items, {}, struct {
        fn lessThan(_: void, a: Document, b: Document) bool {
            return std.mem.lessThan(u8, a.name, b.name);
        }
    }.lessThan);
    return .{ .allocator = allocator, .items = items };
}

pub fn cleanupStaging(io: std.Io, runtimes_dir: std.Io.Dir) void {
    var iterator = runtimes_dir.iterate();
    while (iterator.next(io) catch null) |entry| {
        if (entry.kind != .directory) continue;
        if (!isStagingName(entry.name)) continue;
        runtimes_dir.deleteTree(io, entry.name) catch {};
    }
}

pub fn remove(io: std.Io, runtimes_dir: std.Io.Dir, name: []const u8) !void {
    try validateRuntimeName(name);
    if (isStagingName(name)) return error.InvalidRuntimeName;
    var runtime_dir = runtimes_dir.openDir(io, name, .{}) catch |err| switch (err) {
        error.FileNotFound => return error.RuntimeNotFound,
        else => |other| return other,
    };
    runtime_dir.close(io);
    try runtimes_dir.deleteTree(io, name);
}

pub const HomeDiscovery = struct {
    java_home: []u8,
    executable: []u8,

    pub fn deinit(discovery: *HomeDiscovery, allocator: std.mem.Allocator) void {
        allocator.free(discovery.java_home);
        allocator.free(discovery.executable);
        discovery.* = undefined;
    }
};

pub fn findJavaHome(
    allocator: std.mem.Allocator,
    io: std.Io,
    runtime_dir: std.Io.Dir,
) !HomeDiscovery {
    const executable_name = if (builtin.os.tag == .windows) "java.exe" else "java";
    var matches: std.ArrayList([]u8) = .empty;
    defer {
        for (matches.items) |match| allocator.free(match);
        matches.deinit(allocator);
    }
    try collectJavaHomes(allocator, io, runtime_dir, "", executable_name, 0, &matches);
    if (matches.items.len == 0) return error.JavaHomeNotFound;

    std.mem.sort([]u8, matches.items, {}, struct {
        fn lessThan(_: void, a: []u8, b: []u8) bool {
            const depth_a = std.mem.count(u8, a, "/");
            const depth_b = std.mem.count(u8, b, "/");
            if (depth_a != depth_b) return depth_a < depth_b;
            return std.mem.lessThan(u8, a, b);
        }
    }.lessThan);

    const chosen = matches.items[0];
    const home = try allocator.dupe(u8, chosen);
    errdefer allocator.free(home);
    const executable = if (home.len == 0)
        try std.fmt.allocPrint(allocator, "bin/{s}", .{executable_name})
    else
        try std.fmt.allocPrint(allocator, "{s}/bin/{s}", .{ home, executable_name });
    return .{ .java_home = home, .executable = executable };
}

fn collectJavaHomes(
    allocator: std.mem.Allocator,
    io: std.Io,
    root: std.Io.Dir,
    relative: []const u8,
    executable_name: []const u8,
    depth: usize,
    matches: *std.ArrayList([]u8),
) !void {
    if (depth > 5) return;
    var dir = if (relative.len == 0) root else try root.openDir(io, relative, .{ .iterate = true });
    defer if (relative.len != 0) dir.close(io);

    const java_rel = try std.fmt.allocPrint(allocator, "bin/{s}", .{executable_name});
    defer allocator.free(java_rel);
    const found = if (dir.access(io, java_rel, .{})) |_| true else |_| false;
    if (found) {
        try matches.append(allocator, try allocator.dupe(u8, relative));
        return;
    }

    var iterator = dir.iterate();
    while (try iterator.next(io)) |entry| {
        if (entry.kind == .sym_link) continue;
        if (entry.kind != .directory) continue;
        if (std.mem.eql(u8, entry.name, ".") or std.mem.eql(u8, entry.name, "..")) continue;
        if (entry.name.len == 0 or std.mem.indexOfAny(u8, entry.name, "/\\") != null) continue;
        const child = if (relative.len == 0)
            try allocator.dupe(u8, entry.name)
        else
            try std.fmt.allocPrint(allocator, "{s}/{s}", .{ relative, entry.name });
        defer allocator.free(child);
        try collectJavaHomes(allocator, io, root, child, executable_name, depth + 1, matches);
    }
}

test "platform names round-trip through the parser" {
    const target: install_plan.Target = .{ .os = .osx, .arch = .arm64 };
    const name = platformName(target).?;
    try std.testing.expectEqualStrings("mac-os-arm64", name);
    const parsed = parsePlatform(name).?;
    try std.testing.expectEqual(target.os, parsed.os);
    try std.testing.expectEqual(target.arch, parsed.arch);
    try std.testing.expect(platformName(.{ .os = .osx, .arch = .x86 }) == null);
    try std.testing.expect(parsePlatform("plan9-mips") == null);
}

test "runtime directory names embed provider, id, and platform" {
    var buffer: [max_runtime_name_bytes]u8 = undefined;
    const name = try runtimeDirName(&buffer, .mojang, "java-runtime-gamma", "mac-os-arm64");
    try std.testing.expectEqualStrings("mojang-java-runtime-gamma-mac-os-arm64", name);
    try validateRuntimeName(name);
    try std.testing.expectError(error.InvalidRuntimeId, runtimeDirName(&buffer, .zulu, "8/../etc", "linux"));
    try std.testing.expectError(error.InvalidRuntimePlatform, runtimeDirName(&buffer, .zulu, "8", "plan9"));
}

test "relative path validation rejects traversal and separators" {
    try validateRelativePath("bin/java");
    try validateRelativePath("jre.bundle/Contents/Home/bin/java");
    try std.testing.expectError(error.InvalidRelativePath, validateRelativePath("../escape"));
    try std.testing.expectError(error.InvalidRelativePath, validateRelativePath("a//b"));
    try std.testing.expectError(error.InvalidRelativePath, validateRelativePath("/absolute"));
    try std.testing.expectError(error.InvalidRelativePath, validateRelativePath("a\\b"));
    try std.testing.expectError(error.InvalidRelativePath, validateRelativePath(""));
}

test "receipt write, load, and validation round-trip" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    const dir = temporary.dir;

    const receipt: Receipt = .{
        .provider = .mojang,
        .id = "java-runtime-gamma",
        .platform = "mac-os-arm64",
        .major_version = 17,
        .java_home = "jre.bundle/Contents/Home",
        .executable = "jre.bundle/Contents/Home/bin/java",
        .component = "java-runtime-gamma",
        .source_url = "https://piston-meta.mojang.com/v1/packages/abc/manifest.json",
        .installed_at_ms = 1234,
    };
    try writeReceipt(io, dir, receipt);
    var loaded = try loadReceipt(std.testing.allocator, io, dir);
    defer loaded.deinit();
    try std.testing.expectEqual(@as(u16, 17), loaded.value.major_version);
    try std.testing.expectEqual(Provider.mojang, loaded.value.provider);
    try std.testing.expectEqualStrings("jre.bundle/Contents/Home", loaded.value.java_home);

    try dir.writeFile(io, .{ .sub_path = receipt_name, .data = "{\"schema_version\":99}" });
    try std.testing.expectError(error.InvalidRuntimeReceipt, loadReceipt(std.testing.allocator, io, dir));
}

test "list returns sorted receipts and skips staging directories" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    var runtimes = try temporary.dir.createDirPathOpen(io, "runtimes", .{
        .open_options = .{ .iterate = true },
    });
    defer runtimes.close(io);

    for ([_][]const u8{ "zulu-8-mac-os-arm64", "mojang-java-runtime-delta-mac-os-arm64" }) |name| {
        var dir = try runtimes.createDirPathOpen(io, name, .{});
        defer dir.close(io);
        const provider: Provider = if (std.mem.startsWith(u8, name, "zulu")) .zulu else .mojang;
        try writeReceipt(io, dir, .{
            .provider = provider,
            .id = if (provider == .zulu) "8" else "java-runtime-delta",
            .platform = "mac-os-arm64",
            .major_version = if (provider == .zulu) 8 else 21,
            .java_home = "",
            .executable = "bin/java",
            .source_url = "https://example.invalid/runtime",
            .installed_at_ms = 1,
        });
    }
    var staging = try runtimes.createDirPathOpen(io, "zulu-8-mac-os-arm64.staging-0123", .{});
    staging.close(io);

    var documents = try list(std.testing.allocator, io, runtimes);
    defer documents.deinit();
    try std.testing.expectEqual(@as(usize, 2), documents.items.len);
    try std.testing.expectEqualStrings("mojang-java-runtime-delta-mac-os-arm64", documents.items[0].name);
    try std.testing.expectEqualStrings("zulu-8-mac-os-arm64", documents.items[1].name);

    try remove(io, runtimes, "zulu-8-mac-os-arm64");
    try std.testing.expectError(error.RuntimeNotFound, remove(io, runtimes, "zulu-8-mac-os-arm64"));
    try std.testing.expectError(error.InvalidRuntimeName, remove(io, runtimes, ".."));
}

test "opened runtimes directory supports listing and staging cleanup" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{ .iterate = true });
    defer temporary.cleanup();
    var runtimes = try openRuntimesDir(io, temporary.dir);
    defer runtimes.close(io);

    try runtimes.createDirPath(io, "zulu-8-linux-x64.staging-0123");
    cleanupStaging(io, runtimes);
    try std.testing.expectError(
        error.FileNotFound,
        runtimes.access(io, "zulu-8-linux-x64.staging-0123", .{}),
    );

    var documents = try list(std.testing.allocator, io, runtimes);
    defer documents.deinit();
    try std.testing.expectEqual(@as(usize, 0), documents.items.len);
}

test "java home discovery prefers the shallowest bin/java" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{ .iterate = true });
    defer temporary.cleanup();
    const dir = temporary.dir;

    try dir.createDirPath(io, "zulu-8.jre/Contents/Home/bin");
    try dir.writeFile(io, .{ .sub_path = (if (builtin.os.tag == .windows) "zulu-8.jre/Contents/Home/bin/java.exe" else "zulu-8.jre/Contents/Home/bin/java"), .data = "" });
    var discovery = try findJavaHome(std.testing.allocator, io, dir);
    defer discovery.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings("zulu-8.jre/Contents/Home", discovery.java_home);
    try std.testing.expectEqualStrings((if (builtin.os.tag == .windows) "zulu-8.jre/Contents/Home/bin/java.exe" else "zulu-8.jre/Contents/Home/bin/java"), discovery.executable);
}

test "java home discovery finds a root-level bin/java" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{ .iterate = true });
    defer temporary.cleanup();
    const dir = temporary.dir;

    try dir.createDirPath(io, "bin");
    try dir.writeFile(io, .{ .sub_path = (if (builtin.os.tag == .windows) "bin/java.exe" else "bin/java"), .data = "" });
    try dir.createDirPath(io, "legal/java.base");
    var discovery = try findJavaHome(std.testing.allocator, io, dir);
    defer discovery.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings("", discovery.java_home);
    try std.testing.expectEqualStrings((if (builtin.os.tag == .windows) "bin/java.exe" else "bin/java"), discovery.executable);
}
