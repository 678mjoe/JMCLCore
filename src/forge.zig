const std = @import("std");
const neoforge = @import("neoforge.zig");
const version_metadata = @import("version_metadata.zig");

pub const maven_root = "https://maven.minecraftforge.net/";

pub const InstallProfile = neoforge.InstallProfile;
pub const VersionProfile = neoforge.VersionProfile;
pub const Overlay = neoforge.Overlay;
pub const parseInstallProfile = neoforge.parseInstallProfile;

pub fn validateForgeVersion(value: []const u8) !void {
    neoforge.validateVersionId(value) catch return error.InvalidForgeVersion;
}

pub fn installerUrlAlloc(
    allocator: std.mem.Allocator,
    minecraft_version: []const u8,
    forge_version: []const u8,
) ![]u8 {
    try neoforge.validateVersionId(minecraft_version);
    try validateForgeVersion(forge_version);
    return std.fmt.allocPrint(
        allocator,
        maven_root ++ "net/minecraftforge/forge/{s}-{s}/forge-{s}-{s}-installer.jar",
        .{ minecraft_version, forge_version, minecraft_version, forge_version },
    );
}

pub fn profileIdAlloc(
    allocator: std.mem.Allocator,
    minecraft_version: []const u8,
    forge_version: []const u8,
) ![]u8 {
    return std.fmt.allocPrint(allocator, "{s}-forge-{s}", .{ minecraft_version, forge_version });
}

pub fn supportDirAlloc(
    allocator: std.mem.Allocator,
    minecraft_version: []const u8,
    forge_version: []const u8,
) ![]u8 {
    try neoforge.validateVersionId(minecraft_version);
    try validateForgeVersion(forge_version);
    return std.fmt.allocPrint(allocator, ".jmcl/forge/{s}-{s}", .{ minecraft_version, forge_version });
}

pub fn parseVersionProfile(
    allocator: std.mem.Allocator,
    source: []const u8,
    patched_coordinate: []const u8,
) !std.json.Parsed(VersionProfile) {
    if (source.len > neoforge.max_version_profile_bytes) return error.VersionProfileTooLarge;
    var document = try std.json.parseFromSlice(VersionProfile, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = neoforge.max_version_profile_bytes,
    });
    errdefer document.deinit();
    try validateVersionProfile(allocator, document.value, patched_coordinate);
    return document;
}

fn validateVersionProfile(
    allocator: std.mem.Allocator,
    profile: VersionProfile,
    patched_coordinate: []const u8,
) !void {
    try neoforge.validateVersionId(profile.id);
    try neoforge.validateVersionId(profile.inheritsFrom);
    if (profile.mainClass.len == 0 or profile.mainClass.len > 512) return error.InvalidMainClass;
    for (profile.mainClass) |byte| {
        if (byte <= 0x20 or byte >= 0x7f or byte == '/' or byte == '\\') return error.InvalidMainClass;
    }
    if (profile.libraries.len > neoforge.max_profile_libraries) return error.TooManyLibraries;

    const patched_path = try neoforge.artifactPathAlloc(allocator, patched_coordinate);
    defer allocator.free(patched_path);
    for (profile.libraries) |library| {
        const coordinate = try neoforge.parseCoordinate(library.name);
        if (!std.mem.eql(u8, coordinate.extension, "jar")) return error.UnsupportedLibraryExtension;
        const downloads = library.downloads orelse return error.MissingLibraryDownloads;
        const artifact = downloads.artifact orelse return error.MissingLibraryArtifact;
        try neoforge.validateSha1(artifact.sha1);
        if (artifact.size == 0 or artifact.size > neoforge.max_library_size) return error.InvalidLibrarySize;
        if (artifact.url.len == 0) {
            const library_path = try neoforge.artifactPathAlloc(allocator, library.name);
            defer allocator.free(library_path);
            if (!std.mem.eql(u8, library_path, patched_path)) return error.MissingLibraryArtifactUrl;
            continue;
        }
        try validateArtifactUrl(artifact.url);
    }
}

fn validateArtifactUrl(value: []const u8) !void {
    const prefix = "https://";
    if (!std.mem.startsWith(u8, value, prefix)) return error.InsecureArtifactUrl;
    const authority_end = std.mem.indexOfScalarPos(u8, value, prefix.len, '/') orelse value.len;
    if (authority_end == prefix.len) return error.InvalidArtifactUrl;
    for (value) |byte| {
        if (byte <= 0x20 or byte >= 0x7f or byte == '\\' or byte == '?' or byte == '#' or byte == '@') {
            return error.InvalidArtifactUrl;
        }
    }
    if (std.mem.indexOf(u8, value[prefix.len..], "..") != null) return error.InvalidArtifactUrl;
}

