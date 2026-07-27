const std = @import("std");
const version_metadata = @import("../install/version_metadata.zig");

pub const max_profile_bytes: usize = 4 * 1024 * 1024;
pub const max_profile_libraries: usize = 1_024;
pub const max_library_size: u64 = 256 * 1024 * 1024;

pub const Library = struct {
    name: []const u8,
    url: []const u8,
    sha1: ?[]const u8 = null,
    size: ?u64 = null,
};

pub const Profile = struct {
    id: []const u8,
    inheritsFrom: []const u8,
    mainClass: []const u8,
    libraries: []const Library,
    arguments: ?version_metadata.Arguments = null,
    releaseTime: ?[]const u8 = null,
    time: ?[]const u8 = null,
    type: ?[]const u8 = null,
};

pub const LibraryIntegrity = struct {
    sha1: []const u8,
    size: u64,
};

pub const Overlay = struct {
    arena: *std.heap.ArenaAllocator,
    value: version_metadata.Metadata,

    pub fn deinit(overlay: *Overlay) void {
        const allocator = overlay.arena.child_allocator;
        overlay.arena.deinit();
        allocator.destroy(overlay.arena);
        overlay.* = undefined;
    }
};

pub fn parse(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Profile) {
    if (source.len > max_profile_bytes) return error.ProfileTooLarge;

    var document = try std.json.parseFromSlice(Profile, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_profile_bytes,
    });
    errdefer document.deinit();
    try validate(document.value);
    return document;
}

pub fn validate(profile: Profile) !void {
    try validateProfileId(profile.id);
    try validateProfileId(profile.inheritsFrom);
    if (profile.mainClass.len == 0 or profile.mainClass.len > 512) return error.InvalidMainClass;
    for (profile.mainClass) |byte| {
        if (!isVisibleAscii(byte) or byte == '/' or byte == '\\') return error.InvalidMainClass;
    }

    if (profile.libraries.len > max_profile_libraries) return error.TooManyLibraries;
    for (profile.libraries) |library| try validateLibrary(library);
}

pub fn merge(
    allocator: std.mem.Allocator,
    parent: version_metadata.Metadata,
    profile: Profile,
    integrities: []const LibraryIntegrity,
) !Overlay {
    if (integrities.len != profile.libraries.len) return error.LibraryIntegrityCountMismatch;
    try validate(profile);
    for (integrities) |integrity| {
        try validateSha1(integrity.sha1);
        if (integrity.size == 0 or integrity.size > max_library_size) return error.InvalidLibrarySize;
    }

    const arena = try allocator.create(std.heap.ArenaAllocator);
    errdefer allocator.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(allocator);
    errdefer arena.deinit();
    const output = arena.allocator();

    const libraries = try output.alloc(version_metadata.Library, profile.libraries.len + parent.libraries.len);
    for (profile.libraries, integrities, 0..) |library, integrity, index| {
        const path = try artifactPathAlloc(output, library.name);
        const url = try artifactUrlAlloc(output, library.url, library.name);
        libraries[index] = .{
            .name = library.name,
            .url = library.url,
            .downloads = .{ .artifact = .{
                .path = path,
                .sha1 = try output.dupe(u8, integrity.sha1),
                .size = integrity.size,
                .url = url,
            } },
        };
    }
    @memcpy(libraries[profile.libraries.len..], parent.libraries);

    var value = parent;
    value.id = profile.id;
    value.type = profile.type orelse parent.type;
    value.mainClass = profile.mainClass;
    value.libraries = libraries;
    value.arguments = try mergeArguments(output, parent.arguments, profile.arguments);
    value.releaseTime = profile.releaseTime orelse parent.releaseTime;
    value.time = profile.time orelse parent.time;
    value.inheritsFrom = null;
    return .{ .arena = arena, .value = value };
}

pub fn validateLibrary(library: Library) !void {
    _ = try parseMavenCoordinate(library.name);
    try validateRepositoryUrl(library.url);
    if (library.sha1) |sha1| try validateSha1(sha1);
    if (library.size) |size| {
        if (size == 0 or size > max_library_size) return error.InvalidLibrarySize;
    }
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
    const output: ?[]const version_metadata.Argument = try appendArguments(
        allocator,
        first orelse &.{},
        second orelse &.{},
    );
    return output;
}

