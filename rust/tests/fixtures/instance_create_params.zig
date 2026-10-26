const std = @import("std");

// EXACT_PINNED_STRUCT_BEGIN
const InstanceCreateParams = struct {
    directory: []const u8,
    id: []const u8,
    name: ?[]const u8 = null,
    version_id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
    source: []const u8 = "official",
    validate: bool = true,
};
// EXACT_PINNED_STRUCT_END

pub fn main(init: std.process.Init) !void {
    const allocator = init.arena.allocator();
    const args = try init.minimal.args.toSlice(allocator);
    const bytes = try std.Io.Dir.cwd().readFileAlloc(init.io, args[1], allocator, .limited(65536));
    var buffer: [4096]u8 = undefined;
    var writer: std.Io.File.Writer = .init(.stdout(), init.io, &buffer);
    var json: std.json.Stringify = .{ .writer = &writer.interface };
    parseOnly(allocator, bytes, &json) catch try json.write(.{ .invalid = true });
    try writer.interface.writeByte('\n');
    try writer.interface.flush();
}
fn parseOnly(allocator: std.mem.Allocator, bytes: []const u8, json: *std.json.Stringify) !void {
    const value = try std.json.parseFromSlice(std.json.Value, allocator, bytes, .{});
    defer value.deinit();
    const parsed = try std.json.parseFromValue(InstanceCreateParams, allocator, value.value, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try json.write(.{ .value = parsed.value });
}
