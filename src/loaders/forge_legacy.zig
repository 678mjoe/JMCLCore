const std = @import("std");
const neoforge = @import("neoforge.zig");
const version_metadata = @import("../install/version_metadata.zig");

pub const max_profile_bytes: usize = 4 * 1024 * 1024;
pub const max_libraries: usize = 1_024;
pub const max_checksums: usize = 16;
pub const max_arguments_bytes: usize = 16 * 1024;
pub const max_library_size: u64 = neoforge.max_library_size;

pub const default_repository = "https://libraries.minecraft.net/";

pub const Library = struct {
    name: []const u8,
    url: ?[]const u8 = null,
    checksums: ?[]const []const u8 = null,
    serverreq: bool = false,
    clientreq: bool = false,
};

pub const InstallSection = struct {
    path: []const u8,
    minecraft: []const u8,
    filePath: ?[]const u8 = null,
    target: ?[]const u8 = null,
    version: ?[]const u8 = null,
    profileName: ?[]const u8 = null,
};

pub const VersionInfo = struct {
    id: []const u8,
    mainClass: []const u8,
    minecraftArguments: []const u8,
    type: ?[]const u8 = null,
    time: ?[]const u8 = null,
    releaseTime: ?[]const u8 = null,
    assets: ?[]const u8 = null,
    inheritsFrom: ?[]const u8 = null,
    jar: ?[]const u8 = null,
    minimumLauncherVersion: ?u32 = null,
    libraries: []const Library = &.{},
};

pub const Profile = struct {
    install: InstallSection,
    versionInfo: VersionInfo,
};

pub const ResolvedLibrary = struct {
    path: []const u8,
    url: []const u8,
    sha1: []const u8,
    size: u64,
};

const RepositoryUpgrade = struct { insecure: []const u8, secure: []const u8 };
const trusted_repository_upgrades = [_]RepositoryUpgrade{
    .{ .insecure = "http://files.minecraftforge.net/maven/", .secure = "https://maven.minecraftforge.net/" },
    .{ .insecure = "http://maven.minecraftforge.net/", .secure = "https://maven.minecraftforge.net/" },
    .{ .insecure = "http://libraries.minecraft.net/", .secure = "https://libraries.minecraft.net/" },
    .{ .insecure = "http://repo1.maven.org/maven2/", .secure = "https://repo1.maven.org/maven2/" },
    .{ .insecure = "http://maven-central.storage-download.googleapis.com/maven2/", .secure = "https://maven-central.storage-download.googleapis.com/maven2/" },
};

const Probe = struct {
    versionInfo: ?std.json.Value = null,
};

pub fn isLegacyProfile(allocator: std.mem.Allocator, source: []const u8) !bool {
    if (source.len > neoforge.max_install_profile_bytes) return error.InstallProfileTooLarge;
    var probe = try std.json.parseFromSlice(Probe, allocator, source, .{
        .ignore_unknown_fields = true,
        .max_value_len = neoforge.max_install_profile_bytes,
    });
    defer probe.deinit();
    return probe.value.versionInfo != null;
}

pub fn parseProfile(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Profile) {
    if (source.len > max_profile_bytes) return error.ProfileTooLarge;
    var document = try std.json.parseFromSlice(Profile, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_profile_bytes,
    });
    errdefer document.deinit();
    try validateProfile(document.value);
    return document;
}

fn validateProfile(profile: Profile) !void {
    const universal = try neoforge.parseCoordinate(profile.install.path);
    if (!std.mem.eql(u8, universal.extension, "jar")) return error.UnsupportedLibraryExtension;
    try neoforge.validateVersionId(profile.install.minecraft);
    try neoforge.validateVersionId(profile.versionInfo.id);
    if (profile.versionInfo.inheritsFrom) |parent| try neoforge.validateVersionId(parent);
    try validateMainClass(profile.versionInfo.mainClass);
    try validateArguments(profile.versionInfo.minecraftArguments);
    if (profile.versionInfo.libraries.len == 0 or profile.versionInfo.libraries.len > max_libraries) {
        return error.InvalidLibraryCount;
    }

    var universal_seen = false;
    for (profile.versionInfo.libraries) |library| {
        const coordinate = try neoforge.parseCoordinate(library.name);
        if (!std.mem.eql(u8, coordinate.extension, "jar")) return error.UnsupportedLibraryExtension;
        _ = try normalizedBaseUrl(library.url);
        if (library.checksums) |checksums| {
            if (checksums.len == 0 or checksums.len > max_checksums) return error.InvalidLibraryChecksums;
            for (checksums) |sha1| try neoforge.validateSha1(sha1);
        }
        if (std.mem.eql(u8, library.name, profile.install.path)) universal_seen = true;
    }
    if (!universal_seen) return error.UniversalLibraryMissing;
}

