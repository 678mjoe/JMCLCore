const std = @import("std");
const builtin = @import("builtin");
const asset_index = @import("asset_index.zig");
const DownloadSource = @import("download_source.zig").DownloadSource;
const metadata = @import("version_metadata.zig");

pub const OperatingSystem = enum {
    windows,
    linux,
    osx,

    pub fn parse(value: []const u8) ?OperatingSystem {
        inline for (std.meta.fields(OperatingSystem)) |field| {
            if (std.mem.eql(u8, value, field.name)) return @enumFromInt(field.value);
        }
        return null;
    }
};

pub const Architecture = enum {
    x86,
    x86_64,
    arm,
    arm64,

    pub fn parse(value: []const u8) ?Architecture {
        if (std.mem.eql(u8, value, "amd64")) return .x86_64;
        if (std.mem.eql(u8, value, "aarch64")) return .arm64;
        inline for (std.meta.fields(Architecture)) |field| {
            if (std.mem.eql(u8, value, field.name)) return @enumFromInt(field.value);
        }
        return null;
    }

    fn bitness(architecture: Architecture) []const u8 {
        return switch (architecture) {
            .x86, .arm => "32",
            .x86_64, .arm64 => "64",
        };
    }
};

pub const Target = struct {
    os: OperatingSystem,
    arch: Architecture,
    os_version: ?[]const u8 = null,

    pub fn host() !Target {
        const os: OperatingSystem = switch (builtin.os.tag) {
            .windows => .windows,
            .linux => .linux,
            .macos => .osx,
            else => return error.UnsupportedOperatingSystem,
        };
        const arch: Architecture = switch (builtin.cpu.arch) {
            .x86 => .x86,
            .x86_64 => .x86_64,
            .arm => .arm,
            .aarch64 => .arm64,
            else => return error.UnsupportedArchitecture,
        };
        return .{ .os = os, .arch = arch };
    }
};

pub const Usage = struct {
    client: bool = false,
    asset_index: bool = false,
    library: bool = false,
    native: bool = false,
    logging_config: bool = false,
    asset_object: bool = false,
    mod: bool = false,
    resourcepack: bool = false,
    shaderpack: bool = false,
    server: bool = false,

    fn merge(usage: *Usage, other: Usage) void {
        usage.client = usage.client or other.client;
        usage.asset_index = usage.asset_index or other.asset_index;
        usage.library = usage.library or other.library;
        usage.native = usage.native or other.native;
        usage.logging_config = usage.logging_config or other.logging_config;
        usage.asset_object = usage.asset_object or other.asset_object;
        usage.mod = usage.mod or other.mod;
        usage.resourcepack = usage.resourcepack or other.resourcepack;
        usage.shaderpack = usage.shaderpack or other.shaderpack;
        usage.server = usage.server or other.server;
    }
};

pub const File = struct {
    path: []const u8,
    url: []const u8,
    fallback_url: ?[]const u8,
    sha1: []const u8,
    size: u64,
    usage: Usage,
};

pub const NativeExtraction = struct {
    archive_path: []const u8,
    exclude: []const []const u8,
};

pub const ProjectionKind = enum {
    virtual_assets,
    resources,
};

pub const AssetProjection = struct {
    source_path: []const u8,
    destination_path: []const u8,
    kind: ProjectionKind,
};

pub const Loader = enum {
    none,
    fabric,
    neoforge,
    forge,
    forge_legacy,
};

pub const Data = struct {
    version_id: []const u8,
    source: DownloadSource,
    target: Target,
    files: []const File,
    classpath: []const []const u8,
    native_extractions: []const NativeExtraction,
    asset_projections: []const AssetProjection,
    total_download_size: u64,
    java_major_version: u16 = 0,
    loader: Loader = .none,
};

pub const Plan = struct {
    arena: *std.heap.ArenaAllocator,
    value: Data,

    pub fn deinit(plan: *Plan) void {
        const allocator = plan.arena.child_allocator;
        plan.arena.deinit();
        allocator.destroy(plan.arena);
        plan.* = undefined;
    }
};

