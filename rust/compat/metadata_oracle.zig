// Read-only fixture adapter around the pinned Zig modules; no HTTP or user data.
const std = @import("std");
const core = @import("jmcl_core");
pub fn main(init: std.process.Init) !void {
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    var buffer: [4096]u8 = undefined;
    var output: std.Io.File.Writer = .init(.stdout(), init.io, &buffer);
    var json: std.json.Stringify = .{ .writer = &output.interface };
    run(init, args, &json) catch |err| {
        try json.write(.{ .@"error" = @errorName(err) });
    };
    try output.interface.writeByte('\n');
    try output.interface.flush();
}
fn run(init: std.process.Init, args: []const []const u8, json: *std.json.Stringify) !void {
    const a = init.arena.allocator();
    if (args.len < 3) return error.InvalidArguments;
    const bytes = try std.Io.Dir.cwd().readFileAlloc(init.io, args[2], a, .limited(32 * 1024 * 1024));
    if (std.mem.eql(u8, args[1], "metadata")) {
        var parsed = try core.version_metadata.parse(a, bytes);
        defer parsed.deinit();
        try json.write(parsed.value);
    } else if (std.mem.eql(u8, args[1], "fabric")) {
        var parsed = try core.fabric.parse(a, bytes);
        defer parsed.deinit();
        try json.write(parsed.value);
    } else if (std.mem.eql(u8, args[1], "fabric-merge")) {
        const Fixture = struct { parent: core.version_metadata.Metadata, profile: core.fabric.Profile, integrities: []const core.fabric.LibraryIntegrity };
        var fixture = try std.json.parseFromSlice(Fixture, a, bytes, .{ .ignore_unknown_fields = true });
        defer fixture.deinit();
        var overlay = try core.fabric.merge(a, fixture.value.parent, fixture.value.profile, fixture.value.integrities);
        defer overlay.deinit();
        try json.write(overlay.value);
    } else if (std.mem.eql(u8, args[1], "neo-version")) {
        var parsed = try core.neoforge.parseVersionProfile(a, bytes); defer parsed.deinit(); try json.write(parsed.value);
    } else if (std.mem.eql(u8, args[1], "neo-install")) {
        var parsed = try core.neoforge.parseInstallProfile(a, bytes); defer parsed.deinit(); try json.write(parsed.value);
    } else if (std.mem.eql(u8, args[1], "neo-merge")) {
        const Fixture = struct { parent: core.version_metadata.Metadata, profile: core.neoforge.VersionProfile, patched: []const u8 };
        var fixture = try std.json.parseFromSlice(Fixture,a,bytes,.{.ignore_unknown_fields=true}); defer fixture.deinit();
        var overlay = try core.neoforge.merge(a,fixture.value.parent,fixture.value.profile,fixture.value.patched); defer overlay.deinit(); try json.write(overlay.value);
    } else if (std.mem.eql(u8, args[1], "zip-entry")) {
        const Fixture = struct { archive: []const u8, name: []const u8, max: usize };
        var fixture = try std.json.parseFromSlice(Fixture,a,bytes,.{}); defer fixture.deinit();
        const result = try core.zip_reader.readEntryAlloc(a,fixture.value.archive,fixture.value.name,fixture.value.max); defer if(result) |v| a.free(v); try json.write(result);
    } else if (std.mem.eql(u8, args[1], "forge-legacy")) {
        var parsed = try core.forge_legacy.parseProfile(a,bytes); defer parsed.deinit(); try json.write(parsed.value);
    } else if (std.mem.eql(u8, args[1], "forge-modern")) {
        const Fixture = struct { profile: std.json.Value, patched: []const u8 };
        var fixture = try std.json.parseFromSlice(Fixture,a,bytes,.{}); defer fixture.deinit();
        const profile_bytes = try std.json.Stringify.valueAlloc(a,fixture.value.profile,.{}); defer a.free(profile_bytes);
        var profile = try core.forge.parseVersionProfile(a,profile_bytes,fixture.value.patched); defer profile.deinit(); try json.write(profile.value);
    } else if (std.mem.eql(u8, args[1], "manifest")) {
        var parsed = try core.version_manifest.parse(a, bytes);
        defer parsed.deinit();
        try json.write(parsed.value);
    } else if (std.mem.eql(u8, args[1], "assets")) {
        var parsed = try core.asset_index.parse(a, bytes);
        defer parsed.deinit();
        try json.write(parsed.value);
    } else {
        if (args.len != 7) return error.InvalidArguments;
        var parsed = try core.version_metadata.parse(a, bytes);
        defer parsed.deinit();
        const assets: ?std.json.Parsed(core.asset_index.Index) = if (std.mem.eql(u8, args[3], "-")) null else try core.asset_index.parse(a, try std.Io.Dir.cwd().readFileAlloc(init.io, args[3], a, .limited(32 * 1024 * 1024)));
        defer if (assets) |entry| entry.deinit();
        const source = core.DownloadSource.parse(args[4]) orelse return error.InvalidArguments;
        const target: core.install_plan.Target = .{ .os = core.install_plan.OperatingSystem.parse(args[5]) orelse return error.InvalidArguments, .arch = core.install_plan.Architecture.parse(args[6]) orelse return error.InvalidArguments };
        var plan = if (std.mem.eql(u8, args[1], "server")) try core.install_plan.buildServer(a, parsed.value, source, target) else try core.install_plan.build(a, parsed.value, if (assets) |x| x.value else null, source, target);
        defer plan.deinit();
        try json.write(plan.value);
    }
}