pub fn artifactPathAlloc(allocator: std.mem.Allocator, coordinate: []const u8) ![]u8 {
    const parsed = try parseMavenCoordinate(coordinate);
    var output: std.Io.Writer.Allocating = .init(allocator);
    errdefer output.deinit();
    for (parsed.group) |byte| try output.writer.writeByte(if (byte == '.') '/' else byte);
    try output.writer.print("/{s}/{s}/{s}-{s}", .{
        parsed.artifact,
        parsed.version,
        parsed.artifact,
        parsed.version,
    });
    if (parsed.classifier) |value| try output.writer.print("-{s}", .{value});
    try output.writer.writeAll(".jar");
    return output.toOwnedSlice();
}

const MavenCoordinate = struct {
    group: []const u8,
    artifact: []const u8,
    version: []const u8,
    classifier: ?[]const u8,
};

fn parseMavenCoordinate(coordinate: []const u8) !MavenCoordinate {
    var components = std.mem.splitScalar(u8, coordinate, ':');
    const group = components.next() orelse return error.InvalidMavenCoordinate;
    const artifact = components.next() orelse return error.InvalidMavenCoordinate;
    const version = components.next() orelse return error.InvalidMavenCoordinate;
    const classifier = components.next();
    if (components.next() != null) return error.InvalidMavenCoordinate;

    try validateGroup(group);
    try validateMavenComponent(artifact);
    try validateMavenComponent(version);
    if (classifier) |value| try validateMavenComponent(value);
    return .{
        .group = group,
        .artifact = artifact,
        .version = version,
        .classifier = classifier,
    };
}

pub fn artifactUrlAlloc(
    allocator: std.mem.Allocator,
    repository_url: []const u8,
    coordinate: []const u8,
) ![]u8 {
    try validateRepositoryUrl(repository_url);
    const path = try artifactPathAlloc(allocator, coordinate);
    defer allocator.free(path);
    return std.fmt.allocPrint(allocator, "{s}{s}", .{ repository_url, path });
}

pub fn profileUrlAlloc(
    allocator: std.mem.Allocator,
    minecraft_version: []const u8,
    loader_version: []const u8,
) ![]u8 {
    try validateProfileId(minecraft_version);
    try validateProfileId(loader_version);
    return std.fmt.allocPrint(
        allocator,
        "https://meta.fabricmc.net/v2/versions/loader/{s}/{s}/profile/json",
        .{ minecraft_version, loader_version },
    );
}

pub fn serverProfileUrlAlloc(
    allocator: std.mem.Allocator,
    minecraft_version: []const u8,
    loader_version: []const u8,
) ![]u8 {
    try validateProfileId(minecraft_version);
    try validateProfileId(loader_version);
    return std.fmt.allocPrint(
        allocator,
        "https://meta.fabricmc.net/v2/versions/loader/{s}/{s}/server/json",
        .{ minecraft_version, loader_version },
    );
}

pub fn validateServerProfile(profile: Profile, minecraft_version: []const u8) !void {
    try validate(profile);
    if (!std.mem.eql(u8, profile.inheritsFrom, minecraft_version)) return error.FabricParentMismatch;
}

fn validateProfileId(value: []const u8) !void {
    if (value.len == 0 or value.len > 256) return error.InvalidProfileId;
    if (value[value.len - 1] == '.' or value[value.len - 1] == ' ') return error.InvalidProfileId;
    for (value) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '-' and byte != '_' and byte != '+' and byte != '.') {
            return error.InvalidProfileId;
        }
    }
}

