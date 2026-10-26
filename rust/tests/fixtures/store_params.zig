const std = @import("std");
// Verbatim pinned StoreGcParams except source-verified default constant expanded.
const StoreGcParams = struct {
    store_directory: []const u8 = ".jmclcore",
    dry_run: bool = true,
    min_age_seconds: u64 = 24 * 60 * 60,
};
fn failure(j: *std.json.Stringify, message: []const u8) !void {
    try j.write(.{ .code = "INVALID_PARAMS", .message = message });
}
pub fn main(init: std.process.Init) !void {
    const a = init.arena.allocator();
    const args = try init.minimal.args.toSlice(a);
    var buffer: [4096]u8 = undefined;
    var writer: std.Io.File.Writer = .init(.stdout(), init.io, &buffer);
    var json: std.json.Stringify = .{ .writer = &writer.interface };
    var p: StoreGcParams = .{};
    if (!std.mem.eql(u8, args[2], "absent")) {
        const bytes = try std.Io.Dir.cwd().readFileAlloc(init.io, args[1], a, .limited(65536));
        const value = std.json.parseFromSlice(std.json.Value, a, bytes, .{}) catch |e| {
            // Distinct outer dynamic-JSON rejection, never a typed Params result.
            try json.write(.{ .outer_error = @errorName(e) });
            try writer.interface.flush();
            return;
        };
        defer value.deinit();
        const parsed = std.json.parseFromValue(StoreGcParams, a, value.value, .{ .ignore_unknown_fields = true }) catch {
            try failure(&json, "store.gc parameters are invalid");
            try writer.interface.flush();
            return;
        };
        // The arena owns all parsed slices for this fixture invocation.
        p = parsed.value;
    }
    // Source-derived protocol empty guard; no handler is executed.
    if (p.store_directory.len == 0) {
        try failure(&json, "store.gc store_directory must not be empty");
    } else {
        try json.write(.{ .value = p });
    }
    try writer.interface.flush();
}