const Builder = struct {
    allocator: std.mem.Allocator,
    source: DownloadSource,
    files: std.ArrayList(File) = .empty,
    classpath: std.ArrayList([]const u8) = .empty,
    extractions: std.ArrayList(NativeExtraction) = .empty,
    projections: std.ArrayList(AssetProjection) = .empty,
    path_indexes: std.StringHashMapUnmanaged(usize) = .empty,
    classpath_paths: std.StringHashMapUnmanaged(void) = .empty,
    projection_paths: std.StringHashMapUnmanaged(usize) = .empty,
    total_download_size: u64 = 0,

    fn addFile(
        builder: *Builder,
        path: []const u8,
        url: []const u8,
        sha1: []const u8,
        size: u64,
        usage: Usage,
    ) ![]const u8 {
        try validateRelativePath(path);
        try validateDownload(url, sha1);

        if (builder.path_indexes.get(path)) |index| {
            const existing = &builder.files.items[index];
            if (!std.ascii.eqlIgnoreCase(existing.sha1, sha1) or existing.size != size) {
                return error.ConflictingDestination;
            }
            existing.usage.merge(usage);
            return existing.path;
        }

        if (builder.files.items.len >= 10_000) return error.TooManyFiles;

        const owned_path = try builder.allocator.dupe(u8, path);
        const official_url = try builder.allocator.dupe(u8, url);
        const mirror_url = try builder.source.rewriteUrl(builder.allocator, url);
        const owned_sha1 = try builder.allocator.dupe(u8, sha1);
        const index = builder.files.items.len;
        try builder.files.append(builder.allocator, .{
            .path = owned_path,
            .url = mirror_url orelse official_url,
            .fallback_url = if (mirror_url != null) official_url else null,
            .sha1 = owned_sha1,
            .size = size,
            .usage = usage,
        });
        try builder.path_indexes.put(builder.allocator, owned_path, index);
        builder.total_download_size = std.math.add(u64, builder.total_download_size, size) catch
            return error.DownloadSizeOverflow;
        return owned_path;
    }

    fn addClasspath(builder: *Builder, path: []const u8) !void {
        const result = try builder.classpath_paths.getOrPut(builder.allocator, path);
        if (result.found_existing) return;
        try builder.classpath.append(builder.allocator, path);
    }

    fn addExtraction(builder: *Builder, path: []const u8, extract: ?metadata.Extract) !void {
        const source_excludes = if (extract) |value| value.exclude else &.{};
        const excludes = try builder.allocator.alloc([]const u8, source_excludes.len);
        for (source_excludes, 0..) |exclude, index| {
            try validateArchivePrefix(exclude);
            excludes[index] = try builder.allocator.dupe(u8, exclude);
        }
        try builder.extractions.append(builder.allocator, .{
            .archive_path = path,
            .exclude = excludes,
        });
    }

    fn addProjection(
        builder: *Builder,
        source_path: []const u8,
        destination_path: []const u8,
        kind: ProjectionKind,
    ) !void {
        try validateRelativePath(destination_path);
        if (builder.projection_paths.get(destination_path)) |index| {
            const existing = builder.projections.items[index];
            if (!std.mem.eql(u8, existing.source_path, source_path)) return error.ConflictingProjection;
            return;
        }
        if (builder.projections.items.len >= 20_000) return error.TooManyProjections;

        const owned_destination = try builder.allocator.dupe(u8, destination_path);
        const index = builder.projections.items.len;
        try builder.projections.append(builder.allocator, .{
            .source_path = source_path,
            .destination_path = owned_destination,
            .kind = kind,
        });
        try builder.projection_paths.put(builder.allocator, owned_destination, index);
    }
};

pub fn build(
    allocator: std.mem.Allocator,
    version: metadata.Metadata,
    assets: ?asset_index.Index,
    source: DownloadSource,
    target: Target,
) !Plan {
    return buildWithExtras(allocator, version, assets, source, target, &.{});
}

pub fn buildServer(
    allocator: std.mem.Allocator,
    version: metadata.Metadata,
    source: DownloadSource,
    target: Target,
) !Plan {
    try validateComponent(version.id);
    const download = version.downloads.server orelse return error.ServerDownloadMissing;
    const java_major_version = if (version.javaVersion) |java| blk: {
        if (java.majorVersion == 0) return error.InvalidJavaRequirement;
        break :blk java.majorVersion;
    } else 8;
    const arena = try allocator.create(std.heap.ArenaAllocator);
    errdefer allocator.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(allocator);
    errdefer arena.deinit();
    const plan_allocator = arena.allocator();
    var builder: Builder = .{
        .allocator = plan_allocator,
        .source = source,
    };
    _ = try builder.addFile(
        "server.jar",
        download.url,
        download.sha1,
        download.size,
        .{ .server = true },
    );
    return .{
        .arena = arena,
        .value = .{
            .version_id = try plan_allocator.dupe(u8, version.id),
            .source = source,
            .target = target,
            .files = try builder.files.toOwnedSlice(plan_allocator),
            .classpath = &.{},
            .native_extractions = &.{},
            .asset_projections = &.{},
            .total_download_size = builder.total_download_size,
            .java_major_version = java_major_version,
        },
    };
}