fn validateRepositoryUrl(value: []const u8) !void {
    const prefix = "https://";
    if (!std.mem.startsWith(u8, value, prefix) or !std.mem.endsWith(u8, value, "/")) {
        return error.InsecureRepositoryUrl;
    }
    const authority_end = std.mem.indexOfScalarPos(u8, value, prefix.len, '/') orelse value.len;
    if (authority_end == prefix.len) return error.InvalidRepositoryUrl;
    for (value) |byte| {
        if (!isVisibleAscii(byte) or byte == '\\' or byte == '?' or byte == '#' or byte == '@') {
            return error.InvalidRepositoryUrl;
        }
    }
    if (std.mem.indexOf(u8, value[prefix.len..], "..") != null) return error.InvalidRepositoryUrl;
}

pub fn validateSha1(value: []const u8) !void {
    if (value.len != 40) return error.InvalidLibrarySha1;
    for (value) |byte| if (!std.ascii.isHex(byte)) return error.InvalidLibrarySha1;
}

fn validateGroup(value: []const u8) !void {
    var components = std.mem.splitScalar(u8, value, '.');
    while (components.next()) |component| try validateMavenComponent(component);
}

fn validateMavenComponent(value: []const u8) !void {
    if (value.len == 0 or value.len > 256 or std.mem.eql(u8, value, ".") or std.mem.eql(u8, value, "..")) {
        return error.InvalidMavenCoordinate;
    }
    for (value) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '-' and byte != '_' and byte != '+' and byte != '.') {
            return error.InvalidMavenCoordinate;
        }
    }
}

fn isVisibleAscii(byte: u8) bool {
    return byte >= 0x21 and byte <= 0x7e;
}

test "parses and validates a Fabric loader profile" {
    const source =
        \\{
        \\  "id":"fabric-loader-0.16.10-1.21.1",
        \\  "inheritsFrom":"1.21.1",
        \\  "type":"release",
        \\  "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
        \\  "arguments":{"game":[],"jvm":["-DFabricMcEmu= net.minecraft.client.main.Main "]},
        \\  "libraries":[
        \\    {"name":"org.ow2.asm:asm:9.7.1","url":"https://maven.fabricmc.net/","sha1":"f0ed132a49244b042cd0e15702ab9f2ce3cc8436","size":126093},
        \\    {"name":"net.fabricmc:intermediary:1.21.1","url":"https://maven.fabricmc.net/"},
        \\    {"name":"net.fabricmc:fabric-loader:0.16.10","url":"https://maven.fabricmc.net/"}
        \\  ]
        \\}
    ;

    var document = try parse(std.testing.allocator, source);
    defer document.deinit();
    try std.testing.expectEqualStrings("1.21.1", document.value.inheritsFrom);
    try std.testing.expectEqual(@as(usize, 3), document.value.libraries.len);
    try std.testing.expect(document.value.libraries[1].sha1 == null);
    try std.testing.expect(document.value.libraries[1].size == null);
    const loader_path = try artifactPathAlloc(std.testing.allocator, document.value.libraries[2].name);
    defer std.testing.allocator.free(loader_path);
    try std.testing.expectEqualStrings(
        "net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar",
        loader_path,
    );
}

