const std = @import("std");
const modrinth = @import("../content/modrinth.zig");
const install_plan = @import("../install/plan.zig");

pub const index_name = "modrinth.index.json";
pub const overrides_directory = "overrides";
pub const client_overrides_directory = "client-overrides";
pub const format_version: u32 = 1;
pub const max_index_bytes: usize = 16 * 1024 * 1024;
pub const max_files: usize = 4096;
pub const max_file_size: u64 = 512 * 1024 * 1024;
pub const max_download_urls_per_file: usize = 8;

pub const Env = struct {
    client: ?[]const u8 = null,
    server: ?[]const u8 = null,
};

pub const FileHashes = struct {
    sha1: []const u8,
    sha512: ?[]const u8 = null,
};

pub const File = struct {
    path: []const u8,
    hashes: FileHashes,
    env: ?Env = null,
    downloads: []const []const u8,
    fileSize: u64,
};

pub const Dependencies = struct {
    minecraft: []const u8,
    @"fabric-loader": ?[]const u8 = null,
    @"quilt-loader": ?[]const u8 = null,
    forge: ?[]const u8 = null,
    neoforge: ?[]const u8 = null,
};

pub const Index = struct {
    formatVersion: u32,
    game: []const u8,
    name: []const u8,
    versionId: ?[]const u8 = null,
    summary: ?[]const u8 = null,
    files: []File,
    dependencies: Dependencies,
};

pub const Loader = enum {
    fabric,
    forge,
    neoforge,
};

pub const LoaderSelection = struct {
    loader: Loader,
    version: []const u8,
};

pub fn parse(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Index) {
    if (source.len > max_index_bytes) return error.ModpackIndexTooLarge;
    var document = try std.json.parseFromSlice(Index, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_index_bytes,
    });
    errdefer document.deinit();
    try validate(document.value);
    return document;
}

fn validate(index: Index) !void {
    if (index.formatVersion != format_version) return error.UnsupportedModpackFormat;
    if (!std.mem.eql(u8, index.game, "minecraft")) return error.UnsupportedModpackGame;
    if (index.name.len == 0 or index.name.len > 256) return error.InvalidModpackName;
    if (index.versionId) |version_id| {
        if (version_id.len == 0 or version_id.len > 256) return error.InvalidModpackVersion;
    }
    _ = try loaderSelection(index.dependencies);
    try modrinth.validateGameVersion(index.dependencies.minecraft);
    if (index.files.len > max_files) return error.TooManyModpackFiles;
    for (index.files, 0..) |file, file_index| {
        try validateFile(file);
        for (index.files[file_index + 1 ..]) |other| {
            if (std.mem.eql(u8, file.path, other.path)) return error.DuplicateModpackFile;
        }
    }
}

fn validateFile(file: File) !void {
    try install_plan.validateRelativePath(file.path);
    if (file.path.len > 512) return error.InvalidModpackFile;
    try modrinth.validateSha1(file.hashes.sha1);
    if (file.hashes.sha512) |sha512| {
        if (sha512.len != 128) return error.InvalidModrinthHash;
        for (sha512) |byte| if (!std.ascii.isHex(byte)) return error.InvalidModrinthHash;
    }
    if (file.fileSize == 0 or file.fileSize > max_file_size) return error.InvalidModpackFileSize;
    if (file.downloads.len == 0 or file.downloads.len > max_download_urls_per_file) {
        return error.InvalidModpackDownloads;
    }
    for (file.downloads) |url| try validateDownloadUrl(url);
    if (file.env) |env| {
        if (env.client) |client| try validateEnvValue(client);
        if (env.server) |server| try validateEnvValue(server);
    }
}

fn validateEnvValue(value: []const u8) !void {
    if (!std.mem.eql(u8, value, "required") and
        !std.mem.eql(u8, value, "optional") and
        !std.mem.eql(u8, value, "unsupported"))
    {
        return error.InvalidModpackEnv;
    }
}

