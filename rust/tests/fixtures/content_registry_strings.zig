const std = @import("std");
const core = @import("jmcl_core");
pub fn main(init: std.process.Init) !void {
    const a = init.arena.allocator();
    const args = try init.minimal.args.toSlice(a);
    var b: [4096]u8 = undefined;
    var w: std.Io.File.Writer = .init(.stdout(), init.io, &b);
    var j: std.json.Stringify = .{ .writer = &w.interface };
    run(init, args, &j) catch |e| try j.write(.{ .@"error" = @errorName(e) });
    try w.interface.writeByte('\n');
    try w.interface.flush();
}
fn run(init: std.process.Init, args: []const []const u8, j: *std.json.Stringify) !void {
    const a = init.arena.allocator();
    const b = try std.Io.Dir.cwd().readFileAlloc(init.io, args[1], a, .limited(256 * 1024));
    const p = try core.content_registry.parse(a, b);
    defer p.deinit();
    try j.write(p.value);
}