pub fn buildWithExtras(
    allocator: std.mem.Allocator,
    version: metadata.Metadata,
    assets: ?asset_index.Index,
    source: DownloadSource,
    target: Target,
    extra_files: []const File,
) !Plan {
    try validateComponent(version.id);

    const arena = try allocator.create(std.heap.ArenaAllocator);
    errdefer allocator.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(allocator);
    errdefer arena.deinit();
    const plan_allocator = arena.allocator();

    var builder: Builder = .{
        .allocator = plan_allocator,
        .source = source,
    };

    const client_path = try std.fmt.allocPrint(plan_allocator, "versions/{s}/{s}.jar", .{ version.id, version.id });
    const owned_client_path = try builder.addFile(
        client_path,
        version.downloads.client.url,
        version.downloads.client.sha1,
        version.downloads.client.size,
        .{ .client = true },
    );

    if (version.assetIndex) |asset_descriptor| {
        try validateComponent(asset_descriptor.id);
        const path = try std.fmt.allocPrint(plan_allocator, "assets/indexes/{s}.json", .{asset_descriptor.id});
        _ = try builder.addFile(path, asset_descriptor.url, asset_descriptor.sha1, asset_descriptor.size, .{ .asset_index = true });

        if (assets) |index| {
            var iterator = index.objects.map.iterator();
            while (iterator.next()) |entry| {
                const logical_name = entry.key_ptr.*;
                const object = entry.value_ptr.*;
                try validateRelativePath(logical_name);
                if (object.hash.len != 40) return error.InvalidAssetHash;
                for (object.hash) |character| {
                    if (!std.ascii.isHex(character)) return error.InvalidAssetHash;
                }

                const object_path = try std.fmt.allocPrint(
                    plan_allocator,
                    "assets/objects/{s}/{s}",
                    .{ object.hash[0..2], object.hash },
                );
                const object_url = try std.fmt.allocPrint(
                    plan_allocator,
                    "https://resources.download.minecraft.net/{s}/{s}",
                    .{ object.hash[0..2], object.hash },
                );
                const owned_object_path = try builder.addFile(
                    object_path,
                    object_url,
                    object.hash,
                    object.size,
                    .{ .asset_object = true },
                );

                if (index.virtual) {
                    const destination = try std.fmt.allocPrint(
                        plan_allocator,
                        "assets/virtual/{s}/{s}",
                        .{ asset_descriptor.id, logical_name },
                    );
                    try builder.addProjection(owned_object_path, destination, .virtual_assets);
                }
                if (index.map_to_resources) {
                    const destination = try std.fmt.allocPrint(plan_allocator, "resources/{s}", .{logical_name});
                    try builder.addProjection(owned_object_path, destination, .resources);
                }
            }
        }
    } else if (assets != null) {
        return error.UnexpectedAssetIndex;
    }

    if (version.logging) |logging| {
        try validateComponent(logging.client.file.id);
        const path = try std.fmt.allocPrint(plan_allocator, "assets/log_configs/{s}", .{logging.client.file.id});
        _ = try builder.addFile(
            path,
            logging.client.file.url,
            logging.client.file.sha1,
            logging.client.file.size,
            .{ .logging_config = true },
        );
    }

    for (version.libraries) |library| {
        if (!try rulesAllow(library.rules, target)) continue;

        if (library.natives) |natives| {
            const classifier_template = natives.map.get(@tagName(target.os)) orelse continue;
            const classifier = try std.mem.replaceOwned(
                u8,
                plan_allocator,
                classifier_template,
                "${arch}",
                target.arch.bitness(),
            );
            const downloads = library.downloads orelse return error.MissingLibraryDownloads;
            const classifiers = downloads.classifiers orelse return error.MissingNativeClassifiers;
            const artifact = classifiers.map.get(classifier) orelse return error.MissingNativeClassifier;
            const path = try std.fmt.allocPrint(plan_allocator, "libraries/{s}", .{artifact.path});
            const owned_path = try builder.addFile(path, artifact.url, artifact.sha1, artifact.size, .{ .native = true });
            try builder.addExtraction(owned_path, library.extract);
            continue;
        }

        const downloads = library.downloads orelse return error.MissingLibraryDownloads;
        const artifact = downloads.artifact orelse return error.MissingLibraryArtifact;
        const path = try std.fmt.allocPrint(plan_allocator, "libraries/{s}", .{artifact.path});
        const owned_path = try builder.addFile(path, artifact.url, artifact.sha1, artifact.size, .{ .library = true });
        try builder.addClasspath(owned_path);
    }

    const client_classpath_path = if (version.patched_client) |override| blk: {
        try validateRelativePath(override);
        break :blk try plan_allocator.dupe(u8, override);
    } else owned_client_path;
    try builder.addClasspath(client_classpath_path);

    for (extra_files) |file| {
        _ = try builder.addFile(file.path, file.url, file.sha1, file.size, file.usage);
    }

    return .{
        .arena = arena,
        .value = .{
            .version_id = try plan_allocator.dupe(u8, version.id),
            .source = source,
            .target = target,
            .files = try builder.files.toOwnedSlice(plan_allocator),
            .classpath = try builder.classpath.toOwnedSlice(plan_allocator),
            .native_extractions = try builder.extractions.toOwnedSlice(plan_allocator),
            .asset_projections = try builder.projections.toOwnedSlice(plan_allocator),
            .total_download_size = builder.total_download_size,
            .java_major_version = if (version.javaVersion) |java| java.majorVersion else 0,
        },
    };
}