fn validateDownloadUrl(value: []const u8) !void {
    if (!std.mem.startsWith(u8, value, "https://")) return error.InsecureModpackUrl;
    if (value.len > 2048) return error.InvalidModpackUrl;
    for (value) |byte| {
        if (byte <= 0x20 or byte == 0x7f or byte == '\\' or byte == '"') return error.InvalidModpackUrl;
    }
}

/// Picks the single supported mod loader declared by the pack, if any.
/// Quilt is rejected outright; packs declaring more than one loader are
/// contradictory.
pub fn loaderSelection(dependencies: Dependencies) !?LoaderSelection {
    if (dependencies.@"quilt-loader" != null) return error.UnsupportedModpackLoader;
    var selection: ?LoaderSelection = null;
    if (dependencies.@"fabric-loader") |version| {
        try validateLoaderVersion(version);
        selection = .{ .loader = .fabric, .version = version };
    }
    if (dependencies.forge) |version| {
        if (selection != null) return error.ModpackLoaderConflict;
        try validateLoaderVersion(version);
        selection = .{ .loader = .forge, .version = version };
    }
    if (dependencies.neoforge) |version| {
        if (selection != null) return error.ModpackLoaderConflict;
        try validateLoaderVersion(version);
        selection = .{ .loader = .neoforge, .version = version };
    }
    return selection;
}

fn validateLoaderVersion(version: []const u8) !void {
    if (version.len == 0 or version.len > 64) return error.InvalidModpackLoaderVersion;
    for (version) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '.' and byte != '-' and byte != '_' and byte != '+') {
            return error.InvalidModpackLoaderVersion;
        }
    }
}

/// Whether the file ships to client installations. Files marked
/// client-unsupported are server-only and skipped.
pub fn shipsToClient(file: File) bool {
    const env = file.env orelse return true;
    const client = env.client orelse return true;
    return !std.mem.eql(u8, client, "unsupported");
}

pub const ModrinthIds = struct {
    project_id: []const u8,
    version_id: []const u8,
};

/// Extracts the Modrinth project and version id from a CDN download URL of
/// the form `https://cdn.modrinth.com/data/<project>/versions/<version>/<file>`.
pub fn modrinthIdsFromUrl(url: []const u8) ?ModrinthIds {
    const prefixes = [_][]const u8{
        "https://cdn.modrinth.com/data/",
        "https://cdn-alt.modrinth.com/data/",
    };
    for (prefixes) |prefix| {
        if (!std.mem.startsWith(u8, url, prefix)) continue;
        const rest = url[prefix.len..];
        const project_end = std.mem.indexOfScalar(u8, rest, '/') orelse continue;
        const project_id = rest[0..project_end];
        const after_project = rest[project_end + 1 ..];
        if (!std.mem.startsWith(u8, after_project, "versions/")) continue;
        const version_rest = after_project["versions/".len..];
        const version_end = std.mem.indexOfScalar(u8, version_rest, '/') orelse continue;
        const version_id = version_rest[0..version_end];
        if (version_end + 1 >= version_rest.len) continue;
        modrinth.validateVersionId(project_id) catch continue;
        modrinth.validateVersionId(version_id) catch continue;
        return .{ .project_id = project_id, .version_id = version_id };
    }
    return null;
}

const index_fixture =
    \\{
    \\  "formatVersion": 1,
    \\  "game": "minecraft",
    \\  "name": "Test Pack",
    \\  "versionId": "1.0.0",
    \\  "summary": "A test pack",
    \\  "files": [
    \\    {"path":"mods/sodium.jar","hashes":{"sha1":"1111111111111111111111111111111111111111","sha512":null},"downloads":["https://cdn.modrinth.com/data/AANobbMI/versions/XyZaBcDe/sodium.jar"],"fileSize":100},
    \\    {"path":"mods/server-only.jar","hashes":{"sha1":"2222222222222222222222222222222222222222"},"env":{"client":"unsupported","server":"required"},"downloads":["https://cdn.example.com/server-only.jar"],"fileSize":200}
    \\  ],
    \\  "dependencies": {"minecraft":"1.20.4","fabric-loader":"0.15.7"}
    \\}
;

