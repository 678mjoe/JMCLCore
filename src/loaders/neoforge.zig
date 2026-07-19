const std = @import("std");
const version_metadata = @import("../install/version_metadata.zig");

pub const maven_root = "https://maven.neoforged.net/releases/";
pub const max_installer_bytes: usize = 64 * 1024 * 1024;
pub const max_version_profile_bytes: usize = 4 * 1024 * 1024;
pub const max_install_profile_bytes: usize = 16 * 1024 * 1024;
pub const max_profile_libraries: usize = 1_024;
pub const max_processors: usize = 16;
pub const max_processor_arguments: usize = 64;
pub const max_processor_classpath: usize = 64;
pub const max_argument_bytes: usize = 4 * 1024;
pub const max_data_entries: usize = 64;
pub const max_library_size: u64 = 256 * 1024 * 1024;

pub const Side = enum { client, server };

pub const DataValue = struct {
    client: []const u8,
    server: ?[]const u8 = null,
};

pub const DataMap = std.json.ArrayHashMap(DataValue);

pub const Processor = struct {
    jar: []const u8,
    sides: ?[]const []const u8 = null,
    classpath: []const []const u8 = &.{},
    args: []const []const u8 = &.{},
};

pub const InstallProfile = struct {
    spec: u32,
    version: []const u8,
    minecraft: []const u8,
    data: DataMap,
    processors: []const Processor = &.{},
    libraries: []const version_metadata.Library = &.{},
};