pub fn rulesAllow(rules: ?[]const metadata.Rule, target: Target) !bool {
    const entries = rules orelse return true;
    if (entries.len == 0) return true;

    var allowed = false;
    for (entries) |rule| {
        if (try ruleMatches(rule, target)) allowed = rule.action == .allow;
    }
    return allowed;
}

fn ruleMatches(rule: metadata.Rule, target: Target) !bool {
    if (rule.os) |os| {
        if (os.name) |name| {
            if (!std.mem.eql(u8, name, @tagName(target.os))) return false;
        }
        if (os.arch) |arch| {
            if (!architectureMatches(arch, target.arch)) return false;
        }
        if (os.version != null or os.versionRange != null) {
            if (target.os_version == null) return false;
            return error.UnsupportedOsVersionRule;
        }
    }

    if (rule.features) |features| {
        var iterator = features.map.iterator();
        while (iterator.next()) |entry| {
            const actual = false;
            if (actual != entry.value_ptr.*) return false;
        }
    }
    return true;
}

fn architectureMatches(pattern: []const u8, architecture: Architecture) bool {
    return switch (architecture) {
        .x86 => std.mem.eql(u8, pattern, "x86") or std.mem.eql(u8, pattern, "i386"),
        .x86_64 => std.mem.eql(u8, pattern, "x86_64") or std.mem.eql(u8, pattern, "amd64"),
        .arm => std.mem.eql(u8, pattern, "arm"),
        .arm64 => std.mem.eql(u8, pattern, "arm64") or std.mem.eql(u8, pattern, "aarch64"),
    };
}

fn validateDownload(url: []const u8, sha1: []const u8) !void {
    if (!std.mem.startsWith(u8, url, "https://")) return error.InsecureDownloadUrl;
    if (sha1.len != 40) return error.InvalidSha1;
    for (sha1) |character| {
        if (!std.ascii.isHex(character)) return error.InvalidSha1;
    }
}

pub fn validateRelativePath(path: []const u8) !void {
    if (path.len == 0 or path[0] == '/' or path[0] == '\\') return error.UnsafePath;
    if (std.mem.indexOfScalar(u8, path, '\\') != null) return error.UnsafePath;
    var components = std.mem.splitScalar(u8, path, '/');
    while (components.next()) |component| try validateComponent(component);
}

fn validateComponent(component: []const u8) !void {
    if (component.len == 0 or std.mem.eql(u8, component, ".") or std.mem.eql(u8, component, "..")) {
        return error.UnsafePath;
    }
    if (component[component.len - 1] == '.' or component[component.len - 1] == ' ') return error.UnsafePath;
    for (component) |character| {
        if (character < 0x20 or character == 0x7f or character == '/' or character == '\\' or character == ':') {
            return error.UnsafePath;
        }
    }

    const stem = component[0 .. std.mem.indexOfScalar(u8, component, '.') orelse component.len];
    const reserved = [_][]const u8{
        "CON",  "PRN",  "AUX",  "NUL",  "COM1", "COM2", "COM3", "COM4", "COM5",
        "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5",
        "LPT6", "LPT7", "LPT8", "LPT9",
    };
    for (reserved) |name| {
        if (std.ascii.eqlIgnoreCase(stem, name)) return error.UnsafePath;
    }
}

