const std = @import("std");
const cf = @import("core").curseforge_pack;
test "pure CurseForge game version bytes" {
    const raw = "{\"manifestVersion\":1,\"manifestType\":\"minecraftModpack\",\"name\":\"fixture\",\"files\":[],\"minecraft\":{\"version\":[49,46,50,48,46,49]}}";
    const parsed = try cf.parse(std.testing.allocator, raw);
    defer parsed.deinit();
    try std.testing.expectEqualStrings("1.20.1", parsed.value.minecraft.version);
    try std.testing.expectError(error.InvalidGameVersion, cf.parse(std.testing.allocator, "{\"manifestVersion\":1,\"manifestType\":\"minecraftModpack\",\"name\":\"fixture\",\"files\":[],\"minecraft\":{\"version\":[255],\"modLoaders\":[{\"id\":\"invalid\"}]}}"));
}