pub const VersionProfile = struct {
    id: []const u8,
    inheritsFrom: []const u8,
    mainClass: []const u8,
    type: ?[]const u8 = null,
    time: ?[]const u8 = null,
    releaseTime: ?[]const u8 = null,
    arguments: ?version_metadata.Arguments = null,
    libraries: []const version_metadata.Library = &.{},
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

pub const Coordinate = struct {
    group: []const u8,
    artifact: []const u8,
    version: []const u8,
    classifier: ?[]const u8,
    extension: []const u8,
};

pub fn parseCoordinate(value: []const u8) !Coordinate {
    if (value.len == 0 or value.len > 512) return error.InvalidMavenCoordinate;
    var extension: []const u8 = "jar";
    var coordinate = value;
    if (std.mem.lastIndexOfScalar(u8, coordinate, '@')) |at| {
        extension = coordinate[at + 1 ..];
        coordinate = coordinate[0..at];
        try validateExtension(extension);
    }
    var components = std.mem.splitScalar(u8, coordinate, ':');
    const group = components.next() orelse return error.InvalidMavenCoordinate;
    const artifact = components.next() orelse return error.InvalidMavenCoordinate;
    const version = components.next() orelse return error.InvalidMavenCoordinate;
    const classifier = components.next();
    if (components.next() != null) return error.InvalidMavenCoordinate;

    try validateGroup(group);
    try validateComponent(artifact);
    try validateComponent(version);
    if (classifier) |name| try validateComponent(name);
    return .{
        .group = group,
        .artifact = artifact,
        .version = version,
        .classifier = classifier,
        .extension = extension,
    };
}

pub fn artifactPathAlloc(allocator: std.mem.Allocator, value: []const u8) ![]u8 {
    const coordinate = try parseCoordinate(value);
    var output: std.Io.Writer.Allocating = .init(allocator);
    errdefer output.deinit();
    for (coordinate.group) |byte| try output.writer.writeByte(if (byte == '.') '/' else byte);
    try output.writer.print("/{s}/{s}/{s}-{s}", .{
        coordinate.artifact,
        coordinate.version,
        coordinate.artifact,
        coordinate.version,
    });
    if (coordinate.classifier) |classifier| try output.writer.print("-{s}", .{classifier});
    try output.writer.print(".{s}", .{coordinate.extension});
    return output.toOwnedSlice();
}

pub fn installerUrlAlloc(allocator: std.mem.Allocator, neoforge_version: []const u8) ![]u8 {
    try validateVersionId(neoforge_version);
    return std.fmt.allocPrint(
        allocator,
        maven_root ++ "net/neoforged/neoforge/{s}/neoforge-{s}-installer.jar",
        .{ neoforge_version, neoforge_version },
    );
}

pub fn parseInstallProfile(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(InstallProfile) {
    if (source.len > max_install_profile_bytes) return error.InstallProfileTooLarge;
    var document = try std.json.parseFromSlice(InstallProfile, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_install_profile_bytes,
    });
    errdefer document.deinit();
    try validateInstallProfile(document.value);
    return document;
}

pub fn parseVersionProfile(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(VersionProfile) {
    if (source.len > max_version_profile_bytes) return error.VersionProfileTooLarge;
    var document = try std.json.parseFromSlice(VersionProfile, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_version_profile_bytes,
    });
    errdefer document.deinit();
    try validateVersionProfile(document.value);
    return document;
}

pub fn validateInstallProfile(profile: InstallProfile) !void {
    if (profile.spec > 1) return error.UnsupportedInstallProfileSpec;
    try validateVersionId(profile.version);
    try validateVersionId(profile.minecraft);

    if (profile.data.map.count() > max_data_entries) return error.TooManyDataEntries;
    var iterator = profile.data.map.iterator();
    while (iterator.next()) |entry| {
        try validatePlaceholderName(entry.key_ptr.*);
        try validateDataValue(entry.value_ptr.client);
        if (entry.value_ptr.server) |value| try validateDataValue(value);
    }

    if (profile.processors.len > max_processors) return error.TooManyProcessors;
    for (profile.processors) |processor| {
        _ = try parseCoordinate(processor.jar);
        if (processor.classpath.len > max_processor_classpath) return error.TooManyProcessorClasspaths;
        for (processor.classpath) |entry| _ = try parseCoordinate(entry);
        if (processor.args.len > max_processor_arguments) return error.TooManyProcessorArguments;
        for (processor.args) |argument| {
            if (argument.len == 0 or argument.len > max_argument_bytes) return error.InvalidProcessorArgument;
        }
        if (processor.sides) |sides| for (sides) |side| {
            if (!std.mem.eql(u8, side, "client") and !std.mem.eql(u8, side, "server")) {
                return error.InvalidProcessorSide;
            }
        };
    }

    if (profile.libraries.len > max_profile_libraries) return error.TooManyLibraries;
    for (profile.libraries) |library| try validateProfileLibrary(library, true);
}

pub fn validateVersionProfile(profile: VersionProfile) !void {
    try validateVersionId(profile.id);
    try validateVersionId(profile.inheritsFrom);
    if (profile.mainClass.len == 0 or profile.mainClass.len > 512) return error.InvalidMainClass;
    for (profile.mainClass) |byte| {
        if (!isVisibleAscii(byte) or byte == '/' or byte == '\\') return error.InvalidMainClass;
    }
    if (profile.libraries.len > max_profile_libraries) return error.TooManyLibraries;
    for (profile.libraries) |library| try validateProfileLibrary(library, false);
}

fn validateProfileLibrary(library: version_metadata.Library, allow_data_extensions: bool) !void {
    const coordinate = try parseCoordinate(library.name);
    if (!std.mem.eql(u8, coordinate.extension, "jar")) {
        const data_extension = allow_data_extensions and
            (std.mem.eql(u8, coordinate.extension, "zip") or
                std.mem.eql(u8, coordinate.extension, "txt") or
                std.mem.eql(u8, coordinate.extension, "lzma"));
        if (!data_extension) return error.UnsupportedLibraryExtension;
    }
    const downloads = library.downloads orelse return error.MissingLibraryDownloads;
    const artifact = downloads.artifact orelse return error.MissingLibraryArtifact;
    try validateArtifactUrl(artifact.url);
    try validateSha1(artifact.sha1);
    if (artifact.size == 0 or artifact.size > max_library_size) return error.InvalidLibrarySize;
}

fn validateDataValue(value: []const u8) !void {
    if (value.len == 0 or value.len > max_argument_bytes) return error.InvalidDataValue;
    switch (classifyDataValue(value)) {
        .embedded => |path| {
            if (path.len == 0 or std.mem.indexOf(u8, path, "..") != null or
                std.mem.indexOfScalar(u8, path, '\\') != null) return error.InvalidDataValue;
        },
        .coordinate => |coordinate| _ = try parseCoordinate(coordinate),
        .literal => {},
    }
}

pub const DataClassification = union(enum) {
    embedded: []const u8,
    coordinate: []const u8,
    literal: []const u8,
};

pub fn classifyDataValue(value: []const u8) DataClassification {
    if (value.len >= 2 and value[0] == '[' and value[value.len - 1] == ']') {
        return .{ .coordinate = value[1 .. value.len - 1] };
    }
    if (value.len >= 2 and value[0] == '\'' and value[value.len - 1] == '\'') {
        return .{ .literal = value[1 .. value.len - 1] };
    }
    if (value.len >= 1 and value[0] == '/') return .{ .embedded = value[1..] };
    return .{ .literal = value };
}

pub fn dataValueForSide(value: DataValue, side: Side) []const u8 {
    return switch (side) {
        .client => value.client,
        .server => value.server orelse value.client,
    };
}

pub fn processorApplies(processor: Processor, side: Side) bool {
    const sides = processor.sides orelse return true;
    if (sides.len == 0) return true;
    const name: []const u8 = switch (side) {
        .client => "client",
        .server => "server",
    };
    for (sides) |entry| {
        if (std.mem.eql(u8, entry, name)) return true;
    }
    return false;
}

pub fn patchedClientCoordinate(profile: InstallProfile) ![]const u8 {
    const entry = profile.data.map.get("PATCHED") orelse return error.MissingPatchedData;
    const value = dataValueForSide(entry, .client);
    return switch (classifyDataValue(value)) {
        .coordinate => |coordinate| coordinate,
        else => error.MissingPatchedData,
    };
}

pub fn patchedClientPathAlloc(allocator: std.mem.Allocator, profile: InstallProfile) ![]u8 {
    const coordinate = try patchedClientCoordinate(profile);
    const path = try artifactPathAlloc(allocator, coordinate);
    defer allocator.free(path);
    return std.fmt.allocPrint(allocator, "libraries/{s}", .{path});
}

pub const PlaceholderResolver = struct {
    context: *anyopaque,
    resolve: *const fn (context: *anyopaque, key: []const u8) ?[]const u8,
    artifact: *const fn (context: *anyopaque, coordinate: []const u8, allocator: std.mem.Allocator) anyerror![]u8,
};

pub fn substituteArgumentAlloc(
    allocator: std.mem.Allocator,
    template: []const u8,
    resolver: PlaceholderResolver,
) ![]u8 {
    if (template.len == 0 or template.len > max_argument_bytes) return error.InvalidProcessorArgument;
    if (template.len >= 2 and template[0] == '[' and template[template.len - 1] == ']') {
        const coordinate = template[1 .. template.len - 1];
        _ = try parseCoordinate(coordinate);
        return resolver.artifact(resolver.context, coordinate, allocator);
    }

    var output: std.Io.Writer.Allocating = .init(allocator);
    errdefer output.deinit();
    var index: usize = 0;
    while (index < template.len) {
        const start = std.mem.indexOfScalarPos(u8, template, index, '{') orelse {
            try output.writer.writeAll(template[index..]);
            break;
        };
        try output.writer.writeAll(template[index..start]);
        const end = std.mem.indexOfScalarPos(u8, template, start + 1, '}') orelse
            return error.UnresolvedPlaceholder;
        const key = template[start + 1 .. end];
        try validatePlaceholderName(key);
        const value = resolver.resolve(resolver.context, key) orelse return error.UnresolvedPlaceholder;
        try output.writer.writeAll(value);
        index = end + 1;
    }
    const substituted = output.written();
    if (std.mem.indexOfAny(u8, substituted, "{}") != null) return error.UnresolvedPlaceholder;
    return output.toOwnedSlice();
}

pub fn merge(
    allocator: std.mem.Allocator,
    parent: version_metadata.Metadata,
    profile: VersionProfile,
    patched_client_path: []const u8,
) !Overlay {
    try validateVersionProfile(profile);
    try validateRelativePath(patched_client_path);

    const arena = try allocator.create(std.heap.ArenaAllocator);
    errdefer allocator.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(allocator);
    errdefer arena.deinit();
    const output = arena.allocator();

    const libraries = try output.alloc(version_metadata.Library, profile.libraries.len + parent.libraries.len);
    for (profile.libraries, 0..) |library, index| {
        const downloads = library.downloads.?;
        const artifact = downloads.artifact.?;
        const path = try artifactPathAlloc(output, library.name);
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

pub fn validateVersionId(value: []const u8) !void {
    if (value.len == 0 or value.len > 256) return error.InvalidNeoForgeVersion;
    if (value[value.len - 1] == '.' or value[value.len - 1] == ' ') return error.InvalidNeoForgeVersion;
    for (value) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '-' and byte != '_' and byte != '+' and byte != '.') {
            return error.InvalidNeoForgeVersion;
        }
    }
}

fn validatePlaceholderName(value: []const u8) !void {
    if (value.len == 0 or value.len > 64) return error.InvalidPlaceholderName;
    for (value) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '_') return error.InvalidPlaceholderName;
    }
}