fn validateMainClass(value: []const u8) !void {
    if (value.len == 0 or value.len > 512) return error.InvalidMainClass;
    for (value) |byte| {
        if (byte <= 0x20 or byte >= 0x7f or byte == '/' or byte == '\\') return error.InvalidMainClass;
    }
}

fn validateArguments(value: []const u8) !void {
    if (value.len == 0 or value.len > max_arguments_bytes) return error.InvalidMinecraftArguments;
    for (value) |byte| {
        if (byte < 0x20 or byte >= 0x7f) return error.InvalidMinecraftArguments;
    }
}

/// Maps a legacy repository base URL to a trusted HTTPS base. Legacy
/// profiles declare plain HTTP repositories, and only well-known Maven
/// hosts are upgraded; anything else is rejected.
pub fn normalizedBaseUrl(url: ?[]const u8) ![]const u8 {
    const value = url orelse return default_repository;
    if (std.mem.startsWith(u8, value, "https://")) {
        try validateRepositoryUrl(value);
        return value;
    }
    for (trusted_repository_upgrades) |upgrade| {
        if (std.mem.eql(u8, value, upgrade.insecure)) return upgrade.secure;
    }
    return error.InsecureLibraryUrl;
}

pub fn normalizedArtifactUrlAlloc(allocator: std.mem.Allocator, url: []const u8) ![]u8 {
    if (std.mem.startsWith(u8, url, "https://")) {
        try validateRepositoryUrl(url);
        return allocator.dupe(u8, url);
    }
    for (trusted_repository_upgrades) |upgrade| {
        if (std.mem.startsWith(u8, url, upgrade.insecure)) {
            const upgraded = try std.fmt.allocPrint(allocator, "{s}{s}", .{ upgrade.secure, url[upgrade.insecure.len..] });
            errdefer allocator.free(upgraded);
            try validateRepositoryUrl(upgraded);
            return upgraded;
        }
    }
    return error.InsecureArtifactUrl;
}

fn validateRepositoryUrl(value: []const u8) !void {
    const prefix = "https://";
    const authority_end = std.mem.indexOfScalarPos(u8, value, prefix.len, '/') orelse value.len;
    if (authority_end == prefix.len) return error.InvalidRepositoryUrl;
    for (value) |byte| {
        if (byte <= 0x20 or byte >= 0x7f or byte == '\\' or byte == '?' or byte == '#' or byte == '@') {
            return error.InvalidRepositoryUrl;
        }
    }
    if (std.mem.indexOf(u8, value[prefix.len..], "..") != null) return error.InvalidRepositoryUrl;
}

/// The Forge universal jar is published under the `universal` classifier,
/// while the legacy profile lists the bare coordinate.
pub fn universalCoordinateAlloc(allocator: std.mem.Allocator, name: []const u8) ![]u8 {
    const coordinate = try neoforge.parseCoordinate(name);
    if (coordinate.classifier != null) return allocator.dupe(u8, name);
    if (!std.mem.eql(u8, coordinate.extension, "jar")) return error.UnsupportedLibraryExtension;
    return std.fmt.allocPrint(allocator, "{s}:universal", .{name});
}

