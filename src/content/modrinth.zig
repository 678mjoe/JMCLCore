const std = @import("std");

pub const api_root = "https://api.modrinth.com";
pub const max_project_bytes: usize = 1024 * 1024;
pub const max_version_bytes: usize = 4 * 1024 * 1024;
pub const max_versions_list_bytes: usize = 16 * 1024 * 1024;
pub const max_versions_per_project: usize = 512;
pub const max_files_per_version: usize = 64;
pub const max_dependencies_per_version: usize = 128;
pub const max_file_size: u64 = 512 * 1024 * 1024;

pub const Loader = enum {
    fabric,
    forge,
    neoforge,
    minecraft,

    pub fn name(loader: Loader) []const u8 {
        return @tagName(loader);
    }

    pub fn parse(value: []const u8) ?Loader {
        inline for (std.meta.fields(Loader)) |field| {
            if (std.mem.eql(u8, value, field.name)) return @enumFromInt(field.value);
        }
        return null;
    }
};

pub const ProjectType = enum {
    mod,
    resourcepack,

    pub fn parse(value: []const u8) ?ProjectType {
        inline for (std.meta.fields(ProjectType)) |field| {
            if (std.mem.eql(u8, value, field.name)) return @enumFromInt(field.value);
        }
        return null;
    }
};

pub const Project = struct {
    id: []const u8,
    slug: []const u8,
    title: ?[]const u8 = null,
    project_type: []const u8,
};

pub const DependencyType = enum {
    required,
    optional,
    incompatible,
    embedded,
};

pub const Dependency = struct {
    version_id: ?[]const u8 = null,
    project_id: ?[]const u8 = null,
    file_name: ?[]const u8 = null,
    dependency_type: DependencyType,
};

pub const Hashes = struct {
    sha1: []const u8,
    sha512: ?[]const u8 = null,
};

pub const VersionFile = struct {
    url: []const u8,
    filename: []const u8,
    primary: bool = false,
    hashes: Hashes,
    size: u64,
};

pub const VersionType = enum {
    release,
    beta,
    alpha,

    fn rank(version_type: []const u8) ?u8 {
        if (std.mem.eql(u8, version_type, "release")) return 0;
        if (std.mem.eql(u8, version_type, "beta")) return 1;
        if (std.mem.eql(u8, version_type, "alpha")) return 2;
        return null;
    }
};

pub const Version = struct {
    id: []const u8,
    project_id: []const u8,
    version_number: []const u8,
    version_type: []const u8,
    game_versions: []const []const u8,
    loaders: []const []const u8,
    files: []const VersionFile,
    dependencies: []const Dependency = &.{},
};

pub fn validateProjectRef(value: []const u8) !void {
    if (value.len == 0 or value.len > 64) return error.InvalidModrinthProject;
    for (value) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '-' and byte != '_' and byte != '.') {
            return error.InvalidModrinthProject;
        }
    }
}

pub fn validateVersionId(value: []const u8) !void {
    if (value.len == 0 or value.len > 16) return error.InvalidModrinthVersionId;
    for (value) |byte| {
        if (!std.ascii.isAlphanumeric(byte)) return error.InvalidModrinthVersionId;
    }
}

pub fn validateGameVersion(value: []const u8) !void {
    if (value.len == 0 or value.len > 64) return error.InvalidGameVersion;
    for (value) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '.' and byte != '-' and byte != '_' and byte != '+') {
            return error.InvalidGameVersion;
        }
    }
}

fn validateFilename(value: []const u8) !void {
    if (value.len == 0 or value.len > 256) return error.InvalidModrinthFilename;
    if (std.mem.eql(u8, value, ".") or std.mem.eql(u8, value, "..")) return error.InvalidModrinthFilename;
    if (value[value.len - 1] == '.' or value[value.len - 1] == ' ') return error.InvalidModrinthFilename;
    for (value) |byte| {
        if (byte < 0x20 or byte == 0x7f or byte == '/' or byte == '\\' or byte == ':') {
            return error.InvalidModrinthFilename;
        }
    }
    const stem = value[0 .. std.mem.indexOfScalar(u8, value, '.') orelse value.len];
    const reserved = [_][]const u8{
        "CON",  "PRN",  "AUX",  "NUL",  "COM1", "COM2", "COM3", "COM4", "COM5",
        "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5",
        "LPT6", "LPT7", "LPT8", "LPT9",
    };
    for (reserved) |name| {
        if (std.ascii.eqlIgnoreCase(stem, name)) return error.InvalidModrinthFilename;
    }
}