/// Returns the declared SHA-1 of the patched client when the install profile
/// publishes one, so the processor output can be verified against it.
pub fn expectedPatchedSha1(profile: InstallProfile) ?[]const u8 {
    const entry = profile.data.map.get("PATCHED_SHA") orelse return null;
    const value = neoforge.dataValueForSide(entry, .client);
    const literal = switch (neoforge.classifyDataValue(value)) {
        .literal => |literal| literal,
        else => return null,
    };
    neoforge.validateSha1(literal) catch return null;
    return literal;
}

pub fn merge(
    allocator: std.mem.Allocator,
    parent: version_metadata.Metadata,
    profile: VersionProfile,
    patched_coordinate: []const u8,
    patched_client_path: []const u8,
) !Overlay {
    try validateVersionProfile(allocator, profile, patched_coordinate);
    try neoforge.validateRelativePath(patched_client_path);

    const arena = try allocator.create(std.heap.ArenaAllocator);
    errdefer allocator.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(allocator);
    errdefer arena.deinit();
    const output = arena.allocator();

    const patched_path = try neoforge.artifactPathAlloc(output, patched_coordinate);
    var kept: usize = 0;
    for (profile.libraries) |library| {
        const library_path = try neoforge.artifactPathAlloc(output, library.name);
        if (std.mem.eql(u8, library_path, patched_path)) continue;
        kept += 1;
    }

    const libraries = try output.alloc(version_metadata.Library, kept + parent.libraries.len);
    var index: usize = 0;
    for (profile.libraries) |library| {
        const artifact = library.downloads.?.artifact.?;
        const path = try neoforge.artifactPathAlloc(output, library.name);
        if (std.mem.eql(u8, path, patched_path)) continue;
        libraries[index] = .{
            .name = library.name,
            .rules = library.rules,
            .natives = library.natives,
            .extract = library.extract,
            .downloads = .{ .artifact = .{
                .path = path,
                .sha1 = artifact.sha1,
                .size = artifact.size,
                .url = artifact.url,
            } },
        };
        index += 1;
    }
    @memcpy(libraries[kept..], parent.libraries);

    var value = parent;
    value.id = profile.id;
    value.type = profile.type orelse parent.type;
    value.mainClass = profile.mainClass;
    value.libraries = libraries;
    value.arguments = try mergeArguments(output, parent.arguments, profile.arguments);
    value.releaseTime = profile.releaseTime orelse parent.releaseTime;
    value.time = profile.time orelse parent.time;
    value.inheritsFrom = null;
    value.patched_client = try output.dupe(u8, patched_client_path);
    return .{ .arena = arena, .value = value };
}

fn mergeArguments(
    allocator: std.mem.Allocator,
    parent: ?version_metadata.Arguments,
    profile: ?version_metadata.Arguments,
) !?version_metadata.Arguments {
    if (parent == null and profile == null) return null;
    const empty: version_metadata.Arguments = .{ .game = &.{}, .jvm = &.{} };
    const parent_value = parent orelse empty;
    const profile_value = profile orelse empty;
    return .{
        .game = try appendArguments(allocator, parent_value.game, profile_value.game),
        .jvm = try appendArguments(allocator, parent_value.jvm, profile_value.jvm),
        .@"default-user-jvm" = try appendOptionalArguments(
            allocator,
            parent_value.@"default-user-jvm",
            profile_value.@"default-user-jvm",
        ),
    };
}

fn appendArguments(
    allocator: std.mem.Allocator,
    first: []const version_metadata.Argument,
    second: []const version_metadata.Argument,
) ![]const version_metadata.Argument {
    const output = try allocator.alloc(version_metadata.Argument, first.len + second.len);
    @memcpy(output[0..first.len], first);
    @memcpy(output[first.len..], second);
    return output;
}

fn appendOptionalArguments(
    allocator: std.mem.Allocator,
    first: ?[]const version_metadata.Argument,
    second: ?[]const version_metadata.Argument,
) !?[]const version_metadata.Argument {
    if (first == null and second == null) return null;
    const merged: ?[]const version_metadata.Argument = try appendArguments(allocator, first orelse &.{}, second orelse &.{});
    return merged;
}