pub fn artifactUrlAlloc(allocator: std.mem.Allocator, base: []const u8, name: []const u8) ![]u8 {
    try validateRepositoryUrl(base);
    const path = try neoforge.artifactPathAlloc(allocator, name);
    defer allocator.free(path);
    if (std.mem.endsWith(u8, base, "/")) {
        return std.fmt.allocPrint(allocator, "{s}{s}", .{ base, path });
    }
    return std.fmt.allocPrint(allocator, "{s}/{s}", .{ base, path });
}

/// Merges the legacy version info over the vanilla parent metadata. The
/// legacy arguments string replaces the parent's and the resolved
/// libraries (with verified integrity) take classpath precedence.
pub fn merge(
    allocator: std.mem.Allocator,
    parent: version_metadata.Metadata,
    profile: Profile,
    resolved: []const ResolvedLibrary,
) !neoforge.Overlay {
    if (resolved.len != profile.versionInfo.libraries.len) return error.LibraryIntegrityMismatch;

    const arena = try allocator.create(std.heap.ArenaAllocator);
    errdefer allocator.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(allocator);
    errdefer arena.deinit();
    const output = arena.allocator();

    const libraries = try output.alloc(version_metadata.Library, resolved.len + parent.libraries.len);
    for (resolved, 0..) |library, index| {
        libraries[index] = .{
            .name = try output.dupe(u8, profile.versionInfo.libraries[index].name),
            .downloads = .{ .artifact = .{
                .path = try output.dupe(u8, library.path),
                .sha1 = try output.dupe(u8, library.sha1),
                .size = library.size,
                .url = try output.dupe(u8, library.url),
            } },
        };
    }
    @memcpy(libraries[resolved.len..], parent.libraries);

    var value = parent;
    value.id = try output.dupe(u8, profile.versionInfo.id);
    value.type = profile.versionInfo.type orelse parent.type;
    value.mainClass = try output.dupe(u8, profile.versionInfo.mainClass);
    value.libraries = libraries;
    value.arguments = null;
    value.minecraftArguments = try output.dupe(u8, profile.versionInfo.minecraftArguments);
    value.releaseTime = profile.versionInfo.releaseTime orelse parent.releaseTime;
    value.time = profile.versionInfo.time orelse parent.time;
    value.inheritsFrom = null;
    value.patched_client = null;
    return .{ .arena = arena, .value = value };
}

test "detects the legacy profile layout" {
    const allocator = std.testing.allocator;
    try std.testing.expect(try isLegacyProfile(allocator, "{\"install\":{},\"versionInfo\":{}}"));
    try std.testing.expect(!try isLegacyProfile(allocator, "{\"spec\":1,\"processors\":[],\"data\":{}}"));
}

test "parses a legacy 1.7.10 style profile" {
    const allocator = std.testing.allocator;
    const source =
        \\{
        \\  "install":{
        \\    "profileName":"Forge",
        \\    "target":"1.7.10-Forge10.13.4.1614-1.7.10",
        \\    "path":"net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10",
        \\    "version":"Forge 10.13.4.1614-1.7.10",
        \\    "filePath":"forge-1.7.10-10.13.4.1614-1.7.10-universal.jar",
        \\    "minecraft":"1.7.10"
        \\  },
        \\  "versionInfo":{
        \\    "id":"1.7.10-Forge10.13.4.1614-1.7.10",
        \\    "mainClass":"net.minecraft.launchwrapper.Launch",
        \\    "minecraftArguments":"--username ${auth_player_name} --tweakClass cpw.mods.fml.common.launcher.FMLTweaker",
        \\    "inheritsFrom":"1.7.10",
        \\    "jar":"1.7.10",
        \\    "libraries":[
        \\      {"name":"net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10","url":"https://maven.minecraftforge.net/"},
        \\      {"name":"net.minecraft:launchwrapper:1.12","serverreq":true},
        \\      {"name":"org.scala-lang:scala-library:2.11.1","url":"https://maven.minecraftforge.net/","checksums":["0e11da23da3eabab9f4777b9220e60d44c1aab6a","1e4df76e835201c6eabd43adca89ab11f225f134"],"serverreq":true,"clientreq":true}
        \\    ]
        \\  }
        \\}
    ;
    var profile = try parseProfile(allocator, source);
    defer profile.deinit();

    try std.testing.expectEqualStrings("net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10", profile.value.install.path);
    try std.testing.expectEqualStrings("1.7.10", profile.value.install.minecraft);
    try std.testing.expectEqualStrings("1.7.10-Forge10.13.4.1614-1.7.10", profile.value.versionInfo.id);
    try std.testing.expectEqualStrings("net.minecraft.launchwrapper.Launch", profile.value.versionInfo.mainClass);
    try std.testing.expectEqual(@as(usize, 3), profile.value.versionInfo.libraries.len);
    try std.testing.expectEqualStrings(
        "0e11da23da3eabab9f4777b9220e60d44c1aab6a",
        profile.value.versionInfo.libraries[2].checksums.?[0],
    );
}

