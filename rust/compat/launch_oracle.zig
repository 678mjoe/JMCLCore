// Synthetic fixtures only. Pinned module adapter, never a production fallback.
const std=@import("std");const core=@import("jmcl_core");
pub fn main(init:std.process.Init)!void{
 const a=init.arena.allocator();const args=try init.minimal.args.toSlice(a);
 const bytes=try std.Io.Dir.cwd().readFileAlloc(init.io,args[1],a,.limited(1024*1024));
 var buffer:[4096]u8=undefined;var out:std.Io.File.Writer=.init(.stdout(),init.io,&buffer);var json:std.json.Stringify=.{.writer=&out.interface};
 run(a,bytes,&json) catch |err| {try json.write(.{.@"error"=@errorName(err)});};try out.interface.writeByte('\n');try out.interface.flush();
}
fn run(a:std.mem.Allocator,bytes:[]const u8,json:*std.json.Stringify)!void{
 var fixture=try std.json.parseFromSlice(core.launch_plan.Input,a,bytes,.{.ignore_unknown_fields=true});defer fixture.deinit();
 var plan=try core.launch_plan.build(a,fixture.value);defer plan.deinit();try json.write(plan.value);
}