fn validateExtension(value: []const u8) !void {
    if (value.len == 0 or value.len > 16) return error.InvalidMavenCoordinate;
    for (value) |byte| {
        if (!std.ascii.isAlphanumeric(byte)) return error.InvalidMavenCoordinate;
    }
}

fn validateGroup(value: []const u8) !void {
    var components = std.mem.splitScalar(u8, value, '.');
    while (components.next()) |component| try validateComponent(component);
}

fn validateComponent(value: []const u8) !void {
    if (value.len == 0 or value.len > 256 or std.mem.eql(u8, value, ".") or std.mem.eql(u8, value, "..")) {
        return error.InvalidMavenCoordinate;
    }
    for (value) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '-' and byte != '_' and byte != '+' and byte != '.') {
            return error.InvalidMavenCoordinate;
        }
    }
}

fn validateArtifactUrl(value: []const u8) !void {
    const prefix = "https://";
    if (!std.mem.startsWith(u8, value, prefix)) return error.InsecureArtifactUrl;
    const authority_end = std.mem.indexOfScalarPos(u8, value, prefix.len, '/') orelse value.len;
    if (authority_end == prefix.len) return error.InvalidArtifactUrl;
    for (value) |byte| {
        if (!isVisibleAscii(byte) or byte == '\\' or byte == '?' or byte == '#' or byte == '@') {
            return error.InvalidArtifactUrl;
        }
    }
    if (std.mem.indexOf(u8, value[prefix.len..], "..") != null) return error.InvalidArtifactUrl;
}

