const std = @import("std");
// Exact pure URI predicates from pinned src/auth/authentication.zig:1041-1045,
// 1053-1061. No authentication module or transport is invoked.
fn validHttpsUri(value: []const u8) bool {
    if (value.len > 4096) return false;
    const uri = std.Uri.parse(value) catch return false;
    return validHttpsUriParts(uri, true);
}
fn validHttpsUriParts(uri: std.Uri, allow_fragment: bool) bool {
    if (!std.mem.eql(u8, uri.scheme, "https") or uri.user != null or uri.password != null) return false;
    if (!allow_fragment and uri.fragment != null) return false;
    const component = uri.host orelse return false;
    var host_buffer: [std.Io.net.HostName.max_len]u8 = undefined;
    const host = component.toRaw(&host_buffer) catch return false;
    std.Io.net.HostName.validate(host) catch return false;
    return true;
}
pub fn main(init: std.process.Init) !void {
const args = try init.minimal.args.toSlice(init.arena.allocator());
var buffer: [4096]u8 = undefined;
var file: std.Io.File.Writer = .init(.stdout(), init.io, &buffer);
for(args[1..]) |arg| try file.interface.print("{}\n", .{validHttpsUri(arg)});
try file.interface.flush();
}