fn validateArchivePrefix(prefix: []const u8) !void {
    if (prefix.len == 0 or prefix[0] == '/' or prefix[0] == '\\') return error.UnsafeArchivePath;
    if (std.mem.indexOf(u8, prefix, "..") != null or std.mem.indexOfScalar(u8, prefix, '\\') != null) {
        return error.UnsafeArchivePath;
    }
}

test "builds a mirrored plan with classpath and legacy natives" {
    const source =
        \\{
        \\  "id":"1.0",
        \\  "type":"release",
        \\  "mainClass":"example.Main",
        \\  "downloads":{"client":{"sha1":"1111111111111111111111111111111111111111","size":10,"url":"https://piston-data.mojang.com/v1/objects/client/client.jar"}},
        \\  "assetIndex":{"id":"legacy","sha1":"2222222222222222222222222222222222222222","size":20,"totalSize":20,"url":"https://piston-meta.mojang.com/v1/packages/assets/legacy.json"},
        \\  "libraries":[
        \\    {"name":"example:common:1.0","downloads":{"artifact":{"path":"example/common/1.0/common.jar","sha1":"3333333333333333333333333333333333333333","size":30,"url":"https://libraries.minecraft.net/example/common/1.0/common.jar"}}},
        \\    {"name":"example:native:1.0","downloads":{"classifiers":{"natives-osx-64":{"path":"example/native/1.0/native.jar","sha1":"4444444444444444444444444444444444444444","size":40,"url":"https://libraries.minecraft.net/example/native/1.0/native.jar"}}},"natives":{"osx":"natives-osx-${arch}"},"extract":{"exclude":["META-INF/"]}}
        \\  ]
        \\}
    ;
    var parsed = try metadata.parse(std.testing.allocator, source);
    defer parsed.deinit();

    const asset_source =
        \\{
        \\  "objects": {
        \\    "sounds/example.ogg": {
        \\      "hash": "5555555555555555555555555555555555555555",
        \\      "size": 50
        \\    },
        \\    "sounds/duplicate.ogg": {
        \\      "hash": "5555555555555555555555555555555555555555",
        \\      "size": 50
        \\    }
        \\  },
        \\  "virtual": true
        \\}
    ;
    var parsed_assets = try asset_index.parse(std.testing.allocator, asset_source);
    defer parsed_assets.deinit();

    var plan = try build(
        std.testing.allocator,
        parsed.value,
        parsed_assets.value,
        .bmclapi,
        .{ .os = .osx, .arch = .arm64 },
    );
    defer plan.deinit();

    try std.testing.expectEqual(@as(usize, 5), plan.value.files.len);
    try std.testing.expectEqual(@as(usize, 2), plan.value.classpath.len);
    try std.testing.expectEqualStrings("versions/1.0/1.0.jar", plan.value.classpath[1]);
    try std.testing.expectEqual(@as(usize, 1), plan.value.native_extractions.len);
    try std.testing.expectEqualStrings("META-INF/", plan.value.native_extractions[0].exclude[0]);
    try std.testing.expectEqualStrings(
        "https://bmclapi2.bangbang93.com/maven/example/common/1.0/common.jar",
        plan.value.files[3].url,
    );
    try std.testing.expectEqualStrings(
        "https://libraries.minecraft.net/example/common/1.0/common.jar",
        plan.value.files[3].fallback_url.?,
    );
    try std.testing.expectEqual(@as(usize, 2), plan.value.asset_projections.len);
    try std.testing.expectEqualStrings(
        "assets/virtual/legacy/sounds/example.ogg",
        plan.value.asset_projections[0].destination_path,
    );
    try std.testing.expectEqualStrings(
        "https://bmclapi2.bangbang93.com/assets/55/5555555555555555555555555555555555555555",
        plan.value.files[2].url,
    );
    try std.testing.expectEqualStrings(
        "https://resources.download.minecraft.net/55/5555555555555555555555555555555555555555",
        plan.value.files[2].fallback_url.?,
    );
    try std.testing.expectEqual(@as(u64, 150), plan.value.total_download_size);
}