pub fn validateRelativePath(value: []const u8) !void {
    if (value.len == 0 or value.len > 1024) return error.InvalidRelativePath;
    if (value[0] == '/' or value[value.len - 1] == '/') return error.InvalidRelativePath;
    var components = std.mem.splitScalar(u8, value, '/');
    while (components.next()) |component| {
        if (component.len == 0 or std.mem.eql(u8, component, ".") or std.mem.eql(u8, component, "..")) {
            return error.InvalidRelativePath;
        }
        for (component) |byte| {
            if (!isVisibleAscii(byte) or byte == '\\') return error.InvalidRelativePath;
        }
    }
}

pub fn validateSha1(value: []const u8) !void {
    if (value.len != 40) return error.InvalidLibrarySha1;
    for (value) |byte| if (!std.ascii.isHex(byte)) return error.InvalidLibrarySha1;
}

fn isVisibleAscii(byte: u8) bool {
    return byte >= 0x21 and byte <= 0x7e;
}

test "parses Maven coordinates with classifiers and extensions" {
    const allocator = std.testing.allocator;
    const path = try artifactPathAlloc(allocator, "net.neoforged:neoforge:20.4.251:client");
    defer allocator.free(path);
    try std.testing.expectEqualStrings(
        "net/neoforged/neoforge/20.4.251/neoforge-20.4.251-client.jar",
        path,
    );

    const zipped = try artifactPathAlloc(allocator, "net.neoforged:neoform:1.20.4-20240627.114801@zip");
    defer allocator.free(zipped);
    try std.testing.expectEqualStrings(
        "net/neoforged/neoform/1.20.4-20240627.114801/neoform-1.20.4-20240627.114801.zip",
        zipped,
    );

    const text = try artifactPathAlloc(allocator, "net.minecraft:client:1.20.4-20240627.114801:mappings@txt");
    defer allocator.free(text);
    try std.testing.expectEqualStrings(
        "net/minecraft/client/1.20.4-20240627.114801/client-1.20.4-20240627.114801-mappings.txt",
        text,
    );

    try std.testing.expectError(error.InvalidMavenCoordinate, artifactPathAlloc(allocator, "a:b:../escape"));
    try std.testing.expectError(error.InvalidMavenCoordinate, artifactPathAlloc(allocator, "a:b:1.0@bad ext"));
}