fn validateSha1(value: []const u8) !void {
    if (value.len != 40) return error.InvalidModrinthHash;
    for (value) |byte| if (!std.ascii.isHex(byte)) return error.InvalidModrinthHash;
}

fn validateDownloadUrl(value: []const u8) !void {
    const prefix = "https://";
    if (!std.mem.startsWith(u8, value, prefix)) return error.InsecureModrinthUrl;
    if (value.len > 2048) return error.InvalidModrinthUrl;
    for (value) |byte| {
        if (byte <= 0x20 or byte == 0x7f or byte == '\\' or byte == '"') return error.InvalidModrinthUrl;
    }
}

pub fn projectUrlAlloc(allocator: std.mem.Allocator, project: []const u8) ![]u8 {
    try validateProjectRef(project);
    return std.fmt.allocPrint(allocator, api_root ++ "/v2/project/{s}", .{project});
}

pub fn versionUrlAlloc(allocator: std.mem.Allocator, version_id: []const u8) ![]u8 {
    try validateVersionId(version_id);
    return std.fmt.allocPrint(allocator, api_root ++ "/v2/version/{s}", .{version_id});
}

pub fn projectVersionsUrlAlloc(
    allocator: std.mem.Allocator,
    project: []const u8,
    game_version: []const u8,
    loader: Loader,
) ![]u8 {
    try validateProjectRef(project);
    try validateGameVersion(game_version);
    return std.fmt.allocPrint(
        allocator,
        api_root ++ "/v2/project/{s}/version?game_versions=%5B%22{s}%22%5D&loaders=%5B%22{s}%22%5D",
        .{ project, game_version, loader.name() },
    );
}

pub fn parseProject(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Project) {
    if (source.len > max_project_bytes) return error.ModrinthResponseTooLarge;
    var document = try std.json.parseFromSlice(Project, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_project_bytes,
    });
    errdefer document.deinit();
    try validateProjectRef(document.value.id);
    try validateProjectRef(document.value.slug);
    if (ProjectType.parse(document.value.project_type) == null) return error.UnsupportedProjectType;
    return document;
}

fn validateVersion(version: Version) !void {
    try validateVersionId(version.id);
    try validateVersionId(version.project_id);
    if (VersionType.rank(version.version_type) == null) return error.InvalidModrinthVersionType;
    if (version.game_versions.len == 0 or version.game_versions.len > 128) return error.InvalidModrinthGameVersions;
    for (version.game_versions) |game_version| try validateGameVersion(game_version);
    if (version.loaders.len == 0 or version.loaders.len > 32) return error.InvalidModrinthLoaders;
    for (version.loaders) |loader| {
        if (loader.len == 0 or loader.len > 32) return error.InvalidModrinthLoaders;
        for (loader) |byte| {
            if (!std.ascii.isAlphanumeric(byte) and byte != '-') return error.InvalidModrinthLoaders;
        }
    }
    if (version.files.len == 0 or version.files.len > max_files_per_version) return error.InvalidModrinthFiles;
    for (version.files) |file| {
        try validateFilename(file.filename);
        try validateDownloadUrl(file.url);
        try validateSha1(file.hashes.sha1);
        if (file.size == 0 or file.size > max_file_size) return error.InvalidModrinthFileSize;
    }
    if (version.dependencies.len > max_dependencies_per_version) return error.TooManyModrinthDependencies;
    for (version.dependencies) |dependency| {
        if (dependency.version_id) |id| try validateVersionId(id);
        if (dependency.project_id) |id| try validateVersionId(id);
    }
}

pub fn parseVersion(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Version) {
    if (source.len > max_version_bytes) return error.ModrinthResponseTooLarge;
    var document = try std.json.parseFromSlice(Version, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_version_bytes,
    });
    errdefer document.deinit();
    try validateVersion(document.value);
    return document;
}

