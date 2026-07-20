const std = @import("std");
const modrinth = @import("../content/modrinth.zig");
const install_plan = @import("../install/plan.zig");
const mrpack = @import("mrpack.zig");

pub const manifest_name = "manifest.json";
pub const default_overrides_directory = "overrides";
pub const max_manifest_bytes: usize = 4 * 1024 * 1024;
pub const max_files: usize = 4096;

pub const ModLoader = struct {
    id: []const u8,
    primary: bool = false,
};

pub const Minecraft = struct {
    version: []const u8,
    modLoaders: []const ModLoader = &.{},
};

pub const FileRef = struct {
    projectID: u32,
    fileID: u32,
    required: bool = true,
};

pub const Manifest = struct {
    minecraft: Minecraft,
    manifestType: []const u8,
    manifestVersion: u32,
    name: []const u8,
    version: ?[]const u8 = null,
    author: ?[]const u8 = null,
    files: []FileRef,
    overrides: ?[]const u8 = null,
};

pub fn parse(allocator: std.mem.Allocator, source: []const u8) !std.json.Parsed(Manifest) {
    if (source.len > max_manifest_bytes) return error.ModpackManifestTooLarge;
    var document = try std.json.parseFromSlice(Manifest, allocator, source, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_manifest_bytes,
    });
    errdefer document.deinit();
    try validate(document.value);
    return document;
}

fn validate(manifest: Manifest) !void {
    if (!std.mem.eql(u8, manifest.manifestType, "minecraftModpack")) return error.UnsupportedModpackFormat;
    if (manifest.manifestVersion != 1) return error.UnsupportedModpackFormat;
    if (manifest.name.len == 0 or manifest.name.len > 256) return error.InvalidModpackName;
    if (manifest.version) |version| {
        if (version.len == 0 or version.len > 256) return error.InvalidModpackVersion;
    }
    try modrinth.validateGameVersion(manifest.minecraft.version);
    if (manifest.minecraft.modLoaders.len > 4) return error.InvalidModpackLoader;
    _ = try loaderSelection(manifest.minecraft);
    if (manifest.files.len > max_files) return error.TooManyModpackFiles;
    for (manifest.files) |file| {
        if (file.projectID == 0 or file.fileID == 0) return error.InvalidCurseforgeId;
    }
    const overrides = manifest.overrides orelse default_overrides_directory;
    try install_plan.validateRelativePath(overrides);
}

/// Picks the primary supported mod loader. CurseForge loader ids are
/// `<name>-<version>` pairs such as `forge-47.2.0` or `fabric-0.15.7`.
pub fn loaderSelection(minecraft: Minecraft) !?mrpack.LoaderSelection {
    var selection: ?mrpack.LoaderSelection = null;
    for (minecraft.modLoaders) |mod_loader| {
        if (mod_loader.id.len == 0 or mod_loader.id.len > 128) return error.InvalidModpackLoader;
        const separator = std.mem.indexOfScalar(u8, mod_loader.id, '-') orelse return error.InvalidModpackLoader;
        const name = mod_loader.id[0..separator];
        const version = mod_loader.id[separator + 1 ..];
        if (version.len == 0) return error.InvalidModpackLoader;
        const loader: mrpack.Loader = if (std.mem.eql(u8, name, "fabric"))
            .fabric
        else if (std.mem.eql(u8, name, "forge"))
            .forge
        else if (std.mem.eql(u8, name, "neoforge"))
            .neoforge
        else
            return error.UnsupportedModpackLoader;
        if (selection) |current| {
            if (current.loader != loader or !std.mem.eql(u8, current.version, version)) {
                return error.ModpackLoaderConflict;
            }
        } else {
            selection = .{ .loader = loader, .version = version };
        }
        if (mod_loader.primary) return selection;
    }
    return selection;
}

const manifest_fixture =
    \\{
    \\  "minecraft": {"version":"1.20.1","modLoaders":[{"id":"forge-47.2.0","primary":true}]},
    \\  "manifestType": "minecraftModpack",
    \\  "manifestVersion": 1,
    \\  "name": "CF Test Pack",
    \\  "version": "1.0.0",
    \\  "author": "tester",
    \\  "files": [
    \\    {"projectID":238222,"fileID":5123456,"required":true},
    \\    {"projectID":306612,"fileID":4987654,"required":false}
    \\  ],
    \\  "overrides": "overrides"
    \\}
;

test "parses and validates a curseforge manifest" {
    var document = try parse(std.testing.allocator, manifest_fixture);
    defer document.deinit();
    const manifest = document.value;
    try std.testing.expectEqualStrings("CF Test Pack", manifest.name);
    try std.testing.expectEqual(@as(usize, 2), manifest.files.len);
    const selection = (try loaderSelection(manifest.minecraft)).?;
    try std.testing.expectEqual(mrpack.Loader.forge, selection.loader);
    try std.testing.expectEqualStrings("47.2.0", selection.version);
}

test "rejects invalid manifests" {
    const allocator = std.testing.allocator;
    const wrong_type =
        \\{"minecraft":{"version":"1.20.1","modLoaders":[]},"manifestType":"other","manifestVersion":1,"name":"Pack","files":[]}
    ;
    try std.testing.expectError(error.UnsupportedModpackFormat, parse(allocator, wrong_type));

    const quilt_loader =
        \\{"minecraft":{"version":"1.20.1","modLoaders":[{"id":"quilt-0.24.0","primary":true}]},"manifestType":"minecraftModpack","manifestVersion":1,"name":"Pack","files":[]}
    ;
    try std.testing.expectError(error.UnsupportedModpackLoader, parse(allocator, quilt_loader));

    const missing_separator =
        \\{"minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge","primary":true}]},"manifestType":"minecraftModpack","manifestVersion":1,"name":"Pack","files":[]}
    ;
    try std.testing.expectError(error.InvalidModpackLoader, parse(allocator, missing_separator));

    const zero_id =
        \\{"minecraft":{"version":"1.20.1","modLoaders":[]},"manifestType":"minecraftModpack","manifestVersion":1,"name":"Pack","files":[{"projectID":0,"fileID":1,"required":true}]}
    ;
    try std.testing.expectError(error.InvalidCurseforgeId, parse(allocator, zero_id));

    const unsafe_overrides =
        \\{"minecraft":{"version":"1.20.1","modLoaders":[]},"manifestType":"minecraftModpack","manifestVersion":1,"name":"Pack","files":[],"overrides":"../escape"}
    ;
    try std.testing.expectError(error.UnsafePath, parse(allocator, unsafe_overrides));
}
