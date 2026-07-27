const std = @import("std");
const forge = @import("forge.zig");
const neoforge = @import("neoforge.zig");
const neoforge_executor = @import("neoforge_executor.zig");
const zip_reader = @import("../install/zip_reader.zig");

pub const Options = neoforge_executor.Options;
pub const Summary = neoforge_executor.Summary;
pub const Status = neoforge_executor.Status;
pub const Event = neoforge_executor.Event;
pub const Reporter = neoforge_executor.Reporter;

pub fn run(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: ?*const std.process.Environ.Map,
    minecraft_dir: std.Io.Dir,
    store: std.Io.Dir,
    minecraft_version: []const u8,
    forge_version: []const u8,
    version_id: []const u8,
    options: Options,
    reporter: ?Reporter,
) !Summary {
    try neoforge.validateVersionId(minecraft_version);
    try forge.validateForgeVersion(forge_version);

    const support_relative = try forge.supportDirAlloc(allocator, minecraft_version, forge_version);
    defer allocator.free(support_relative);

    var resolved_options = options;
    const expected_sha1 = try readExpectedPatchedSha1(
        allocator,
        io,
        minecraft_dir,
        support_relative,
        .client,
    );
    defer if (expected_sha1) |sha1| allocator.free(sha1);
    resolved_options.expected_patched_sha1 = expected_sha1;

    return neoforge_executor.runLayout(
        allocator,
        io,
        environ_map,
        minecraft_dir,
        store,
        support_relative,
        version_id,
        resolved_options,
        reporter,
        "forge",
    );
}

pub fn runServer(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: ?*const std.process.Environ.Map,
    server_dir: std.Io.Dir,
    store: std.Io.Dir,
    minecraft_version: []const u8,
    forge_version: []const u8,
    version_id: []const u8,
    options: Options,
    reporter: ?Reporter,
) !Summary {
    try neoforge.validateVersionId(minecraft_version);
    try forge.validateForgeVersion(forge_version);

    const support_relative = try std.fmt.allocPrint(
        allocator,
        ".jmcl/forge-server/{s}-{s}",
        .{ minecraft_version, forge_version },
    );
    defer allocator.free(support_relative);

    var resolved_options = options;
    const expected_sha1 = try readExpectedPatchedSha1(
        allocator,
        io,
        server_dir,
        support_relative,
        .server,
    );
    defer if (expected_sha1) |sha1| allocator.free(sha1);
    resolved_options.expected_patched_sha1 = expected_sha1;
    resolved_options.side = .server;
    resolved_options.minecraft_jar_relative = "server.jar";

    return neoforge_executor.runLayout(
        allocator,
        io,
        environ_map,
        server_dir,
        store,
        support_relative,
        version_id,
        resolved_options,
        reporter,
        "forge",
    );
}

fn readExpectedPatchedSha1(
    allocator: std.mem.Allocator,
    io: std.Io,
    minecraft_dir: std.Io.Dir,
    support_relative: []const u8,
    side: neoforge.Side,
) !?[]const u8 {
    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();
    const scratch = arena.allocator();

    const installer_relative = try std.fmt.allocPrint(scratch, "{s}/installer.jar", .{support_relative});
    const installer_bytes = minecraft_dir.readFileAlloc(io, installer_relative, scratch, .limited(neoforge.max_installer_bytes + 1)) catch |err| switch (err) {
        error.FileNotFound => return error.InstallerArchiveMissing,
        error.StreamTooLong => return error.InstallerArchiveTooLarge,
        else => |other| return other,
    };
    if (installer_bytes.len > neoforge.max_installer_bytes) return error.InstallerArchiveTooLarge;

    const install_source = (try zip_reader.readEntryAlloc(
        scratch,
        installer_bytes,
        "install_profile.json",
        neoforge.max_install_profile_bytes,
    )) orelse return error.ForgeInstallProfileMissing;
    var install_document = try forge.parseInstallProfile(scratch, install_source);
    defer install_document.deinit();
    const expected = (try expectedPatchedSha1ForSide(install_document.value, side)) orelse return null;
    return try allocator.dupe(u8, expected);
}

fn expectedPatchedSha1ForSide(profile: neoforge.InstallProfile, side: neoforge.Side) !?[]const u8 {
    switch (side) {
        .client => return forge.expectedPatchedSha1(profile),
        .server => {
            const entry = profile.data.map.get("PATCHED_SHA") orelse return error.PatchedServerShaMissing;
            const raw = entry.server orelse return error.PatchedServerShaMissing;
            const literal = switch (neoforge.classifyDataValue(raw)) {
                .literal => |value| value,
                else => return error.PatchedServerShaMissing,
            };
            try neoforge.validateSha1(literal);
            return literal;
        },
    }
}

test "forge executor exposes the shared processor chain types" {
    try std.testing.expect(Options == neoforge_executor.Options);
    try std.testing.expect(Summary == neoforge_executor.Summary);
    try std.testing.expect(Event == neoforge_executor.Event);
}

test "extracts the declared patched hash for the requested side" {
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
    var profile = try forge.parseInstallProfile(std.testing.allocator, source);
    defer profile.deinit();
    try std.testing.expectEqualStrings(
        "9a32f7ac703db4fc1374c215b087a95c0637a1f7",
        (try expectedPatchedSha1ForSide(profile.value, .client)).?,
    );
    try std.testing.expectEqualStrings(
        "f90a8b7496366a80967d92b253a7fef0e9adeddf",
        (try expectedPatchedSha1ForSide(profile.value, .server)).?,
    );
}

test "rejects a profile without a server patched hash" {
    const allocator = std.testing.allocator;
    const client_only_source =
        \\{
        \\  "spec":1,
        \\  "version":"1.20.4-forge-49.2.8",
        \\  "minecraft":"1.20.4",
        \\  "data":{
        \\    "PATCHED":{"client":"[net.minecraftforge:forge:1.20.4-49.2.8:client]","server":"[net.minecraftforge:forge:1.20.4-49.2.8:server]"},
        \\    "PATCHED_SHA":{"client":"'9a32f7ac703db4fc1374c215b087a95c0637a1f7'"}
        \\  },
        \\  "processors":[],
        \\  "libraries":[]
        \\}
    ;
    var client_only = try forge.parseInstallProfile(allocator, client_only_source);
    defer client_only.deinit();
    try std.testing.expectError(
        error.PatchedServerShaMissing,
        expectedPatchedSha1ForSide(client_only.value, .server),
    );
    try std.testing.expectEqualStrings(
        "9a32f7ac703db4fc1374c215b087a95c0637a1f7",
        (try expectedPatchedSha1ForSide(client_only.value, .client)).?,
    );

    const absent_source =
        \\{
        \\  "spec":1,
        \\  "version":"1.20.4-forge-49.2.8",
        \\  "minecraft":"1.20.4",
        \\  "data":{
        \\    "PATCHED":{"client":"[net.minecraftforge:forge:1.20.4-49.2.8:client]","server":"[net.minecraftforge:forge:1.20.4-49.2.8:server]"}
        \\  },
        \\  "processors":[],
        \\  "libraries":[]
        \\}
    ;
    var absent = try forge.parseInstallProfile(allocator, absent_source);
    defer absent.deinit();
    try std.testing.expectError(
        error.PatchedServerShaMissing,
        expectedPatchedSha1ForSide(absent.value, .server),
    );
    try std.testing.expect((try expectedPatchedSha1ForSide(absent.value, .client)) == null);
}