test "builds installer URLs and profile ids from both version components" {
    const allocator = std.testing.allocator;
    const url = try installerUrlAlloc(allocator, "1.20.4", "49.2.8");
    defer allocator.free(url);
    try std.testing.expectEqualStrings(
        "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.4-49.2.8/forge-1.20.4-49.2.8-installer.jar",
        url,
    );

    const id = try profileIdAlloc(allocator, "1.20.4", "49.2.8");
    defer allocator.free(id);
    try std.testing.expectEqualStrings("1.20.4-forge-49.2.8", id);

    const support = try supportDirAlloc(allocator, "1.20.4", "49.2.8");
    defer allocator.free(support);
    try std.testing.expectEqualStrings(".jmcl/forge/1.20.4-49.2.8", support);

    try std.testing.expectError(error.InvalidForgeVersion, installerUrlAlloc(allocator, "1.20.4", "bad version"));
    try std.testing.expectError(error.InvalidNeoForgeVersion, installerUrlAlloc(allocator, "bad version", "49.2.8"));
}

test "extracts the declared patched client hash" {
    const source =
        \\{
        \\  "spec":1,
        \\  "version":"1.20.4-forge-49.2.8",
        \\  "minecraft":"1.20.4",
        \\  "data":{
        \\    "PATCHED":{"client":"[net.minecraftforge:forge:1.20.4-49.2.8:client]","server":"[net.minecraftforge:forge:1.20.4-49.2.8:server]"},
        \\    "PATCHED_SHA":{"client":"'9a32f7ac703db4fc1374c215b087a95c0637a1f7'","server":"'f90a8b7496366a80967d92b253a7fef0e9adeddf'"}
        \\  },
        \\  "processors":[],
        \\  "libraries":[]
        \\}
    ;
    var profile = try parseInstallProfile(std.testing.allocator, source);
    defer profile.deinit();
    try std.testing.expectEqualStrings(
        "9a32f7ac703db4fc1374c215b087a95c0637a1f7",
        expectedPatchedSha1(profile.value).?,
    );
    try std.testing.expectEqualStrings(
        "net.minecraftforge:forge:1.20.4-49.2.8:client",
        try neoforge.patchedClientCoordinate(profile.value),
    );
}

test "accepts the patched client library with an empty download url" {
    const allocator = std.testing.allocator;
    const source =
        \\{
        \\  "id":"1.20.4-forge-49.2.8",
        \\  "inheritsFrom":"1.20.4",
        \\  "mainClass":"net.minecraftforge.bootstrap.ForgeBootstrap",
        \\  "libraries":[
        \\    {"name":"net.minecraftforge:forge:1.20.4-49.2.8:client","downloads":{"artifact":{"path":"net/minecraftforge/forge/1.20.4-49.2.8/forge-1.20.4-49.2.8-client.jar","url":"","sha1":"9a32f7ac703db4fc1374c215b087a95c0637a1f7","size":26348985}}},
        \\    {"name":"net.minecraftforge:forge:1.20.4-49.2.8:universal","downloads":{"artifact":{"path":"net/minecraftforge/forge/1.20.4-49.2.8/forge-1.20.4-49.2.8-universal.jar","url":"https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.4-49.2.8/forge-1.20.4-49.2.8-universal.jar","sha1":"0fda80d05674c2f4d08f37752e4e9434e3f3858c","size":1535330}}}
        \\  ]
        \\}
    ;
    var profile = try parseVersionProfile(
        allocator,
        source,
        "net.minecraftforge:forge:1.20.4-49.2.8:client",
    );
    defer profile.deinit();
    try std.testing.expectEqual(@as(usize, 2), profile.value.libraries.len);
}

test "rejects empty download urls on libraries that are not the patched client" {
    const allocator = std.testing.allocator;
    const source =
        \\{
        \\  "id":"1.20.4-forge-49.2.8",
        \\  "inheritsFrom":"1.20.4",
        \\  "mainClass":"net.minecraftforge.bootstrap.ForgeBootstrap",
        \\  "libraries":[
        \\    {"name":"net.minecraftforge:eventbus:6.2.33","downloads":{"artifact":{"path":"net/minecraftforge/eventbus/6.2.33/eventbus-6.2.33.jar","url":"","sha1":"3fae69cfa9c50d1e446a6193f84454e0480b1dd6","size":56199}}}
        \\  ]
        \\}
    ;
    try std.testing.expectError(error.MissingLibraryArtifactUrl, parseVersionProfile(
        allocator,
        source,
        "net.minecraftforge:forge:1.20.4-49.2.8:client",
    ));
}