test "parses and validates a modrinth index" {
    var document = try parse(std.testing.allocator, index_fixture);
    defer document.deinit();
    const index = document.value;
    try std.testing.expectEqualStrings("Test Pack", index.name);
    try std.testing.expectEqual(@as(usize, 2), index.files.len);
    const selection = (try loaderSelection(index.dependencies)).?;
    try std.testing.expectEqual(Loader.fabric, selection.loader);
    try std.testing.expectEqualStrings("0.15.7", selection.version);
    try std.testing.expect(shipsToClient(index.files[0]));
    try std.testing.expect(!shipsToClient(index.files[1]));
}

test "extracts modrinth ids from cdn urls" {
    const ids = modrinthIdsFromUrl("https://cdn.modrinth.com/data/AANobbMI/versions/XyZaBcDe/sodium.jar").?;
    try std.testing.expectEqualStrings("AANobbMI", ids.project_id);
    try std.testing.expectEqualStrings("XyZaBcDe", ids.version_id);
    const alt = modrinthIdsFromUrl("https://cdn-alt.modrinth.com/data/AANobbMI/versions/XyZaBcDe/sodium.jar").?;
    try std.testing.expectEqualStrings("XyZaBcDe", alt.version_id);
    try std.testing.expect(modrinthIdsFromUrl("https://cdn.example.com/data/AANobbMI/versions/XyZaBcDe/x.jar") == null);
    try std.testing.expect(modrinthIdsFromUrl("https://cdn.modrinth.com/data/not valid!/versions/XyZaBcDe/x.jar") == null);
}

test "rejects invalid index documents" {
    const allocator = std.testing.allocator;
    const quilt =
        \\{"formatVersion":1,"game":"minecraft","name":"Pack","files":[],"dependencies":{"minecraft":"1.20.4","quilt-loader":"0.24.0"}}
    ;
    try std.testing.expectError(error.UnsupportedModpackLoader, parse(allocator, quilt));

    const conflict =
        \\{"formatVersion":1,"game":"minecraft","name":"Pack","files":[],"dependencies":{"minecraft":"1.20.4","fabric-loader":"0.15.7","forge":"47.2.0"}}
    ;
    try std.testing.expectError(error.ModpackLoaderConflict, parse(allocator, conflict));

    const traversal =
        \\{"formatVersion":1,"game":"minecraft","name":"Pack","files":[{"path":"../escape.jar","hashes":{"sha1":"1111111111111111111111111111111111111111"},"downloads":["https://cdn.example.com/x.jar"],"fileSize":1}],"dependencies":{"minecraft":"1.20.4"}}
    ;
    try std.testing.expectError(error.UnsafePath, parse(allocator, traversal));

    const insecure =
        \\{"formatVersion":1,"game":"minecraft","name":"Pack","files":[{"path":"mods/x.jar","hashes":{"sha1":"1111111111111111111111111111111111111111"},"downloads":["http://cdn.example.com/x.jar"],"fileSize":1}],"dependencies":{"minecraft":"1.20.4"}}
    ;
    try std.testing.expectError(error.InsecureModpackUrl, parse(allocator, insecure));

    const duplicate =
        \\{"formatVersion":1,"game":"minecraft","name":"Pack","files":[{"path":"mods/x.jar","hashes":{"sha1":"1111111111111111111111111111111111111111"},"downloads":["https://cdn.example.com/x.jar"],"fileSize":1},{"path":"mods/x.jar","hashes":{"sha1":"2222222222222222222222222222222222222222"},"downloads":["https://cdn.example.com/y.jar"],"fileSize":2}],"dependencies":{"minecraft":"1.20.4"}}
    ;
    try std.testing.expectError(error.DuplicateModpackFile, parse(allocator, duplicate));

    const bad_env =
        \\{"formatVersion":1,"game":"minecraft","name":"Pack","files":[{"path":"mods/x.jar","hashes":{"sha1":"1111111111111111111111111111111111111111"},"env":{"client":"sometimes"},"downloads":["https://cdn.example.com/x.jar"],"fileSize":1}],"dependencies":{"minecraft":"1.20.4"}}
    ;
    try std.testing.expectError(error.InvalidModpackEnv, parse(allocator, bad_env));
}
