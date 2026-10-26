const std = @import("std");
const core = @import("jmcl_core");
pub fn main(init: std.process.Init) !void {
    const a = init.arena.allocator();
    const args = try init.minimal.args.toSlice(a);
    var buf: [4096]u8 = undefined;
    var w: std.Io.File.Writer = .init(.stdout(), init.io, &buf);
    var j: std.json.Stringify = .{ .writer = &w.interface };
    run(init, args, &j) catch |e| try j.write(.{ .@"error" = @errorName(e) });
    try w.interface.writeByte('\n');
    try w.interface.flush();
}
fn run(init: std.process.Init, args: []const []const u8, j: *std.json.Stringify) !void {
    const a = init.arena.allocator();
    const b = try std.Io.Dir.cwd().readFileAlloc(init.io, args[1], a, .limited(2 * 1024 * 1024 + 2));
    const p = try core.curseforge.parseDownloadUrl(a, b);
    defer p.deinit();
    try j.write(p.value.data);
}
