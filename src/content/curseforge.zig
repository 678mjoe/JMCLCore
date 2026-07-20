const std = @import("std");
const modrinth = @import("modrinth.zig");

pub const api_root = "https://api.curseforge.com";
pub const minecraft_game_id: u32 = 432;
pub const max_project_bytes: usize = 2 * 1024 * 1024;
pub const max_files_page_bytes: usize = 8 * 1024 * 1024;
pub const max_file_bytes: usize = 2 * 1024 * 1024;
pub const max_files_per_project: usize = 512;
pub const max_file_size: u64 = 512 * 1024 * 1024;
pub const files_page_size: u32 = 50;

pub const class_id_mods: u32 = 6;
pub const class_id_resourcepacks: u32 = 12;
pub const class_id_shaderpacks: u32 = 6552;

pub const release_type_release: u8 = 1;
pub const release_type_beta: u8 = 2;
pub const release_type_alpha: u8 = 3;

pub const hash_algo_sha1: u8 = 1;

pub const relation_required: u8 = 3;

pub fn loaderTypeId(loader: modrinth.Loader) ?u8 {
    return switch (loader) {
        .forge => 1,
        .fabric => 4,
        .neoforge => 6,
        .minecraft => null,
    };
}

pub fn loaderGameVersionTag(loader: modrinth.Loader) ?[]const u8 {
    return switch (loader) {
        .forge => "Forge",
        .fabric => "Fabric",
        .neoforge => "NeoForge",
        .minecraft => null,
    };
}

pub const Mod = struct {
    id: u32,
    slug: []const u8,
    name: []const u8,
    classId: u32,
    isAvailable: bool = true,
};

pub const FileHash = struct {
    value: []const u8,
    algo: u8,
};

pub const FileDependency = struct {
    modId: u32,
    relationType: u8,
};

pub const File = struct {
    id: u32,
    modId: u32,
    isAvailable: bool = true,
    fileName: []const u8,
    releaseType: u8,
    hashes: []const FileHash,
    fileDate: []const u8,
    fileLength: u64,
    downloadUrl: ?[]const u8 = null,
    gameVersions: []const []const u8,
    dependencies: []const FileDependency = &.{},
};

pub const Pagination = struct {
    index: u32 = 0,
    pageSize: u32 = 0,
    resultCount: u32 = 0,
    totalCount: u32 = 0,
};

pub fn Envelope(comptime T: type) type {
    return struct {
        data: T,
        pagination: ?Pagination = null,
    };
}

pub fn validateApiKey(value: []const u8) !void {
    if (value.len == 0 or value.len > 512) return error.InvalidCurseforgeApiKey;
    for (value) |byte| {
        if (byte < 0x21 or byte > 0x7e) return error.InvalidCurseforgeApiKey;
    }
}

pub fn validateNumericId(value: []const u8) !void {
    if (value.len == 0 or value.len > 10) return error.InvalidCurseforgeId;
    for (value) |byte| {
        if (!std.ascii.isDigit(byte)) return error.InvalidCurseforgeId;
    }
    const parsed = std.fmt.parseInt(u32, value, 10) catch return error.InvalidCurseforgeId;
    if (parsed == 0) return error.InvalidCurseforgeId;
}

pub fn validateFilename(value: []const u8) !void {
    if (value.len == 0 or value.len > 256) return error.InvalidCurseforgeFilename;
    if (std.mem.eql(u8, value, ".") or std.mem.eql(u8, value, "..")) return error.InvalidCurseforgeFilename;
    if (value[value.len - 1] == '.' or value[value.len - 1] == ' ') return error.InvalidCurseforgeFilename;
    for (value) |byte| {
        if (byte < 0x20 or byte == 0x7f or byte == '/' or byte == '\\' or byte == ':') {
            return error.InvalidCurseforgeFilename;
        }
    }
    const stem = value[0 .. std.mem.indexOfScalar(u8, value, '.') orelse value.len];
    const reserved = [_][]const u8{
        "CON",  "PRN",  "AUX",  "NUL",  "COM1", "COM2", "COM3", "COM4", "COM5",
        "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5",
        "LPT6", "LPT7", "LPT8", "LPT9",
    };
    for (reserved) |name| {
        if (std.ascii.eqlIgnoreCase(stem, name)) return error.InvalidCurseforgeFilename;
    }
}

