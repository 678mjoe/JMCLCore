const std = @import("std");
const mrpack = @import("core").mrpack;
test "pure pack dependency bytes" {
    const prefix = "{\"formatVersion\":1,\"game\":\"minecraft\",\"name\":\"fixture\",\"files\":[],\"dependencies\":";
    const suffix = "}";
    const parsed = try mrpack.parse(std.testing.allocator, prefix ++ "{\"minecraft\":[49,46,50,48,46,49],\"fabric-loader\":[49,46,50]}" ++ suffix);
    defer parsed.deinit();
    try std.testing.expectEqualStrings("1.20.1", parsed.value.dependencies.minecraft);
    const loader = (try mrpack.loaderSelection(parsed.value.dependencies)).?;
    try std.testing.expectEqualStrings("1.2", loader.version);
    try std.testing.expectError(error.UnsupportedModpackLoader, mrpack.parse(std.testing.allocator, prefix ++ "{\"minecraft\":[255],\"quilt-loader\":[255]}" ++ suffix));
    try std.testing.expectError(error.InvalidModpackLoaderVersion, mrpack.parse(std.testing.allocator, prefix ++ "{\"minecraft\":[255],\"fabric-loader\":[255],\"forge\":\"1\"}" ++ suffix));
    try std.testing.expectError(error.ModpackLoaderConflict, mrpack.parse(std.testing.allocator, prefix ++ "{\"minecraft\":[255],\"fabric-loader\":\"1\",\"forge\":[255]}" ++ suffix));
    try std.testing.expectError(error.InvalidGameVersion, mrpack.parse(std.testing.allocator, prefix ++ "{\"minecraft\":[255]}" ++ suffix));
}
