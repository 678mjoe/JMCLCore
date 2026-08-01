const std = @import("std");

pub const StringMap = std.json.ArrayHashMap([]const u8);
pub const BoolMap = std.json.ArrayHashMap(bool);

pub const RuleAction = enum {
    allow,
    disallow,
};

pub const VersionRange = struct {
    min: ?[]const u8 = null,
    max: ?[]const u8 = null,
};

pub const OperatingSystem = struct {
    name: ?[]const u8 = null,
    version: ?[]const u8 = null,
    arch: ?[]const u8 = null,
    versionRange: ?VersionRange = null,
};

pub const Rule = struct {
    action: RuleAction,
    os: ?OperatingSystem = null,
    features: ?BoolMap = null,
};

pub const ArgumentValue = union(enum) {
    single: []const u8,
    multiple: []const []const u8,

    pub fn jsonParse(allocator: std.mem.Allocator, source: anytype, options: std.json.ParseOptions) !ArgumentValue {
        const value = try std.json.Value.jsonParse(allocator, source, options);
        return jsonParseFromValue(allocator, value, options);
    }

    pub fn jsonParseFromValue(
        allocator: std.mem.Allocator,
        source: std.json.Value,
        options: std.json.ParseOptions,
    ) !ArgumentValue {
        return switch (source) {
            .string => .{ .single = try std.json.innerParseFromValue([]const u8, allocator, source, options) },
            .array => .{ .multiple = try std.json.innerParseFromValue([]const []const u8, allocator, source, options) },
            else => error.UnexpectedToken,
        };
    }

    pub fn jsonStringify(self: ArgumentValue, json: anytype) !void {
        switch (self) {
            .single => |value| try json.write(value),
            .multiple => |values| try json.write(values),
        }
    }
};

pub const StructuredArgument = struct {
    rules: []const Rule = &.{},
    value: ArgumentValue,
};

pub const Argument = union(enum) {
    literal: []const u8,
    structured: StructuredArgument,

    pub fn jsonParse(allocator: std.mem.Allocator, source: anytype, options: std.json.ParseOptions) !Argument {
        const value = try std.json.Value.jsonParse(allocator, source, options);
        return jsonParseFromValue(allocator, value, options);
    }

    pub fn jsonParseFromValue(
        allocator: std.mem.Allocator,
        source: std.json.Value,
        options: std.json.ParseOptions,
    ) !Argument {
        return switch (source) {
            .string => .{ .literal = try std.json.innerParseFromValue([]const u8, allocator, source, options) },
            .object => .{ .structured = try std.json.innerParseFromValue(StructuredArgument, allocator, source, options) },
            else => error.UnexpectedToken,
        };
    }

    pub fn jsonStringify(self: Argument, json: anytype) !void {
        switch (self) {
            .literal => |value| try json.write(value),
            .structured => |value| try json.write(value),
        }
    }
};

pub const Arguments = struct {
    game: []const Argument,
    jvm: []const Argument = &.{},
    @"default-user-jvm": ?[]const Argument = null,
};

pub const Download = struct {
    sha1: []const u8,
    size: u64,
    url: []const u8,
};

pub const Artifact = struct {
    path: []const u8,
    sha1: []const u8,
    size: u64,
    url: []const u8,
};

pub const ArtifactMap = std.json.ArrayHashMap(Artifact);

pub const LibraryDownloads = struct {
    artifact: ?Artifact = null,
    classifiers: ?ArtifactMap = null,
};

pub const Extract = struct {
    exclude: []const []const u8 = &.{},
};

pub const Library = struct {
    name: []const u8,
    downloads: ?LibraryDownloads = null,
    rules: ?[]const Rule = null,
    natives: ?StringMap = null,
    extract: ?Extract = null,
    url: ?[]const u8 = null,
};

pub const AssetIndex = struct {
    id: []const u8,
    sha1: []const u8,
    size: u64,
    totalSize: u64,
    url: []const u8,
};

pub const Downloads = struct {
    client: Download,
    server: ?Download = null,
    client_mappings: ?Download = null,
    server_mappings: ?Download = null,
    windows_server: ?Download = null,
};

pub const JavaVersion = struct {
    component: []const u8,
    majorVersion: u16,
};

pub const LoggingFile = struct {
    id: []const u8,
    sha1: []const u8,
    size: u64,
    url: []const u8,
};

pub const LoggingClient = struct {
    argument: []const u8,
    file: LoggingFile,
    type: []const u8,
};

pub const Logging = struct {
    client: LoggingClient,
};

