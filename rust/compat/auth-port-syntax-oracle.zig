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
test "pure verification URI decimal port syntax" {
    for ([_][]const u8{"443", "4_43", "4__43", "+443", "-0", "0", "65535"}) |port| {
        const uri = try std.fmt.allocPrint(std.testing.allocator, "https://example.com:{s}/link", .{port});
        defer std.testing.allocator.free(uri);
        try std.testing.expect(validHttpsUri(uri));
    }
    for ([_][]const u8{"_443", "443_", "+_443", "-1", "65536", "", "0x1", "1.0"}) |port| {
        const uri = try std.fmt.allocPrint(std.testing.allocator, "https://example.com:{s}/link", .{port});
        defer std.testing.allocator.free(uri);
        try std.testing.expect(!validHttpsUri(uri));
    }
    for ([_][]const u8{"http://example.com:4_43/link", "https://user@example.com:4_43/link", "https://bad host:4_43/link", "https://:4_43/link"}) |uri| {
        try std.testing.expect(!validHttpsUri(uri));
    }
}
test "pure verification URI percent host buffer limit" {
    const label = "a" ** 63;
    const host = label ++ "." ++ label ++ "." ++ label ++ "." ++ label;
    try std.testing.expect(validHttpsUri("https://" ++ host ++ "/"));
    try std.testing.expect(validHttpsUri("https://" ++ host ++ "./"));
    const encoded = "%61" ++ host[1..];
    try std.testing.expect(validHttpsUri("https://" ++ encoded ++ "/"));
    try std.testing.expect(!validHttpsUri("https://" ++ encoded ++ "./"));
}
