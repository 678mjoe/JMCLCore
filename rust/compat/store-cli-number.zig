// Pure standard parser reference used by pinned src/cli/store.zig.
// No Core import, store, filesystem collection or deletion.
const std = @import("std");
test "pinned decimal unsigned CLI grammar" {
    const good = [_]struct { []const u8, u64 }{
        .{ "0", 0 }, .{ "+0", 0 }, .{ "-0", 0 }, .{ "-00_0", 0 },
        .{ "1__2", 12 }, .{ "+01_2", 12 },
        .{ "18446744073709551615", std.math.maxInt(u64) },
        .{ "00018446744073709551615", std.math.maxInt(u64) },
    };
    for (good) |case| try std.testing.expectEqual(case[1], try std.fmt.parseInt(u64, case[0], 10));
    const bad = [_][]const u8{
        "18446744073709551616", "-1", "+_1", "-_0", "-+0", "-_+0", "++0", "--0", "+-0",
        "", "+", "-", "_", "_0", "0_", "+0_", " 0", "0 ", "0x0", "0e0", "0.0", "０", "1_+2", "0\n",
    };
    for (bad) |raw| {
        if (std.fmt.parseInt(u64, raw, 10)) |_| return error.ExpectedRejection else |_| {}
    }
}