pub fn validateSha1(value: []const u8) !void {
    if (value.len != 40) return error.InvalidCurseforgeHash;
    for (value) |byte| if (!std.ascii.isHex(byte)) return error.InvalidCurseforgeHash;
}

pub fn validateDownloadUrl(value: []const u8) !void {
    const prefix = "https://";
    if (!std.mem.startsWith(u8, value, prefix)) return error.InsecureCurseforgeUrl;
    if (value.len > 2048) return error.InvalidCurseforgeUrl;
    for (value) |byte| {
        if (byte <= 0x20 or byte == 0x7f or byte == '\\' or byte == '"') return error.InvalidCurseforgeUrl;
    }
}

fn validateMod(mod: Mod) !void {
    if (mod.id == 0) return error.InvalidCurseforgeId;
    if (mod.slug.len == 0 or mod.slug.len > 128) return error.InvalidCurseforgeSlug;
    for (mod.slug) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '-' and byte != '_' and byte != '.') {
            return error.InvalidCurseforgeSlug;
        }
    }
}

fn validateFile(file: File) !void {
    if (file.id == 0 or file.modId == 0) return error.InvalidCurseforgeId;
    try validateFilename(file.fileName);
    switch (file.releaseType) {
        release_type_release, release_type_beta, release_type_alpha => {},
        else => return error.InvalidCurseforgeReleaseType,
    }
    if (file.gameVersions.len == 0 or file.gameVersions.len > 128) return error.InvalidCurseforgeGameVersions;
    for (file.gameVersions) |game_version| {
        if (game_version.len == 0 or game_version.len > 64) return error.InvalidCurseforgeGameVersions;
    }
    var have_sha1 = false;
    if (file.hashes.len > 8) return error.InvalidCurseforgeHash;
    for (file.hashes) |hash| {
        if (hash.algo == hash_algo_sha1) {
            try validateSha1(hash.value);
            have_sha1 = true;
        }
    }
    if (!have_sha1) return error.MissingCurseforgeSha1;
    if (file.fileLength == 0 or file.fileLength > max_file_size) return error.InvalidCurseforgeFileSize;
    if (file.downloadUrl) |url| try validateDownloadUrl(url);
    if (file.dependencies.len > 128) return error.TooManyCurseforgeDependencies;
}

pub fn sha1Of(file: File) []const u8 {
    for (file.hashes) |hash| {
        if (hash.algo == hash_algo_sha1) return hash.value;
    }
    unreachable;
}

pub fn parseMod(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Envelope(*Mod)) {
    if (source.len > max_project_bytes) return error.CurseforgeResponseTooLarge;
    var document = try std.json.parseFromSlice(Envelope(*Mod), allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_project_bytes,
    });
    errdefer document.deinit();
    try validateMod(document.value.data.*);
    return document;
}

pub fn parseFile(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Envelope(*File)) {
    if (source.len > max_file_bytes) return error.CurseforgeResponseTooLarge;
    var document = try std.json.parseFromSlice(Envelope(*File), allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_file_bytes,
    });
    errdefer document.deinit();
    try validateFile(document.value.data.*);
    return document;
}

pub fn parseFilesPage(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Envelope([]File)) {
    if (source.len > max_files_page_bytes) return error.CurseforgeResponseTooLarge;
    var document = try std.json.parseFromSlice(Envelope([]File), allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_files_page_bytes,
    });
    errdefer document.deinit();
    if (document.value.data.len > files_page_size) return error.TooManyCurseforgeFiles;
    for (document.value.data) |file| try validateFile(file);
    return document;
}

pub fn parseDownloadUrl(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Envelope(?[]const u8)) {
    if (source.len > max_file_bytes) return error.CurseforgeResponseTooLarge;
    var document = try std.json.parseFromSlice(Envelope(?[]const u8), allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_file_bytes,
    });
    errdefer document.deinit();
    if (document.value.data) |url| try validateDownloadUrl(url);
    return document;
}

pub fn projectUrlAlloc(allocator: std.mem.Allocator, project_id: []const u8) ![]u8 {
    try validateNumericId(project_id);
    return std.fmt.allocPrint(allocator, api_root ++ "/v1/mods/{s}", .{project_id});
}

