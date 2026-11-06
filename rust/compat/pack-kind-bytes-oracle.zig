const std = @import("std");
const core = @import("core");
test "pure pack discriminator bytes" {
    const raw = "{\"formatVersion\":1,\"game\":[109,105,110,101,99,114,97,102,116],\"name\":\"fixture\",\"files\":[],\"dependencies\":{\"minecraft\":\"1.20.1\"}}";
    const a = try core.mrpack.parse(std.testing.allocator, raw);
    defer a.deinit();
    const bad = try std.mem.replaceOwned(u8, std.testing.allocator, raw, "[109,105,110,101,99,114,97,102,116]", "[255]");
    defer std.testing.allocator.free(bad);
    try std.testing.expectError(error.UnsupportedModpackGame, core.mrpack.parse(std.testing.allocator, bad));
    const bad_format = try std.mem.replaceOwned(u8, std.testing.allocator, bad, "\"formatVersion\":1", "\"formatVersion\":2");
    defer std.testing.allocator.free(bad_format);
    try std.testing.expectError(error.UnsupportedModpackFormat, core.mrpack.parse(std.testing.allocator, bad_format));
    const raw_cf = "{\"manifestVersion\":1,\"manifestType\":[109,105,110,101,99,114,97,102,116,77,111,100,112,97,99,107],\"name\":\"fixture\",\"files\":[],\"minecraft\":{\"version\":\"1.20.1\"}}";
    const b = try core.curseforge_pack.parse(std.testing.allocator, raw_cf);
    defer b.deinit();
    try std.testing.expectError(error.UnsupportedModpackFormat, core.curseforge_pack.parse(std.testing.allocator, "{\"manifestVersion\":1,\"manifestType\":[255],\"name\":\"\",\"files\":[],\"minecraft\":{\"version\":\"1.20.1\"}}"));
}
