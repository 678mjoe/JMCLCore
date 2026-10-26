// Calls only the exported cache lookup; no Service, transport or acquisition.
const std = @import("std");
const core = @import("jmcl_core");
pub fn main(init: std.process.Init) !void {
    const a = init.arena.allocator();
    const args = try init.minimal.args.toSlice(a);
    var root = try std.Io.Dir.cwd().openDir(init.io, args[1], .{});
    defer root.close(init.io);
    const now = try std.fmt.parseInt(i64, args[2], 10);
    const max = try std.fmt.parseInt(usize, args[3], 10);
    const policy: core.metadata_cache.Policy = if (std.mem.eql(u8, args[4], "immutable")) .immutable else .{ .ttl_seconds = try std.fmt.parseInt(u64, args[4], 10) };
    var b: [4096]u8 = undefined;
    var w: std.Io.File.Writer = .init(.stdout(), init.io, &b);
    var j: std.json.Stringify = .{ .writer = &w.interface };
    if (core.metadata_cache.lookup(a, init.io, root, "https://fixture.invalid/cache", max, policy, now)) |value| {
        var entry = value;
        defer entry.deinit();
        try j.beginObject();
        try j.objectField("fresh");
        try j.write(entry.fresh);
        try j.objectField("payload");
        try j.beginArray();
        for (entry.payload) |byte| try j.write(byte);
        try j.endArray();
        try j.endObject();
    } else try j.write(null);
    try w.interface.writeByte('\n');
    try w.interface.flush();
}