pub fn fileUrlAlloc(allocator: std.mem.Allocator, project_id: []const u8, file_id: []const u8) ![]u8 {
    try validateNumericId(project_id);
    try validateNumericId(file_id);
    return std.fmt.allocPrint(allocator, api_root ++ "/v1/mods/{s}/files/{s}", .{ project_id, file_id });
}

pub fn downloadUrlAlloc(allocator: std.mem.Allocator, project_id: []const u8, file_id: []const u8) ![]u8 {
    try validateNumericId(project_id);
    try validateNumericId(file_id);
    return std.fmt.allocPrint(allocator, api_root ++ "/v1/mods/{s}/files/{s}/download-url", .{ project_id, file_id });
}

pub fn filesPageUrlAlloc(
    allocator: std.mem.Allocator,
    project_id: []const u8,
    game_version: []const u8,
    loader: ?modrinth.Loader,
    index: u32,
) ![]u8 {
    try validateNumericId(project_id);
    try modrinth.validateGameVersion(game_version);
    const escaped = try escapeQueryComponentAlloc(allocator, game_version);
    defer allocator.free(escaped);
    if (loader) |value| {
        const type_id = loaderTypeId(value) orelse return error.InvalidContentLoader;
        return std.fmt.allocPrint(
            allocator,
            api_root ++ "/v1/mods/{s}/files?gameVersion={s}&modLoaderType={d}&index={d}&pageSize={d}",
            .{ project_id, escaped, type_id, index, files_page_size },
        );
    }
    return std.fmt.allocPrint(
        allocator,
        api_root ++ "/v1/mods/{s}/files?gameVersion={s}&index={d}&pageSize={d}",
        .{ project_id, escaped, index, files_page_size },
    );
}

fn escapeQueryComponentAlloc(allocator: std.mem.Allocator, value: []const u8) ![]u8 {
    var output: std.ArrayList(u8) = .empty;
    defer output.deinit(allocator);
    for (value) |byte| {
        if (std.ascii.isAlphanumeric(byte) or byte == '-' or byte == '.' or byte == '_' or byte == '~') {
            try output.append(allocator, byte);
        } else {
            try output.print(allocator, "%{X:0>2}", .{byte});
        }
    }
    return output.toOwnedSlice(allocator);
}

pub fn fileSupports(file: File, game_version: []const u8, loader: ?modrinth.Loader) bool {
    var game_match = false;
    const loader_tag = if (loader) |value| loaderGameVersionTag(value) else null;
    var loader_match = loader_tag == null;
    for (file.gameVersions) |candidate| {
        if (std.mem.eql(u8, candidate, game_version)) game_match = true;
        if (loader_tag) |tag| {
            if (std.ascii.eqlIgnoreCase(candidate, tag)) loader_match = true;
        }
    }
    return game_match and loader_match;
}

fn releaseRank(release_type: u8) u8 {
    return switch (release_type) {
        release_type_release => 0,
        release_type_beta => 1,
        release_type_alpha => 2,
        else => 3,
    };
}

/// Selects the most suitable file from a filtered listing: available files
/// only, stable releases win over beta and alpha, newest file date breaks
/// ties. File dates are ISO 8601 timestamps and compare lexicographically.
pub fn selectFile(files: []const File) ?File {
    var best: ?File = null;
    var best_rank: u8 = 255;
    for (files) |file| {
        if (!file.isAvailable) continue;
        const rank = releaseRank(file.releaseType);
        if (rank > best_rank) continue;
        if (best) |current| {
            if (rank == best_rank and std.mem.order(u8, file.fileDate, current.fileDate) != .gt) continue;
        }
        best = file;
        best_rank = rank;
    }
    return best;
}

const mod_fixture =
    \\{"data":{"id":238222,"slug":"jei","name":"Just Enough Items","classId":6,"isAvailable":true}}
;

const file_fixture =
    \\{"id":5123456,"gameId":432,"modId":238222,"isAvailable":true,"displayName":"jei-1.20.4-fabric-17.3.0.49.jar","fileName":"jei-1.20.4-fabric-17.3.0.49.jar","releaseType":1,"fileStatus":4,"hashes":[{"value":"1111111111111111111111111111111111111111","algo":1},{"value":"abc","algo":2}],"fileDate":"2024-01-02T03:04:05.000Z","fileLength":1350000,"downloadCount":1000,"downloadUrl":"https://edge.forgecdn.net/files/5123/456/jei-1.20.4-fabric-17.3.0.49.jar","gameVersions":["1.20.4","Fabric"],"sortableGameVersions":[],"dependencies":[{"modId":306612,"relationType":3},{"modId":400000,"relationType":2}],"fileFingerprint":123456789,"modules":[]}