pub fn parseVersions(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed([]Version) {
    if (source.len > max_versions_list_bytes) return error.ModrinthResponseTooLarge;
    var document = try std.json.parseFromSlice([]Version, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_versions_list_bytes,
    });
    errdefer document.deinit();
    if (document.value.len > max_versions_per_project) return error.TooManyModrinthVersions;
    for (document.value) |version| try validateVersion(version);
    return document;
}

pub fn versionSupports(version: Version, game_version: []const u8, loader: Loader) bool {
    var game_match = false;
    for (version.game_versions) |candidate| {
        if (std.mem.eql(u8, candidate, game_version)) {
            game_match = true;
            break;
        }
    }
    if (!game_match) return false;
    for (version.loaders) |candidate| {
        if (std.mem.eql(u8, candidate, loader.name())) return true;
    }
    return false;
}

/// Selects the most suitable version from a compatibility-filtered listing.
/// The Modrinth API returns versions newest first; stable releases win over
/// beta and alpha builds.
pub fn selectVersion(versions: []const Version, game_version: []const u8, loader: Loader) ?Version {
    var best: ?Version = null;
    var best_rank: u8 = 255;
    for (versions) |version| {
        if (!versionSupports(version, game_version, loader)) continue;
        const rank = VersionType.rank(version.version_type) orelse continue;
        if (rank < best_rank) {
            best = version;
            best_rank = rank;
            if (rank == 0) break;
        }
    }
    return best;
}

/// Selects the primary file of a version, falling back to the single listed
/// file when nothing is marked primary.
pub fn selectFile(version: Version) ?VersionFile {
    for (version.files) |file| {
        if (file.primary) return file;
    }
    if (version.files.len == 1) return version.files[0];
    return null;
}

test "builds validated API URLs" {
    const allocator = std.testing.allocator;
    const project = try projectUrlAlloc(allocator, "sodium");
    defer allocator.free(project);
    try std.testing.expectEqualStrings("https://api.modrinth.com/v2/project/sodium", project);

    const versions = try projectVersionsUrlAlloc(allocator, "AANobbMI", "1.20.4", .fabric);
    defer allocator.free(versions);
    try std.testing.expectEqualStrings(
        "https://api.modrinth.com/v2/project/AANobbMI/version?game_versions=%5B%221.20.4%22%5D&loaders=%5B%22fabric%22%5D",
        versions,
    );

    try std.testing.expectError(error.InvalidModrinthProject, projectUrlAlloc(allocator, "bad/slug"));
    try std.testing.expectError(error.InvalidModrinthVersionId, versionUrlAlloc(allocator, "bad id"));
    try std.testing.expectError(error.InvalidGameVersion, projectVersionsUrlAlloc(allocator, "sodium", "1.20.4\"", .fabric));
}

const version_fixture =
    \\{
    \\  "id":"XyZaBcDe",
    \\  "project_id":"AANobbMI",
    \\  "version_number":"0.5.11+mc1.20.4",
    \\  "version_type":"release",
    \\  "game_versions":["1.20.4"],
    \\  "loaders":["fabric"],
    \\  "files":[
    \\    {"url":"https://cdn.modrinth.com/data/AANobbMI/versions/XyZaBcDe/sodium-extra.jar","filename":"sodium-extra.jar","primary":false,"hashes":{"sha1":"1111111111111111111111111111111111111111"},"size":100},
    \\    {"url":"https://cdn.modrinth.com/data/AANobbMI/versions/XyZaBcDe/sodium.jar","filename":"sodium.jar","primary":true,"hashes":{"sha1":"2222222222222222222222222222222222222222","sha512":"aa"},"size":200}
    \\  ],
    \\  "dependencies":[
    \\    {"version_id":null,"project_id":"P7dR8mSH","dependency_type":"required"},
    \\    {"version_id":"OpTional1","project_id":null,"dependency_type":"optional"}
    \\  ]
    \\}
;

