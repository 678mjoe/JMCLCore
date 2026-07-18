const std = @import("std");
const forge = @import("forge.zig");
const neoforge = @import("neoforge.zig");
const neoforge_executor = @import("neoforge_executor.zig");
const zip_reader = @import("zip_reader.zig");

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

fn readExpectedPatchedSha1(
    allocator: std.mem.Allocator,
    io: std.Io,
    minecraft_dir: std.Io.Dir,
    support_relative: []const u8,
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
    const expected = forge.expectedPatchedSha1(install_document.value) orelse return null;
    return try allocator.dupe(u8, expected);
}

test "forge executor exposes the shared processor chain types" {
    try std.testing.expect(Options == neoforge_executor.Options);
    try std.testing.expect(Summary == neoforge_executor.Summary);
    try std.testing.expect(Event == neoforge_executor.Event);
}
