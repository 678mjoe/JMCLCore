const std = @import("std");
test "pure dynamic string coercion" {
 const Case = struct { text: []const u8, value: u64 };
 const cases = [_]Case{
  .{.text="1_0",.value=10}, .{.text="+1__0",.value=10}, .{.text="-0",.value=0},
  .{.text="0x1.8p+1",.value=3}, .{.text="9007199254740993.0",.value=9007199254740993},
  .{.text="18446744073709551615.0",.value=std.math.maxInt(u64)},
  .{.text="3.00000000000000000000000000000000001",.value=3},
 };
 for(cases) |case| {
  const p=try std.json.parseFromValue(u64,std.testing.allocator,.{.string=case.text},.{});
  defer p.deinit();
  try std.testing.expectEqual(case.value,p.value);
 }
 for([_][]const u8{"3.0000000000000001","18446744073709551615.1","18446744073709551616","1_","_1","-1","0x1p0","NaN"}) |text| {
  if(std.json.parseFromValue(u64,std.testing.allocator,.{.string=text},.{})) |p| {p.deinit();return error.UnexpectedSuccess;} else |_| {}
 }
 const p=try std.json.parseFromValue(u64,std.testing.allocator,.{.float=3.0000000000000001},.{});
 defer p.deinit();
 try std.testing.expectEqual(@as(u64,3),p.value);
}
