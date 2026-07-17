const std = @import("std");

pub const DownloadSource = enum {
    official,
    bmclapi,

    const bmclapi_base = "https://bmclapi2.bangbang93.com";

    pub fn parse(value: []const u8) ?DownloadSource {
        inline for (std.meta.fields(DownloadSource)) |field| {
            if (std.mem.eql(u8, value, field.name)) return @enumFromInt(field.value);
        }
        return null;
    }

    pub fn manifestUrl(source: DownloadSource) []const u8 {
        return switch (source) {
            .official => "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
            .bmclapi => bmclapi_base ++ "/mc/game/version_manifest_v2.json",
        };
    }

    pub fn displayName(source: DownloadSource) []const u8 {
        return switch (source) {
            .official => "Mojang",
            .bmclapi => "BMCLAPI",
        };
    }

    /// Returns an owned mirror URL, or null when the original URL should be used.
    pub fn rewriteUrl(
        source: DownloadSource,
        allocator: std.mem.Allocator,
        original: []const u8,
    ) !?[]u8 {
        if (source == .official or std.mem.startsWith(u8, original, bmclapi_base)) return null;

        const direct_prefixes = [_][]const u8{
            "https://piston-meta.mojang.com/",
            "https://piston-data.mojang.com/",
            "https://launchermeta.mojang.com/",
            "http://launchermeta.mojang.com/",
            "https://launcher.mojang.com/",
            "http://launcher.mojang.com/",
        };
        for (direct_prefixes) |prefix| {
            if (std.mem.startsWith(u8, original, prefix)) {
                return try std.fmt.allocPrint(allocator, "{s}/{s}", .{ bmclapi_base, original[prefix.len..] });
            }
        }

        const asset_prefix = "https://resources.download.minecraft.net/";
        if (std.mem.startsWith(u8, original, asset_prefix)) {
            return try std.fmt.allocPrint(allocator, "{s}/assets/{s}", .{ bmclapi_base, original[asset_prefix.len..] });
        }

        const library_prefix = "https://libraries.minecraft.net/";
        if (std.mem.startsWith(u8, original, library_prefix)) {
            return try std.fmt.allocPrint(allocator, "{s}/maven/{s}", .{ bmclapi_base, original[library_prefix.len..] });
        }

        return null;
    }
};

test "rewrites Mojang URLs for BMCLAPI" {
    const allocator = std.testing.allocator;

    const metadata = (try DownloadSource.bmclapi.rewriteUrl(
        allocator,
        "https://piston-meta.mojang.com/v1/packages/hash/version.json",
    )).?;
    defer allocator.free(metadata);
    try std.testing.expectEqualStrings(
        "https://bmclapi2.bangbang93.com/v1/packages/hash/version.json",
        metadata,
    );

    const asset = (try DownloadSource.bmclapi.rewriteUrl(
        allocator,
        "https://resources.download.minecraft.net/ab/abcdef",
    )).?;
    defer allocator.free(asset);
    try std.testing.expectEqualStrings(
        "https://bmclapi2.bangbang93.com/assets/ab/abcdef",
        asset,
    );

    const library = (try DownloadSource.bmclapi.rewriteUrl(
        allocator,
        "https://libraries.minecraft.net/org/example/demo.jar",
    )).?;
    defer allocator.free(library);
    try std.testing.expectEqualStrings(
        "https://bmclapi2.bangbang93.com/maven/org/example/demo.jar",
        library,
    );
}

test "official source preserves original URLs" {
    try std.testing.expect(try DownloadSource.official.rewriteUrl(
        std.testing.allocator,
        "https://piston-meta.mojang.com/example",
    ) == null);
}
