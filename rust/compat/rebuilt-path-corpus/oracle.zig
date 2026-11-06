const std = @import("std");
const core = @import("core");
test "rebuilt_path_oracle_pure_schema_20261106" {
    {
        const result = core.mrpack.parse(std.testing.allocator, @embedFile("mr_opaque.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("mr_opaque\tok\n", .{});
            try std.testing.expectEqualStrings("ok", "ok");
            try std.testing.expectEqual(@as(u8, 255), parsed.value.files[0].path[5]);
        } else |err| {
            std.debug.print("mr_opaque\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("ok", @errorName(err));
        }
    }
    {
        const result = core.mrpack.parse(std.testing.allocator, @embedFile("mr_nul.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("mr_nul\tok\n", .{});
            try std.testing.expectEqualStrings("UnsafePath", "ok");
        } else |err| {
            std.debug.print("mr_nul\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("UnsafePath", @errorName(err));
        }
    }
    {
        const result = core.mrpack.parse(std.testing.allocator, @embedFile("mr_512.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("mr_512\tok\n", .{});
            try std.testing.expectEqualStrings("ok", "ok");
        } else |err| {
            std.debug.print("mr_512\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("ok", @errorName(err));
        }
    }
    {
        const result = core.mrpack.parse(std.testing.allocator, @embedFile("mr_513.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("mr_513\tok\n", .{});
            try std.testing.expectEqualStrings("InvalidModpackFile", "ok");
        } else |err| {
            std.debug.print("mr_513\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("InvalidModpackFile", @errorName(err));
        }
    }
    {
        const result = core.mrpack.parse(std.testing.allocator, @embedFile("mr_skip.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("mr_skip\tok\n", .{});
            try std.testing.expectEqualStrings("ok", "ok");
            try std.testing.expect(!core.mrpack.shipsToClient(parsed.value.files[0]));
        } else |err| {
            std.debug.print("mr_skip\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("ok", @errorName(err));
        }
    }
    {
        const result = core.mrpack.parse(std.testing.allocator, @embedFile("mr_duplicate_before_later_hash.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("mr_duplicate_before_later_hash\tok\n", .{});
            try std.testing.expectEqualStrings("DuplicateModpackFile", "ok");
        } else |err| {
            std.debug.print("mr_duplicate_before_later_hash\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("DuplicateModpackFile", @errorName(err));
        }
    }
    {
        const result = core.mrpack.parse(std.testing.allocator, @embedFile("mr_skipped_bad_hash.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("mr_skipped_bad_hash\tok\n", .{});
            try std.testing.expectEqualStrings("InvalidModrinthHash", "ok");
        } else |err| {
            std.debug.print("mr_skipped_bad_hash\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("InvalidModrinthHash", @errorName(err));
        }
    }
    {
        const result = core.mrpack.parse(std.testing.allocator, @embedFile("mr_path_before_size.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("mr_path_before_size\tok\n", .{});
            try std.testing.expectEqualStrings("UnsafePath", "ok");
        } else |err| {
            std.debug.print("mr_path_before_size\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("UnsafePath", @errorName(err));
        }
    }
    {
        const result = core.mrpack.parse(std.testing.allocator, @embedFile("mr_numeric.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("mr_numeric\tok\n", .{});
            try std.testing.expectEqualStrings("ok", "ok");
        } else |err| {
            std.debug.print("mr_numeric\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("ok", @errorName(err));
        }
    }
    {
        const result = core.mrpack.parse(std.testing.allocator, @embedFile("mr_overflow.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("mr_overflow\tok\n", .{});
            try std.testing.expectEqualStrings("Overflow", "ok");
        } else |err| {
            std.debug.print("mr_overflow\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("Overflow", @errorName(err));
        }
    }
    {
        const result = core.mrpack.parse(std.testing.allocator, @embedFile("mr_fraction.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("mr_fraction\tok\n", .{});
            try std.testing.expectEqualStrings("InvalidNumber", "ok");
        } else |err| {
            std.debug.print("mr_fraction\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("InvalidNumber", @errorName(err));
        }
    }
    {
        const result = core.mrpack.parse(std.testing.allocator, @embedFile("mr_null.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("mr_null\tok\n", .{});
            try std.testing.expectEqualStrings("UnexpectedToken", "ok");
        } else |err| {
            std.debug.print("mr_null\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("UnexpectedToken", @errorName(err));
        }
    }
    {
        const result = core.curseforge.parseFile(std.testing.allocator, @embedFile("cf_opaque.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("cf_opaque\tok\n", .{});
            try std.testing.expectEqualStrings("ok", "ok");
            try std.testing.expectEqual(@as(u8, 255), parsed.value.data.fileName[0]);
        } else |err| {
            std.debug.print("cf_opaque\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("ok", @errorName(err));
        }
    }
    {
        const result = core.curseforge.parseFile(std.testing.allocator, @embedFile("cf_nul.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("cf_nul\tok\n", .{});
            try std.testing.expectEqualStrings("InvalidCurseforgeFilename", "ok");
        } else |err| {
            std.debug.print("cf_nul\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("InvalidCurseforgeFilename", @errorName(err));
        }
    }
    {
        const result = core.curseforge.parseFile(std.testing.allocator, @embedFile("cf_256.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("cf_256\tok\n", .{});
            try std.testing.expectEqualStrings("ok", "ok");
        } else |err| {
            std.debug.print("cf_256\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("ok", @errorName(err));
        }
    }
    {
        const result = core.curseforge.parseFile(std.testing.allocator, @embedFile("cf_257.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("cf_257\tok\n", .{});
            try std.testing.expectEqualStrings("InvalidCurseforgeFilename", "ok");
        } else |err| {
            std.debug.print("cf_257\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("InvalidCurseforgeFilename", @errorName(err));
        }
    }
    {
        const result = core.curseforge.parseFile(std.testing.allocator, @embedFile("cf_name_before_release.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("cf_name_before_release\tok\n", .{});
            try std.testing.expectEqualStrings("InvalidCurseforgeFilename", "ok");
        } else |err| {
            std.debug.print("cf_name_before_release\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("InvalidCurseforgeFilename", @errorName(err));
        }
    }
    {
        const result = core.curseforge.parseFile(std.testing.allocator, @embedFile("cf_id_before_name.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("cf_id_before_name\tok\n", .{});
            try std.testing.expectEqualStrings("InvalidCurseforgeId", "ok");
        } else |err| {
            std.debug.print("cf_id_before_name\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("InvalidCurseforgeId", @errorName(err));
        }
    }
    {
        const result = core.curseforge.parseFile(std.testing.allocator, @embedFile("cf_empty.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("cf_empty\tok\n", .{});
            try std.testing.expectEqualStrings("InvalidCurseforgeFilename", "ok");
        } else |err| {
            std.debug.print("cf_empty\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("InvalidCurseforgeFilename", @errorName(err));
        }
    }
    {
        const result = core.curseforge.parseFile(std.testing.allocator, @embedFile("cf_numeric.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("cf_numeric\tok\n", .{});
            try std.testing.expectEqualStrings("ok", "ok");
        } else |err| {
            std.debug.print("cf_numeric\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("ok", @errorName(err));
        }
    }
    {
        const result = core.curseforge.parseFile(std.testing.allocator, @embedFile("cf_overflow.json"));
        if (result) |parsed_value| {
            var parsed = parsed_value; defer parsed.deinit();
            std.debug.print("cf_overflow\tok\n", .{});
            try std.testing.expectEqualStrings("Overflow", "ok");
        } else |err| {
            std.debug.print("cf_overflow\t{s}\n", .{@errorName(err)});
            try std.testing.expectEqualStrings("Overflow", @errorName(err));
        }
    }
}
