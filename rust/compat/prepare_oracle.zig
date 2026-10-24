const std = @import("std");
const core = @import("jmcl_core");
pub fn main(init: std.process.Init) !void {
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    var buffer: [4096]u8 = undefined;
    var output: std.Io.File.Writer = .init(.stdout(), init.io, &buffer);
    var json: std.json.Stringify = .{ .writer = &output.interface };
    run(init, args, &json) catch |err| try json.write(.{ .@"error" = @errorName(err) });
    try output.interface.writeByte('\n');
    try output.interface.flush();
}
fn run(init: std.process.Init, args: []const []const u8, json: *std.json.Stringify) !void {
    if (args.len != 3) return error.InvalidArguments;
    const a = init.arena.allocator();
    const bytes = try std.Io.Dir.cwd().readFileAlloc(init.io, args[1], a, .limited(1024 * 1024));
    var parsed = try std.json.parseFromSlice(core.install_plan.Data, a, bytes, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    var root = try core.installer.openRoot(init.io, args[2]);
    defer root.close(init.io);
    var result = try core.preparer.prepare(a, init.io, root, parsed.value);
    defer result.deinit();
    try json.write(.{ .native_directory = result.native_directory, .native_files = result.native_files, .projections_created = result.projections_created, .projections_cached = result.projections_cached });
}