test "parses and validates a version document" {
    var version = try parseVersion(std.testing.allocator, version_fixture);
    defer version.deinit();
    try std.testing.expectEqualStrings("XyZaBcDe", version.value.id);
    try std.testing.expectEqual(@as(usize, 2), version.value.files.len);
    try std.testing.expectEqual(@as(usize, 2), version.value.dependencies.len);
    try std.testing.expect(versionSupports(version.value, "1.20.4", .fabric));
    try std.testing.expect(!versionSupports(version.value, "1.20.4", .forge));
    try std.testing.expect(!versionSupports(version.value, "1.20.3", .fabric));
    const file = selectFile(version.value).?;
    try std.testing.expectEqualStrings("sodium.jar", file.filename);
    try std.testing.expectEqualStrings("2222222222222222222222222222222222222222", file.hashes.sha1);
}

test "rejects unsafe version documents" {
    const bad_filename =
        \\{"id":"XyZaBcDe","project_id":"AANobbMI","version_number":"1","version_type":"release","game_versions":["1.20.4"],"loaders":["fabric"],"files":[{"url":"https://cdn.modrinth.com/x.jar","filename":"../escape.jar","primary":true,"hashes":{"sha1":"1111111111111111111111111111111111111111"},"size":100}],"dependencies":[]}
    ;
    try std.testing.expectError(error.InvalidModrinthFilename, parseVersion(std.testing.allocator, bad_filename));

    const insecure =
        \\{"id":"XyZaBcDe","project_id":"AANobbMI","version_number":"1","version_type":"release","game_versions":["1.20.4"],"loaders":["fabric"],"files":[{"url":"http://cdn.modrinth.com/x.jar","filename":"x.jar","primary":true,"hashes":{"sha1":"1111111111111111111111111111111111111111"},"size":100}],"dependencies":[]}
    ;
    try std.testing.expectError(error.InsecureModrinthUrl, parseVersion(std.testing.allocator, insecure));

    var resourcepack_project = try parseProject(std.testing.allocator,
        \\{"id":"BvCvBnMk","slug":"default-dark-mode","title":"Dark","project_type":"resourcepack"}
    );
    defer resourcepack_project.deinit();
    try std.testing.expectEqual(ProjectType.resourcepack, ProjectType.parse(resourcepack_project.value.project_type).?);

    try std.testing.expectError(error.UnsupportedProjectType, parseProject(std.testing.allocator,
        \\{"id":"AANobbMI","slug":"pack","title":"Pack","project_type":"shader"}
    ));
    try std.testing.expectError(error.UnsupportedProjectType, parseProject(std.testing.allocator,
        \\{"id":"AANobbMI","slug":"pack","title":"Pack","project_type":"modpack"}
    ));
}

test "loader parsing covers the minecraft pseudo-loader" {
    try std.testing.expectEqual(Loader.minecraft, Loader.parse("minecraft").?);
    try std.testing.expectEqualStrings("minecraft", Loader.minecraft.name());
    try std.testing.expect(Loader.parse("quilt") == null);
}

test "selects a resourcepack version through the minecraft pseudo-loader" {
    const allocator = std.testing.allocator;
    const source =
        \\[{"id":"QwErTyUi","project_id":"BvCvBnMk","version_number":"1.0","version_type":"release","game_versions":["1.21.1"],"loaders":["minecraft"],"files":[{"url":"https://cdn.modrinth.com/data/BvCvBnMk/versions/QwErTyUi/pack.zip","filename":"pack.zip","primary":true,"hashes":{"sha1":"3333333333333333333333333333333333333333"},"size":300}],"dependencies":[]}]
    ;
    var versions = try parseVersions(allocator, source);
    defer versions.deinit();
    const selected = selectVersion(versions.value, "1.21.1", .minecraft).?;
    try std.testing.expectEqualStrings("QwErTyUi", selected.id);
    try std.testing.expect(selectVersion(versions.value, "1.21.1", .fabric) == null);
}

test "selects the newest stable compatible version" {
    const allocator = std.testing.allocator;
    const source = try std.fmt.allocPrint(allocator, "[{s},{s}]", .{ version_fixture, version_fixture });
    defer allocator.free(source);
    var versions = try parseVersions(allocator, source);
    defer versions.deinit();
    const selected = selectVersion(versions.value, "1.20.4", .fabric).?;
    try std.testing.expectEqualStrings("XyZaBcDe", selected.id);
    try std.testing.expect(selectVersion(versions.value, "1.20.4", .neoforge) == null);
}