pub const Metadata = struct {
    id: []const u8,
    type: []const u8,
    mainClass: []const u8,
    libraries: []const Library,
    downloads: Downloads,
    arguments: ?Arguments = null,
    minecraftArguments: ?[]const u8 = null,
    assetIndex: ?AssetIndex = null,
    assets: ?[]const u8 = null,
    javaVersion: ?JavaVersion = null,
    logging: ?Logging = null,
    minimumLauncherVersion: ?u32 = null,
    complianceLevel: ?u8 = null,
    releaseTime: ?[]const u8 = null,
    time: ?[]const u8 = null,
    inheritsFrom: ?[]const u8 = null,
    patched_client: ?[]const u8 = null,
};

pub fn parse(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Metadata) {
    return std.json.parseFromSlice(Metadata, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
    });
}

pub fn verifySha1(source: []const u8, expected: []const u8) bool {
    if (expected.len != std.crypto.hash.Sha1.digest_length * 2) return false;

    var digest: [std.crypto.hash.Sha1.digest_length]u8 = undefined;
    std.crypto.hash.Sha1.hash(source, &digest, .{});
    const actual = std.fmt.bytesToHex(digest, .lower);
    return std.ascii.eqlIgnoreCase(&actual, expected);
}

test "parses modern arguments, rules, and platform libraries" {
    const source =
        \\{
        \\  "id":"26.2",
        \\  "type":"release",
        \\  "mainClass":"net.minecraft.client.main.Main",
        \\  "arguments":{
        \\    "game":["--username",{"rules":[{"action":"allow","features":{"is_demo_user":true}}],"value":"--demo"}],
        \\    "jvm":[{"rules":[{"action":"allow","os":{"name":"windows","versionRange":{"min":"10.0"}}}],"value":["-Dos.name=Windows 10"]}],
        \\    "default-user-jvm":[{"value":["-Xmx2G"]}]
        \\  },
        \\  "downloads":{"client":{"sha1":"0123456789012345678901234567890123456789","size":12,"url":"https://example.invalid/client.jar"}},
        \\  "libraries":[{"name":"org.example:demo:1.0:natives-windows","downloads":{"artifact":{"path":"org/example/demo/1.0/demo.jar","sha1":"abcdef","size":4,"url":"https://example.invalid/demo.jar"}},"rules":[{"action":"allow","os":{"name":"windows"}}]}],
        \\  "javaVersion":{"component":"java-runtime-epsilon","majorVersion":25}
        \\}
    ;

    var parsed = try parse(std.testing.allocator, source);
    defer parsed.deinit();

    try std.testing.expectEqualStrings("26.2", parsed.value.id);
    try std.testing.expectEqual(@as(u16, 25), parsed.value.javaVersion.?.majorVersion);
    try std.testing.expectEqual(@as(usize, 1), parsed.value.arguments.?.@"default-user-jvm".?.len);
    try std.testing.expectEqualStrings("10.0", parsed.value.arguments.?.jvm[0].structured.rules[0].os.?.versionRange.?.min.?);
    try std.testing.expect(parsed.value.arguments.?.game[0] == .literal);

    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var json: std.json.Stringify = .{ .writer = &output.writer };
    try json.write(parsed.value.arguments.?);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "default-user-jvm") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "--demo") != null);
}

test "parses legacy arguments and classifier natives" {
    const source =
        \\{
        \\  "id":"1.6.4",
        \\  "type":"release",
        \\  "mainClass":"net.minecraft.client.main.Main",
        \\  "minecraftArguments":"--username ${auth_player_name} --session ${auth_session}",
        \\  "downloads":{"client":{"sha1":"0123456789012345678901234567890123456789","size":12,"url":"https://example.invalid/client.jar"}},
        \\  "libraries":[{"name":"org.lwjgl:lwjgl-platform:2.9.0","downloads":{"classifiers":{"natives-osx":{"path":"org/lwjgl/lwjgl-platform/2.9.0/lwjgl.jar","sha1":"abcdef","size":4,"url":"https://example.invalid/lwjgl.jar"}}},"natives":{"osx":"natives-osx"},"extract":{"exclude":["META-INF/"]}}]
        \\}
    ;

    var parsed = try parse(std.testing.allocator, source);
    defer parsed.deinit();

    try std.testing.expect(parsed.value.arguments == null);
    try std.testing.expectEqualStrings("natives-osx", parsed.value.libraries[0].natives.?.map.get("osx").?);
    try std.testing.expect(parsed.value.libraries[0].downloads.?.classifiers.?.map.contains("natives-osx"));
}

test "verifies SHA-1 checksums" {
    try std.testing.expect(verifySha1("abc", "a9993e364706816aba3e25717850c26c9cd0d89d"));
    try std.testing.expect(!verifySha1("abcd", "a9993e364706816aba3e25717850c26c9cd0d89d"));
    try std.testing.expect(!verifySha1("abc", "too-short"));
}
