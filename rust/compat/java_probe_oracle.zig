// Explicit synthetic probe only. No candidate discovery or runtime acquisition.
const std = @import("std");
const core = @import("jmcl_core");
pub fn main(init: std.process.Init) !void {
    const allocator = init.arena.allocator();
    const args = try init.minimal.args.toSlice(allocator);
    var environment = std.process.Environ.Map.init(allocator);
    // The executable lookup uses the parent's PATH, not this child environment.
    try environment.put("PATH", "/jmcl-no-child-path");
    try environment.put("JAVA_TOOL_OPTIONS", "must-be-cleared");
    try environment.put("_JAVA_OPTIONS", "must-be-cleared");
    try environment.put("JDK_JAVA_OPTIONS", "must-be-cleared");
    var buffer: [4096]u8 = undefined;
    var output: std.Io.File.Writer = .init(.stdout(), init.io, &buffer);
    var json: std.json.Stringify = .{ .writer = &output.interface };
    const runtime = core.java_runtime.probeExecutable(allocator, allocator, init.io, &environment, args[1], .explicit) catch |err| {
        try json.write(.{ .@"error" = @errorName(err) });
        try output.interface.writeByte('\n');
        try output.interface.flush();
        return;
    };
    try json.write(.{ .result = runtime });
    try output.interface.writeByte('\n');
    try output.interface.flush();
}