test "merges a Fabric profile over its vanilla parent" {
    const parent_source =
        \\{
        \\  "id":"1.21.1",
        \\  "type":"release",
        \\  "mainClass":"net.minecraft.client.main.Main",
        \\  "arguments":{"game":["--username","${auth_player_name}"],"jvm":["-Dparent=true"]},
        \\  "downloads":{"client":{"sha1":"1111111111111111111111111111111111111111","size":1,"url":"https://example.invalid/client.jar"}},
        \\  "libraries":[{"name":"com.example:parent:1.0","downloads":{"artifact":{"path":"com/example/parent/1.0/parent-1.0.jar","sha1":"2222222222222222222222222222222222222222","size":2,"url":"https://example.invalid/parent.jar"}}}]
        \\}
    ;
    const profile_source =
        \\{
        \\  "id":"fabric-loader-0.16.10-1.21.1",
        \\  "inheritsFrom":"1.21.1",
        \\  "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
        \\  "arguments":{"game":[],"jvm":["-DFabricMcEmu= net.minecraft.client.main.Main "]},
        \\  "libraries":[{"name":"net.fabricmc:fabric-loader:0.16.10","url":"https://maven.fabricmc.net/"}]
        \\}
    ;
    var parent = try version_metadata.parse(std.testing.allocator, parent_source);
    defer parent.deinit();
    var profile = try parse(std.testing.allocator, profile_source);
    defer profile.deinit();
    const integrities = [_]LibraryIntegrity{.{
        .sha1 = "3333333333333333333333333333333333333333",
        .size = 3,
    }};
    var overlay = try merge(std.testing.allocator, parent.value, profile.value, &integrities);
    defer overlay.deinit();

    try std.testing.expectEqualStrings("fabric-loader-0.16.10-1.21.1", overlay.value.id);
    try std.testing.expectEqualStrings("net.fabricmc.loader.impl.launch.knot.KnotClient", overlay.value.mainClass);
    try std.testing.expectEqual(@as(usize, 2), overlay.value.libraries.len);
    try std.testing.expectEqualStrings("net.fabricmc:fabric-loader:0.16.10", overlay.value.libraries[0].name);
    try std.testing.expectEqualStrings(
        "net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar",
        overlay.value.libraries[0].downloads.?.artifact.?.path,
    );
    try std.testing.expectEqual(@as(usize, 2), overlay.value.arguments.?.jvm.len);
    try std.testing.expectEqualStrings("-Dparent=true", overlay.value.arguments.?.jvm[0].literal);
    try std.testing.expectEqualStrings(
        "-DFabricMcEmu= net.minecraft.client.main.Main ",
        overlay.value.arguments.?.jvm[1].literal,
    );
    try std.testing.expect(overlay.value.inheritsFrom == null);
}

test "builds Maven artifact paths and URLs" {
    const allocator = std.testing.allocator;
    const path = try artifactPathAlloc(allocator, "org.lwjgl:lwjgl:3.3.3:natives-windows");
    defer allocator.free(path);
    try std.testing.expectEqualStrings(
        "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar",
        path,
    );

    const url = try artifactUrlAlloc(allocator, "https://maven.fabricmc.net/", "net.fabricmc:fabric-loader:0.16.10");
    defer allocator.free(url);
    try std.testing.expectEqualStrings(
        "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar",
        url,
    );

    const profile_url = try profileUrlAlloc(allocator, "1.21.1", "0.16.10");
    defer allocator.free(profile_url);
    try std.testing.expectEqualStrings(
        "https://meta.fabricmc.net/v2/versions/loader/1.21.1/0.16.10/profile/json",
        profile_url,
    );
}

test "rejects unsafe Fabric profile and Maven inputs" {
    try std.testing.expectError(error.InvalidMavenCoordinate, artifactPathAlloc(std.testing.allocator, "group:artifact:../escape"));
    try std.testing.expectError(error.InvalidMavenCoordinate, artifactPathAlloc(std.testing.allocator, "group:artifact:1.0:bad/path"));
    try std.testing.expectError(error.InsecureRepositoryUrl, artifactUrlAlloc(std.testing.allocator, "http://maven.fabricmc.net/", "net.fabricmc:fabric-loader:0.16.10"));
    try std.testing.expectError(error.InvalidRepositoryUrl, artifactUrlAlloc(std.testing.allocator, "https://maven.fabricmc.net/../", "net.fabricmc:fabric-loader:0.16.10"));
    try std.testing.expectError(error.InvalidLibrarySha1, validateLibrary(.{
        .name = "net.fabricmc:fabric-loader:0.16.10",
        .url = "https://maven.fabricmc.net/",
        .sha1 = "not-a-sha1",
    }));
    try std.testing.expectError(error.InvalidProfileId, validate(.{
        .id = "fabric/escape",
        .inheritsFrom = "1.21.1",
        .mainClass = "net.fabricmc.loader.impl.launch.knot.KnotClient",
        .libraries = &.{},
    }));
}