;

test "parses and validates a mod document" {
    var document = try parseMod(std.testing.allocator, mod_fixture);
    defer document.deinit();
    try std.testing.expectEqual(@as(u32, 238222), document.value.data.id);
    try std.testing.expectEqualStrings("jei", document.value.data.slug);
    try std.testing.expectEqual(@as(u32, class_id_mods), document.value.data.classId);
}

test "parses and validates a file document" {
    const source = try std.fmt.allocPrint(std.testing.allocator, "{{\"data\":{s}}}", .{file_fixture});
    defer std.testing.allocator.free(source);
    var document = try parseFile(std.testing.allocator, source);
    defer document.deinit();
    const file = document.value.data;
    try std.testing.expectEqual(@as(u32, 5123456), file.id);
    try std.testing.expectEqual(@as(u64, 1350000), file.fileLength);
    try std.testing.expectEqualStrings("1111111111111111111111111111111111111111", sha1Of(file.*));
    try std.testing.expectEqual(@as(usize, 2), file.dependencies.len);
    try std.testing.expect(fileSupports(file.*, "1.20.4", .fabric));
    try std.testing.expect(!fileSupports(file.*, "1.20.4", .forge));
    try std.testing.expect(!fileSupports(file.*, "1.20.3", .fabric));
    try std.testing.expect(fileSupports(file.*, "1.20.4", null));
}

test "rejects unsafe file documents" {
    const missing_sha1 =
        \\{"data":{"id":5123456,"modId":238222,"isAvailable":true,"fileName":"x.jar","releaseType":1,"hashes":[{"value":"abc","algo":2}],"fileDate":"2024-01-02T03:04:05Z","fileLength":10,"downloadUrl":"https://edge.forgecdn.net/files/x.jar","gameVersions":["1.20.4"],"dependencies":[]}}
    ;
    try std.testing.expectError(error.MissingCurseforgeSha1, parseFile(std.testing.allocator, missing_sha1));

    const insecure =
        \\{"data":{"id":5123456,"modId":238222,"isAvailable":true,"fileName":"x.jar","releaseType":1,"hashes":[{"value":"1111111111111111111111111111111111111111","algo":1}],"fileDate":"2024-01-02T03:04:05Z","fileLength":10,"downloadUrl":"http://edge.forgecdn.net/files/x.jar","gameVersions":["1.20.4"],"dependencies":[]}}
    ;
    try std.testing.expectError(error.InsecureCurseforgeUrl, parseFile(std.testing.allocator, insecure));

    const escaped_name =
        \\{"data":{"id":5123456,"modId":238222,"isAvailable":true,"fileName":"../escape.jar","releaseType":1,"hashes":[{"value":"1111111111111111111111111111111111111111","algo":1}],"fileDate":"2024-01-02T03:04:05Z","fileLength":10,"downloadUrl":null,"gameVersions":["1.20.4"],"dependencies":[]}}
    ;
    try std.testing.expectError(error.InvalidCurseforgeFilename, parseFile(std.testing.allocator, escaped_name));
}

test "parses a files page with pagination" {
    const source = try std.fmt.allocPrint(
        std.testing.allocator,
        "{{\"data\":[{s}],\"pagination\":{{\"index\":0,\"pageSize\":50,\"resultCount\":1,\"totalCount\":1}}}}",
        .{file_fixture},
    );
    defer std.testing.allocator.free(source);
    var document = try parseFilesPage(std.testing.allocator, source);
    defer document.deinit();
    try std.testing.expectEqual(@as(usize, 1), document.value.data.len);
    try std.testing.expectEqual(@as(u32, 1), document.value.pagination.?.totalCount);
}

test "parses nullable download url responses" {
    var present = try parseDownloadUrl(std.testing.allocator,
        \\{"data":"https://edge.forgecdn.net/files/5123/456/x.jar"}
    );
    defer present.deinit();
    try std.testing.expectEqualStrings("https://edge.forgecdn.net/files/5123/456/x.jar", present.value.data.?);

    var missing = try parseDownloadUrl(std.testing.allocator,
        \\{"data":null}
    );
    defer missing.deinit();
    try std.testing.expect(missing.value.data == null);
}

