// Fixture-only adapter around pinned Zig installer; temporary roots and loopback
// URLs are supplied by tests, never arbitrary user/account data.
const std = @import("std");
const core = @import("jmcl_core");
const Context = struct {
    writer: *std.Io.Writer,
    fn emit(raw: *anyopaque, event: core.installer.Event) void {
        const self: *Context = @ptrCast(@alignCast(raw));
        var json: std.json.Stringify = .{ .writer = self.writer };
        switch (event) {
            .file => |v| json.write(.{ .event = "file", .file = v }) catch {},
            .retry => |v| json.write(.{ .event = "retry", .retry = .{ .path = v.path, .attempt = v.attempt, .max_attempts = v.max_attempts, .@"error" = v.error_name } }) catch {},
            .progress => |v| json.write(.{ .event = "progress", .progress = v }) catch {},
        }
        self.writer.writeByte('\n') catch {};
    }
};
pub fn main(init: std.process.Init) !void {
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    var buffer: [4096]u8 = undefined;
    var output: std.Io.File.Writer = .init(.stdout(), init.io, &buffer);
    var json: std.json.Stringify = .{ .writer = &output.interface };
    run(init, args, &output.interface, &json) catch |err| try json.write(.{ .@"error" = @errorName(err) });
    try output.interface.writeByte('\n');
    try output.interface.flush();
}
fn run(init: std.process.Init, args: []const []const u8, writer: *std.Io.Writer, json: *std.json.Stringify) !void {
    if (args.len != 4) return error.InvalidArguments;
    const a = init.arena.allocator();
    const Fixture = struct { files: []const core.install_plan.File, total_download_size: u64, workers: u8 = 1, retries: u8 = 0 };
    const bytes = try std.Io.Dir.cwd().readFileAlloc(init.io, args[1], a, .limited(1024 * 1024));
    var parsed = try std.json.parseFromSlice(Fixture, a, bytes, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    var root = try core.installer.openRoot(init.io, args[2]);
    defer root.close(init.io);
    var store = try core.installer.openRoot(init.io, args[3]);
    defer store.close(init.io);
    var context: Context = .{ .writer = writer };
    const summary = try core.installer.execute(a, init.io, null, root, store, .{
        .version_id = "fixture", .source = .official, .target = .{ .os = .linux, .arch = .x86_64 },
        .files = parsed.value.files, .classpath = &.{}, .native_extractions = &.{}, .asset_projections = &.{},
        .total_download_size = parsed.value.total_download_size,
    }, .{ .workers = parsed.value.workers, .retries = parsed.value.retries }, .{ .context = &context, .callback = Context.emit });
    try json.write(.{ .result = summary });
}
