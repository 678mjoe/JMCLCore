const std = @import("std");
pub fn main(init: std.process.Init) !void {
 const args=try init.minimal.args.toSlice(init.arena.allocator());
 var buf:[4096]u8=undefined;
 var out:std.Io.File.Writer=.init(.stdout(),init.io,&buf);
 var json:std.json.Stringify=.{.writer=&out.interface};
 if (std.mem.eql(u8,args[2],"8")) { try run(u8,init,&json,args[1]); } else if (std.mem.eql(u8,args[2],"32")) { try run(u32,init,&json,args[1]); } else { try run(u64,init,&json,args[1]); }
 try out.interface.flush();
}

fn run(comptime T:type, init:std.process.Init, json:*std.json.Stringify, text:[]const u8) !void {
 const result=std.json.parseFromSlice(T,init.arena.allocator(),text,.{}) catch |e| {try json.write(.{.@"error"=@errorName(e)});return;};
 try json.write(.{.value=result.value});
}