test "builds validated API URLs" {
    const allocator = std.testing.allocator;
    const project = try projectUrlAlloc(allocator, "238222");
    defer allocator.free(project);
    try std.testing.expectEqualStrings("https://api.curseforge.com/v1/mods/238222", project);

    const file = try fileUrlAlloc(allocator, "238222", "5123456");
    defer allocator.free(file);
    try std.testing.expectEqualStrings("https://api.curseforge.com/v1/mods/238222/files/5123456", file);

    const page = try filesPageUrlAlloc(allocator, "238222", "1.20.4", .fabric, 50);
    defer allocator.free(page);
    try std.testing.expectEqualStrings(
        "https://api.curseforge.com/v1/mods/238222/files?gameVersion=1.20.4&modLoaderType=4&index=50&pageSize=50",
        page,
    );

    const unfiltered = try filesPageUrlAlloc(allocator, "4490", "1.21.4", null, 0);
    defer allocator.free(unfiltered);
    try std.testing.expectEqualStrings(
        "https://api.curseforge.com/v1/mods/4490/files?gameVersion=1.21.4&index=0&pageSize=50",
        unfiltered,
    );

    try std.testing.expectError(error.InvalidCurseforgeId, projectUrlAlloc(allocator, "not-a-number"));
    try std.testing.expectError(error.InvalidCurseforgeId, projectUrlAlloc(allocator, "0"));
    try std.testing.expectError(error.InvalidCurseforgeId, fileUrlAlloc(allocator, "238222", "12 34"));
    try std.testing.expectError(error.InvalidContentLoader, filesPageUrlAlloc(allocator, "238222", "1.20.4", .minecraft, 0));
}

test "validates api keys without pinning a format" {
    try validateApiKey("$2a$10$abcdefghijklmnopqrstuvwxyz0123456789abcd");
    try validateApiKey("token-with-dashes");
    try std.testing.expectError(error.InvalidCurseforgeApiKey, validateApiKey(""));
    try std.testing.expectError(error.InvalidCurseforgeApiKey, validateApiKey("has space"));
    try std.testing.expectError(error.InvalidCurseforgeApiKey, validateApiKey("has\nnewline"));
}

test "selects the newest stable available file" {
    const allocator = std.testing.allocator;
    const source = try std.fmt.allocPrint(
        allocator,
        "{{\"data\":[{s},{s},{s}],\"pagination\":{{\"index\":0,\"pageSize\":50,\"resultCount\":3,\"totalCount\":3}}}}",
        .{ file_fixture, beta_fixture, unavailable_fixture },
    );
    defer allocator.free(source);
    var document = try parseFilesPage(allocator, source);
    defer document.deinit();
    const selected = selectFile(document.value.data).?;
    try std.testing.expectEqual(@as(u32, 5123456), selected.id);
}

const beta_fixture =
    \\{"id":5123457,"gameId":432,"modId":238222,"isAvailable":true,"displayName":"jei-beta.jar","fileName":"jei-beta.jar","releaseType":2,"fileStatus":4,"hashes":[{"value":"2222222222222222222222222222222222222222","algo":1}],"fileDate":"2024-02-02T03:04:05.000Z","fileLength":1400000,"downloadCount":10,"downloadUrl":"https://edge.forgecdn.net/files/5123/457/jei-beta.jar","gameVersions":["1.20.4","Fabric"],"sortableGameVersions":[],"dependencies":[],"fileFingerprint":123456790,"modules":[]}
;

const unavailable_fixture =
    \\{"id":5123458,"gameId":432,"modId":238222,"isAvailable":false,"displayName":"jei-newer.jar","fileName":"jei-newer.jar","releaseType":1,"fileStatus":4,"hashes":[{"value":"3333333333333333333333333333333333333333","algo":1}],"fileDate":"2024-03-02T03:04:05.000Z","fileLength":1500000,"downloadCount":5,"downloadUrl":null,"gameVersions":["1.20.4","Fabric"],"sortableGameVersions":[],"dependencies":[],"fileFingerprint":123456791,"modules":[]}
;