test "classifies install-profile data values" {
    const coordinate = classifyDataValue("[net.neoforged:neoforge:20.4.251:client]");
    try std.testing.expectEqualStrings("net.neoforged:neoforge:20.4.251:client", coordinate.coordinate);
    const embedded = classifyDataValue("/data/client.lzma");
    try std.testing.expectEqualStrings("data/client.lzma", embedded.embedded);
    const quoted = classifyDataValue("'1.20.4-20240627.114801'");
    try std.testing.expectEqualStrings("1.20.4-20240627.114801", quoted.literal);
}

test "selects processors for the requested side" {
    const both: Processor = .{ .jar = "a:b:1.0" };
    const server_only: Processor = .{ .jar = "a:b:1.0", .sides = &.{"server"} };
    const client_only: Processor = .{ .jar = "a:b:1.0", .sides = &.{"client"} };
    try std.testing.expect(processorApplies(both, .client));
    try std.testing.expect(!processorApplies(server_only, .client));
    try std.testing.expect(processorApplies(client_only, .client));
    try std.testing.expect(processorApplies(server_only, .server));
}

test "substitutes processor argument placeholders" {
    const allocator = std.testing.allocator;
    const context: struct {} = .{};
    const resolver: PlaceholderResolver = .{
        .context = @constCast(&context),
        .resolve = struct {
            fn resolve(_: *anyopaque, key: []const u8) ?[]const u8 {
                if (std.mem.eql(u8, key, "SIDE")) return "client";
                if (std.mem.eql(u8, key, "ROOT")) return "/game";
                return null;
            }
        }.resolve,
        .artifact = struct {
            fn artifact(_: *anyopaque, coordinate: []const u8, alloc: std.mem.Allocator) ![]u8 {
                return std.fmt.allocPrint(alloc, "/game/libraries/{s}.jar", .{coordinate});
            }
        }.artifact,
    };

    const plain = try substituteArgumentAlloc(allocator, "--side", resolver);
    defer allocator.free(plain);
    try std.testing.expectEqualStrings("--side", plain);

    const replaced = try substituteArgumentAlloc(allocator, "{ROOT}/libraries", resolver);
    defer allocator.free(replaced);
    try std.testing.expectEqualStrings("/game/libraries", replaced);

    const bracketed = try substituteArgumentAlloc(allocator, "[net.neoforged:neoform:1.0@zip]", resolver);
    defer allocator.free(bracketed);
    try std.testing.expectEqualStrings("/game/libraries/net.neoforged:neoform:1.0@zip.jar", bracketed);

    try std.testing.expectError(error.UnresolvedPlaceholder, substituteArgumentAlloc(allocator, "{UNKNOWN}", resolver));
    try std.testing.expectError(error.UnresolvedPlaceholder, substituteArgumentAlloc(allocator, "{SIDE", resolver));
}

