//! Typed integer grammar checks using the same Zig 0.16 parser as pinned Core.
//! No Core processes, filesystem records, Java or network are exercised here.
const std = @import("std");
fn accept(comptime T: type, raw: []const u8, expected: T) !void {
    const parsed = try std.json.parseFromSlice(T, std.testing.allocator, raw, .{});
    defer parsed.deinit();
    try std.testing.expectEqual(expected, parsed.value);
}
fn reject(comptime T: type, raw: []const u8) !void {
    if (std.json.parseFromSlice(T, std.testing.allocator, raw, .{})) |parsed| {
        defer parsed.deinit();
        return error.TestUnexpectedAcceptance;
    } else |_| {}
}
test "signed record boundaries and legacy grammar" {
    try accept(i64, "-9223372036854775808", std.math.minInt(i64));
    try accept(i64, "\"-9223372036854775808.0\"", std.math.minInt(i64));
    try accept(i64, "9223372036854775807", std.math.maxInt(i64));
    try accept(i64, "\"9223372036854775807.0\"", std.math.maxInt(i64));
    try accept(i64, "9007199254740993.0", 9007199254740993);
    try accept(i64, "\"-9007199254740993e0\"", -9007199254740993);
    try accept(i64, "\"-0\"", 0);
    try accept(i64, "-0.0", 0);
    try accept(i64, "\"+1_0\"", 10);
    try accept(i64, "\"-1__0\"", -10);
    try accept(i64, "\"0x1.8p+1\"", 3);
    try accept(i64, "\"-0x1.8p+1\"", -3);
    try accept(i64, "\"1e1\"", 10);
    try accept(i64, "\"3.00000000000000000000000000000000001\"", 3);
    for ([_][]const u8{ "1.0000000000000001", "\"1.0000000000000001\"", "\"9223372036854775807.1\"", "\"9223372036854775808\"", "\"-9223372036854775809\"", "\"-+1\"", "\"+-1\"", "\"--1\"", "\"++1\"", "\" 1\"", "\"1_\"", "\"0x1p0\"", "\"NaN\"", "null", "true", "[]" }) |raw| try reject(i64, raw);
}
test "unsigned record widths retain typed precision" {
    try accept(u64, "18446744073709551615", std.math.maxInt(u64));
    try accept(u64, "\"18446744073709551615.0\"", std.math.maxInt(u64));
    try accept(u64, "\"0x1.8p+1\"", 3);
    try accept(u64, "\"+1__0\"", 10);
    try accept(u64, "-0.0", 0);
    for ([_][]const u8{ "18446744073709551616", "\"18446744073709551615.1\"", "\"-1\"", "3.0000000000000001", "\"3.0000000000000001\"" }) |raw| try reject(u64, raw);
    try accept(u16, "65535", 65535);
    try accept(u16, "\"65535e0\"", 65535);
    try accept(u16, "\"0x1.5p4\"", 21);
    try reject(u16, "65536");
    try reject(u16, "\"65535.0000000000000001\"");
}
