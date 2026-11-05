const std = @import("std");
const properties = @import("properties");
pub fn main(init: std.process.Init) !void {
 const a = init.arena.allocator();
 const args = try init.minimal.args.toSlice(a);
 var buffer: [4096]u8 = undefined;
 var file: std.Io.File.Writer = .init(.stdout(), init.io, &buffer);
 const out = &file.interface;
 for (args[1..]) |arg| {
  const bytes = try a.alloc(u8,arg.len/2);
  _ = try std.fmt.hexToBytes(bytes,arg);
  var d = properties.parse(a,bytes) catch |err| {try out.print("error:{s}\n",.{@errorName(err)});continue;};
  defer d.deinit();
  try out.writeAll("ok:");
  for(d.entries.items) |entry| {
   for(entry.key) |b| try out.print("{x:0>2}",.{b});
   try out.writeAll(":");
   for(entry.value) |b| try out.print("{x:0>2}",.{b});
   try out.writeAll(";");
  }
  try out.writeAll("\n");
 }
 try out.flush();
}