test "merges a Forge version profile over its vanilla parent without the patched library" {
    const allocator = std.testing.allocator;
    const parent_source =
        \\{
        \\  "id":"1.20.4",
        \\  "type":"release",
        \\  "mainClass":"net.minecraft.client.main.Main",
        \\  "arguments":{"game":["--username","${auth_player_name}"],"jvm":["-Dparent=true"]},
        \\  "downloads":{"client":{"sha1":"1111111111111111111111111111111111111111","size":1,"url":"https://example.invalid/client.jar"}},
        \\  "libraries":[{"name":"com.example:parent:1.0","downloads":{"artifact":{"path":"com/example/parent/1.0/parent-1.0.jar","sha1":"2222222222222222222222222222222222222222","size":2,"url":"https://example.invalid/parent.jar"}}}],
        \\  "logging":{"client":{"argument":"-Dlog4j.configurationFile=${path}","file":{"id":"client-1.12.xml","sha1":"3333333333333333333333333333333333333333","size":3,"url":"https://example.invalid/log.xml"},"type":"log4j2-xml"}}
        \\}
    ;
    const profile_source =
        \\{
        \\  "id":"1.20.4-forge-49.2.8",
        \\  "inheritsFrom":"1.20.4",
        \\  "type":"release",
        \\  "mainClass":"net.minecraftforge.bootstrap.ForgeBootstrap",
        \\  "arguments":{"game":["--launchTarget","forge_client"],"jvm":["-DlibraryDirectory=${library_directory}"]},
        \\  "libraries":[
        \\    {"name":"net.minecraftforge:forge:1.20.4-49.2.8:client","downloads":{"artifact":{"path":"net/minecraftforge/forge/1.20.4-49.2.8/forge-1.20.4-49.2.8-client.jar","url":"","sha1":"9a32f7ac703db4fc1374c215b087a95c0637a1f7","size":26348985}}},
        \\    {"name":"net.minecraftforge:eventbus:6.2.33","downloads":{"artifact":{"path":"net/minecraftforge/eventbus/6.2.33/eventbus-6.2.33.jar","url":"https://maven.minecraftforge.net/net/minecraftforge/eventbus/6.2.33/eventbus-6.2.33.jar","sha1":"3fae69cfa9c50d1e446a6193f84454e0480b1dd6","size":56199}}}
        \\  ],
        \\  "logging":{}
        \\}
    ;
    var parent = try version_metadata.parse(allocator, parent_source);
    defer parent.deinit();
    var profile = try parseVersionProfile(
        allocator,
        profile_source,
        "net.minecraftforge:forge:1.20.4-49.2.8:client",
    );
    defer profile.deinit();

    var overlay = try merge(
        allocator,
        parent.value,
        profile.value,
        "net.minecraftforge:forge:1.20.4-49.2.8:client",
        "libraries/net/minecraftforge/forge/1.20.4-49.2.8/forge-1.20.4-49.2.8-client.jar",
    );
    defer overlay.deinit();

    try std.testing.expectEqualStrings("1.20.4-forge-49.2.8", overlay.value.id);
    try std.testing.expectEqualStrings("net.minecraftforge.bootstrap.ForgeBootstrap", overlay.value.mainClass);
    try std.testing.expectEqual(@as(usize, 2), overlay.value.libraries.len);
    try std.testing.expectEqualStrings(
        "net/minecraftforge/eventbus/6.2.33/eventbus-6.2.33.jar",
        overlay.value.libraries[0].downloads.?.artifact.?.path,
    );
    try std.testing.expectEqualStrings("com.example:parent:1.0", overlay.value.libraries[1].name);
    try std.testing.expectEqual(@as(usize, 4), overlay.value.arguments.?.game.len);
    try std.testing.expectEqual(@as(usize, 2), overlay.value.arguments.?.jvm.len);
    try std.testing.expectEqualStrings("forge_client", overlay.value.arguments.?.game[3].literal);
    try std.testing.expect(overlay.value.logging != null);
    try std.testing.expectEqualStrings(
        "libraries/net/minecraftforge/forge/1.20.4-49.2.8/forge-1.20.4-49.2.8-client.jar",
        overlay.value.patched_client.?,
    );
    try std.testing.expect(overlay.value.inheritsFrom == null);
}