test "merges a NeoForge version profile over its vanilla parent" {
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
        \\  "id":"neoforge-20.4.251",
        \\  "inheritsFrom":"1.20.4",
        \\  "type":"release",
        \\  "mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
        \\  "arguments":{"game":["--launchTarget","forgeclient"],"jvm":["-DlibraryDirectory=${library_directory}"]},
        \\  "libraries":[{"name":"net.neoforged:bus:7.2.0@jar","downloads":{"artifact":{"path":"net/neoforged/bus/7.2.0/bus-7.2.0.jar","sha1":"03bc91f330d0e2f350ab1dbe5026a5769dc52b9a","size":35117,"url":"https://maven.neoforged.net/releases/net/neoforged/bus/7.2.0/bus-7.2.0.jar"}}}],
        \\  "logging":{}
        \\}
    ;
    var parent = try version_metadata.parse(allocator, parent_source);
    defer parent.deinit();
    var profile = try parseVersionProfile(allocator, profile_source);
    defer profile.deinit();

    var overlay = try merge(
        allocator,
        parent.value,
        profile.value,
        "libraries/net/neoforged/neoforge/20.4.251/neoforge-20.4.251-client.jar",
    );
    defer overlay.deinit();

    try std.testing.expectEqualStrings("neoforge-20.4.251", overlay.value.id);
    try std.testing.expectEqualStrings("cpw.mods.bootstraplauncher.BootstrapLauncher", overlay.value.mainClass);
    try std.testing.expectEqual(@as(usize, 2), overlay.value.libraries.len);
    try std.testing.expectEqualStrings(
        "net/neoforged/bus/7.2.0/bus-7.2.0.jar",
        overlay.value.libraries[0].downloads.?.artifact.?.path,
    );
    try std.testing.expectEqual(@as(usize, 4), overlay.value.arguments.?.game.len);
    try std.testing.expectEqual(@as(usize, 2), overlay.value.arguments.?.jvm.len);
    try std.testing.expectEqualStrings("forgeclient", overlay.value.arguments.?.game[3].literal);
    try std.testing.expect(overlay.value.logging != null);
    try std.testing.expectEqualStrings(
        "libraries/net/neoforged/neoforge/20.4.251/neoforge-20.4.251-client.jar",
        overlay.value.patched_client.?,
    );
    try std.testing.expect(overlay.value.inheritsFrom == null);
}

test "rejects unsafe NeoForge profiles" {
    const allocator = std.testing.allocator;
    const bad_url =
        \\{
        \\  "id":"neoforge-1",
        \\  "inheritsFrom":"1.20.4",
        \\  "mainClass":"example.Main",
        \\  "libraries":[{"name":"net.neoforged:bus:7.2.0@jar","downloads":{"artifact":{"path":"x","sha1":"03bc91f330d0e2f350ab1dbe5026a5769dc52b9a","size":35117,"url":"http://maven.neoforged.net/bus.jar"}}}]
        \\}
    ;
    try std.testing.expectError(error.InsecureArtifactUrl, parseVersionProfile(allocator, bad_url));

    const bad_path =
        \\{
        \\  "id":"neoforge-1",
        \\  "inheritsFrom":"1.20.4",
        \\  "mainClass":"example.Main",
        \\  "libraries":[{"name":"net.neoforged:bus:../escape@jar","downloads":{"artifact":{"path":"x","sha1":"03bc91f330d0e2f350ab1dbe5026a5769dc52b9a","size":35117,"url":"https://maven.neoforged.net/bus.jar"}}}]
        \\}
    ;
    try std.testing.expectError(error.InvalidMavenCoordinate, parseVersionProfile(allocator, bad_path));
}