test "builds a verified dedicated server plan" {
    const source =
        \\{
        \\  "id":"1.21.4",
        \\  "type":"release",
        \\  "mainClass":"net.minecraft.server.Main",
        \\  "downloads":{"client":{"sha1":"2222222222222222222222222222222222222222","size":84,"url":"https://piston-data.mojang.com/v1/objects/client/client.jar"},"server":{"sha1":"1111111111111111111111111111111111111111","size":42,"url":"https://piston-data.mojang.com/v1/objects/server/server.jar"}},
        \\  "javaVersion":{"component":"java-runtime-delta","majorVersion":21},
        \\  "libraries":[]
        \\}
    ;
    var parsed = try metadata.parse(std.testing.allocator, source);
    defer parsed.deinit();
    var plan = try buildServer(
        std.testing.allocator,
        parsed.value,
        .bmclapi,
        .{ .os = .linux, .arch = .arm64 },
    );
    defer plan.deinit();
    try std.testing.expectEqualStrings("1.21.4", plan.value.version_id);
    try std.testing.expectEqual(DownloadSource.bmclapi, plan.value.source);
    try std.testing.expectEqual(@as(usize, 1), plan.value.files.len);
    try std.testing.expectEqualStrings("server.jar", plan.value.files[0].path);
    try std.testing.expect(plan.value.files[0].usage.server);
    try std.testing.expectEqual(@as(u64, 42), plan.value.total_download_size);
    try std.testing.expectEqual(@as(u16, 21), plan.value.java_major_version);
    try std.testing.expectEqualStrings(
        "https://piston-data.mojang.com/v1/objects/server/server.jar",
        plan.value.files[0].fallback_url.?,
    );
}

test "dedicated server plans default legacy metadata to Java 8" {
    const source =
        \\{
        \\  "id":"1.12.2",
        \\  "type":"release",
        \\  "mainClass":"net.minecraft.server.MinecraftServer",
        \\  "downloads":{"client":{"sha1":"2222222222222222222222222222222222222222","size":84,"url":"https://piston-data.mojang.com/v1/objects/client/client.jar"},"server":{"sha1":"1111111111111111111111111111111111111111","size":42,"url":"https://piston-data.mojang.com/v1/objects/server/server.jar"}},
        \\  "libraries":[]
        \\}
    ;
    var parsed = try metadata.parse(std.testing.allocator, source);
    defer parsed.deinit();
    var plan = try buildServer(
        std.testing.allocator,
        parsed.value,
        .official,
        .{ .os = .linux, .arch = .x86_64 },
    );
    defer plan.deinit();
    try std.testing.expectEqual(@as(u16, 8), plan.value.java_major_version);
}

test "rejects metadata without a dedicated server download" {
    const source =
        \\{
        \\  "id":"1.21.4",
        \\  "type":"release",
        \\  "mainClass":"net.minecraft.client.Main",
        \\  "downloads":{"client":{"sha1":"2222222222222222222222222222222222222222","size":84,"url":"https://piston-data.mojang.com/v1/objects/client/client.jar"}},
        \\  "libraries":[]
        \\}
    ;
    var parsed = try metadata.parse(std.testing.allocator, source);
    defer parsed.deinit();
    try std.testing.expectError(
        error.ServerDownloadMissing,
        buildServer(
            std.testing.allocator,
            parsed.value,
            .official,
            .{ .os = .linux, .arch = .x86_64 },
        ),
    );
}

test "library rules use last matching action" {
    const rules = [_]metadata.Rule{
        .{ .action = .allow },
        .{ .action = .disallow, .os = .{ .name = "osx" } },
    };
    try std.testing.expect(!try rulesAllow(&rules, .{ .os = .osx, .arch = .arm64 }));
    try std.testing.expect(try rulesAllow(&rules, .{ .os = .linux, .arch = .x86_64 }));
    try std.testing.expect(try rulesAllow(&.{}, .{ .os = .windows, .arch = .x86_64 }));
}

test "rejects unsafe metadata paths" {
    try std.testing.expectError(error.UnsafePath, validateRelativePath("libraries/../escape.jar"));
    try std.testing.expectError(error.UnsafePath, validateRelativePath("libraries\\escape.jar"));
    try std.testing.expectError(error.InvalidSha1, validateDownload("https://example.invalid/file", "short"));
    try std.testing.expectError(error.InsecureDownloadUrl, validateDownload(
        "http://example.invalid/file",
        "1111111111111111111111111111111111111111",
    ));
}