test "builds a Fabric server profile URL" {
    const allocator = std.testing.allocator;
    const url = try serverProfileUrlAlloc(allocator, "1.21.1", "0.16.10");
    defer allocator.free(url);
    try std.testing.expectEqualStrings(
        "https://meta.fabricmc.net/v2/versions/loader/1.21.1/0.16.10/server/json",
        url,
    );
    try std.testing.expectError(error.InvalidProfileId, serverProfileUrlAlloc(allocator, "bad/id", "0.16.10"));
    try std.testing.expectError(error.InvalidProfileId, serverProfileUrlAlloc(allocator, "1.21.1", ""));
}

const server_profile_fixture =
    \\{
    \\  "id":"fabric-loader-0.16.10-1.21.1",
    \\  "inheritsFrom":"1.21.1",
    \\  "releaseTime":"2024-11-28T13:53:11+00:00",
    \\  "time":"2024-11-28T13:53:11+00:00",
    \\  "type":"release",
    \\  "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotServer",
    \\  "arguments":{"game":[],"jvm":[]},
    \\  "libraries":[
    \\    {"name":"net.fabricmc:tiny-mappings-parser:0.3.0+build.17","url":"https://maven.fabricmc.net/","sha1":"e9dfb8e1b9e3bc0ac2a5a70a13bb9e8b036e0d1b","size":52432},
    \\    {"name":"org.ow2.asm:asm:9.7.1","url":"https://maven.fabricmc.net/","sha1":"f0ed132a49244b042cd0e15702ab9f2ce3cc8436","size":126093},
    \\    {"name":"net.fabricmc:intermediary:1.21.1","url":"https://maven.fabricmc.net/"},
    \\    {"name":"net.fabricmc:fabric-loader:0.16.10","url":"https://maven.fabricmc.net/"}
    \\  ]
    \\}
;

test "parses and validates a Fabric server profile" {
    var document = try parse(std.testing.allocator, server_profile_fixture);
    defer document.deinit();
    const profile = document.value;
    try std.testing.expectEqualStrings("fabric-loader-0.16.10-1.21.1", profile.id);
    try std.testing.expectEqualStrings("1.21.1", profile.inheritsFrom);
    try std.testing.expectEqualStrings("net.fabricmc.loader.impl.launch.knot.KnotServer", profile.mainClass);
    try std.testing.expectEqual(@as(usize, 4), profile.libraries.len);
    try std.testing.expectEqualStrings("net.fabricmc:tiny-mappings-parser:0.3.0+build.17", profile.libraries[0].name);
    try std.testing.expectEqualStrings("e9dfb8e1b9e3bc0ac2a5a70a13bb9e8b036e0d1b", profile.libraries[0].sha1.?);
    try std.testing.expectEqual(@as(u64, 52432), profile.libraries[0].size.?);
    try std.testing.expect(profile.libraries[2].sha1 == null);
    try std.testing.expect(profile.libraries[2].size == null);
    try std.testing.expect(profile.libraries[3].sha1 == null);
    try std.testing.expect(profile.libraries[3].size == null);
    try validateServerProfile(profile, "1.21.1");
}

test "rejects a Fabric server profile with mismatched parent" {
    var document = try parse(std.testing.allocator, server_profile_fixture);
    defer document.deinit();
    try std.testing.expectError(error.FabricParentMismatch, validateServerProfile(document.value, "1.21.2"));
}

test "rejects invalid Fabric server profiles" {
    const empty_main_class =
        \\{
        \\  "id":"fabric-loader-0.16.10-1.21.1",
        \\  "inheritsFrom":"1.21.1",
        \\  "mainClass":"",
        \\  "libraries":[{"name":"net.fabricmc:fabric-loader:0.16.10","url":"https://maven.fabricmc.net/"}]
        \\}
    ;
    try std.testing.expectError(error.InvalidMainClass, parse(std.testing.allocator, empty_main_class));

    const malformed_coordinate =
        \\{
        \\  "id":"fabric-loader-0.16.10-1.21.1",
        \\  "inheritsFrom":"1.21.1",
        \\  "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotServer",
        \\  "libraries":[{"name":"net.fabricmc:fabric-loader","url":"https://maven.fabricmc.net/"}]
        \\}
    ;
    try std.testing.expectError(error.InvalidMavenCoordinate, parse(std.testing.allocator, malformed_coordinate));
}