test "rejects profiles without the universal library" {
    const allocator = std.testing.allocator;
    const source =
        \\{
        \\  "install":{"path":"net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10","minecraft":"1.7.10"},
        \\  "versionInfo":{
        \\    "id":"legacy","mainClass":"net.minecraft.launchwrapper.Launch",
        \\    "minecraftArguments":"--username ${auth_player_name}",
        \\    "libraries":[{"name":"net.minecraft:launchwrapper:1.12"}]
        \\  }
        \\}
    ;
    try std.testing.expectError(error.UniversalLibraryMissing, parseProfile(allocator, source));
}

test "rejects insecure repository urls outside the allowlist" {
    const allocator = std.testing.allocator;
    try std.testing.expectError(error.InsecureLibraryUrl, normalizedBaseUrl("http://evil.example/maven/"));
    const source =
        \\{
        \\  "install":{"path":"net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10","minecraft":"1.7.10"},
        \\  "versionInfo":{
        \\    "id":"legacy","mainClass":"net.minecraft.launchwrapper.Launch",
        \\    "minecraftArguments":"--username ${auth_player_name}",
        \\    "libraries":[
        \\      {"name":"net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10","url":"http://evil.example/maven/"}
        \\    ]
        \\  }
        \\}
    ;
    try std.testing.expectError(error.InsecureLibraryUrl, parseProfile(allocator, source));
}

test "normalizes legacy repository bases to trusted https" {
    try std.testing.expectEqualStrings(default_repository, try normalizedBaseUrl(null));
    try std.testing.expectEqualStrings(
        "https://maven.minecraftforge.net/",
        try normalizedBaseUrl("http://files.minecraftforge.net/maven/"),
    );
    try std.testing.expectEqualStrings(
        "https://libraries.minecraft.net/",
        try normalizedBaseUrl("http://libraries.minecraft.net/"),
    );
    try std.testing.expectEqualStrings(
        "https://repo1.maven.org/maven2/",
        try normalizedBaseUrl("http://repo1.maven.org/maven2/"),
    );
    try std.testing.expectEqualStrings(
        "https://maven.minecraftforge.net/",
        try normalizedBaseUrl("https://maven.minecraftforge.net/"),
    );
}

test "the universal coordinate gains the universal classifier" {
    const allocator = std.testing.allocator;
    const coordinate = try universalCoordinateAlloc(allocator, "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10");
    defer allocator.free(coordinate);
    try std.testing.expectEqualStrings("net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10:universal", coordinate);

    const path = try neoforge.artifactPathAlloc(allocator, coordinate);
    defer allocator.free(path);
    try std.testing.expectEqualStrings(
        "net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-universal.jar",
        path,
    );

    const classified = try universalCoordinateAlloc(allocator, "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10:universal");
    defer allocator.free(classified);
    try std.testing.expectEqualStrings("net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10:universal", classified);
}

test "joins repository bases and artifact paths" {
    const allocator = std.testing.allocator;
    const url = try artifactUrlAlloc(allocator, "https://libraries.minecraft.net/", "net.minecraft:launchwrapper:1.12");
    defer allocator.free(url);
    try std.testing.expectEqualStrings(
        "https://libraries.minecraft.net/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar",
        url,
    );
    const slashless = try artifactUrlAlloc(allocator, "https://libraries.minecraft.net", "net.minecraft:launchwrapper:1.12");
    defer allocator.free(slashless);
    try std.testing.expectEqualStrings(
        "https://libraries.minecraft.net/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar",
        slashless,
    );
}

