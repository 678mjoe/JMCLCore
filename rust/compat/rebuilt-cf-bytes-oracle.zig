const std = @import("std");
const core = @import("core");
const cf = core.curseforge_pack;
const a = std.testing.allocator;
fn document(loaders: []const u8, overrides: []const u8) ![]u8 {
    return std.fmt.allocPrint(a, "{{\"manifestType\":\"minecraftModpack\",\"manifestVersion\":1,\"name\":\"Pack\",\"files\":[],\"minecraft\":{{\"version\":\"1.20.1\",\"modLoaders\":{s}}},\"overrides\":{s}}}", .{loaders, overrides});
}
test "pure rebuilt CurseForge byte representation" {
    for ([_][]const u8{"fabric", "forge", "neoforge"}) |prefix| {
        for ([_][]const u8{&.{255, 0, '-', 254}, &.{0}, "版"}) |suffix| {
            const id = try std.mem.concat(a, u8, &.{prefix, "-", suffix});
            defer a.free(id);
            const encoded = try std.json.Stringify.valueAlloc(a, &[_]cf.ModLoader{.{.id=id,.primary=true},.{.id="quilt-invalid"}}, .{});
            defer a.free(encoded);
            const raw = try document(encoded,"[255,47,254]");
            defer a.free(raw);
            const parsed = try cf.parse(a,raw);
            defer parsed.deinit();
            const selection = (try cf.loaderSelection(parsed.value.minecraft)).?;
            try std.testing.expectEqualStrings(prefix,@tagName(selection.loader));
            try std.testing.expectEqualSlices(u8,suffix,selection.version);
            try std.testing.expectEqualSlices(u8,&.{255,47,254},parsed.value.overrides.?);
            const dir_prefix = try std.fmt.allocPrint(a,"{s}/",.{parsed.value.overrides.?});
            defer a.free(dir_prefix);
            try std.testing.expectEqualSlices(u8,&.{255,47,254,47},dir_prefix);
            var manifest: core.instance_store.Manifest = .{.id="demo",.name="Pack",.version_id="1.20.1"};
            switch (selection.loader) {
                .fabric => manifest.fabric_loader = selection.version,
                .forge => manifest.forge_version = selection.version,
                .neoforge => manifest.neoforge_version = selection.version,
            }
            const json = try std.json.Stringify.valueAlloc(a,manifest,.{});
            defer a.free(json);
            const back = try std.json.parseFromSlice(@TypeOf(manifest),a,json,.{});
            defer back.deinit();
            try std.testing.expectEqualSlices(u8,suffix,back.value.fabric_loader orelse back.value.forge_version orelse back.value.neoforge_version.?);
            const line = try std.fmt.allocPrint(a,"loader\t{s}\t{s}\n",.{prefix,selection.version});
            defer a.free(line);
            try std.testing.expectEqualSlices(u8,suffix,line[8+prefix.len..line.len-1]);
        }
        const id = try a.alloc(u8,128);
        defer a.free(id);
        @memcpy(id[0..prefix.len],prefix);
        id[prefix.len]='-';
        @memset(id[prefix.len+1..],255);
        const selection = (try cf.loaderSelection(.{.version="1.20.1",.modLoaders=@constCast(&[_]cf.ModLoader{.{.id=id}})})).?;
        try std.testing.expectEqualSlices(u8,id[prefix.len+1..],selection.version);
    }
    const cases = .{
        .{"[{\"id\":[102,97,98,114,105,99,45,255]},{\"id\":\"fabric-x\",\"primary\":true}]","null",error.ModpackLoaderConflict},
        .{"[{\"id\":[102,97,98,114,105,99,45,256]}]","null",error.Overflow},
        .{"[{\"id\":[102,97,98,114,105,99,45,65.00000000000000001]}]","null",error.InvalidNumber},
        .{"[{\"id\":[255,45,1]}]","null",error.UnsupportedModpackLoader},
        .{"[{\"id\":[102,97,98,114,105,99,45]}]","null",error.InvalidModpackLoader},
        .{"[]","[256]",error.Overflow},
        .{"[]","[65.00000000000000001]",error.InvalidNumber},
        .{"[]","[]",error.UnsafePath},
        .{"[]","[255,0]",error.UnsafePath},
        .{"[]","[46,46,47,255]",error.UnsafePath},
        .{"[]","[255,47,46]",error.UnsafePath},
        .{"[]","[255,92,254]",error.UnsafePath},
    };
    inline for (cases) |case| {
        const raw = try document(case[0],case[1]);
        defer a.free(raw);
        try std.testing.expectError(case[2],cf.parse(a,raw));
    }
}