test "merges a legacy profile over its vanilla parent" {
    const allocator = std.testing.allocator;
    const parent_source =
        \\{
        \\  "id":"1.7.10",
        \\  "type":"release",
        \\  "mainClass":"net.minecraft.client.main.Main",
        \\  "minecraftArguments":"--username ${auth_player_name} --session ${auth_session}",
        \\  "downloads":{"client":{"sha1":"1111111111111111111111111111111111111111","size":1,"url":"https://example.invalid/client.jar"}},
        \\  "libraries":[{"name":"com.example:parent:1.0","downloads":{"artifact":{"path":"com/example/parent/1.0/parent-1.0.jar","sha1":"2222222222222222222222222222222222222222","size":2,"url":"https://example.invalid/parent.jar"}}}]
        \\}
    ;
    const profile_source =
        \\{
        \\  "install":{"path":"net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10","minecraft":"1.7.10"},
        \\  "versionInfo":{
        \\    "id":"1.7.10-Forge10.13.4.1614-1.7.10",
        \\    "mainClass":"net.minecraft.launchwrapper.Launch",
        \\    "minecraftArguments":"--username ${auth_player_name} --tweakClass cpw.mods.fml.common.launcher.FMLTweaker",
        \\    "libraries":[
        \\      {"name":"net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10","url":"https://maven.minecraftforge.net/"},
        \\      {"name":"net.minecraft:launchwrapper:1.12"}
        \\    ]
        \\  }
        \\}
    ;
    var parent = try version_metadata.parse(allocator, parent_source);
    defer parent.deinit();
    var profile = try parseProfile(allocator, profile_source);
    defer profile.deinit();

    const resolved = [_]ResolvedLibrary{
        .{
            .path = "net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-universal.jar",
            .url = "https://maven.minecraftforge.net/net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-universal.jar",
            .sha1 = "25fd97f72beca728112256938e03e8105b1b78cc",
            .size = 3018833,
        },
        .{
            .path = "net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar",
            .url = "https://libraries.minecraft.net/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar",
            .sha1 = "111e7bea9c968cdb3d06ef4632bf7ff0824d0f36",
            .size = 32999,
        },
    };
    var overlay = try merge(allocator, parent.value, profile.value, &resolved);
    defer overlay.deinit();

    try std.testing.expectEqualStrings("1.7.10-Forge10.13.4.1614-1.7.10", overlay.value.id);
    try std.testing.expectEqualStrings("net.minecraft.launchwrapper.Launch", overlay.value.mainClass);
    try std.testing.expect(overlay.value.arguments == null);
    try std.testing.expect(std.mem.indexOf(u8, overlay.value.minecraftArguments.?, "FMLTweaker") != null);
    try std.testing.expectEqual(@as(usize, 3), overlay.value.libraries.len);
    const universal = overlay.value.libraries[0].downloads.?.artifact.?;
    try std.testing.expectEqualStrings(
        "net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-universal.jar",
        universal.path,
    );
    try std.testing.expectEqualStrings("25fd97f72beca728112256938e03e8105b1b78cc", universal.sha1);
    try std.testing.expectEqualStrings("com.example:parent:1.0", overlay.value.libraries[2].name);
    try std.testing.expect(overlay.value.patched_client == null);
    try std.testing.expect(overlay.value.inheritsFrom == null);
}

test "rejects arguments with control characters" {
    const allocator = std.testing.allocator;
    const source =
        \\{
        \\  "install":{"path":"net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10","minecraft":"1.7.10"},
        \\  "versionInfo":{
        \\    "id":"legacy","mainClass":"net.minecraft.launchwrapper.Launch",
        \\    "minecraftArguments":"--username ${auth_player_name}\n--injected",
        \\    "libraries":[{"name":"net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10"}]
        \\  }
        \\}
    ;
    try std.testing.expectError(error.InvalidMinecraftArguments, parseProfile(allocator, source));
}
