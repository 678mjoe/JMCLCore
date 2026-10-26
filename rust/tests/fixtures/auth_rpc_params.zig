const std=@import("std");const core=@import("jmcl_core");
const MicrosoftDevicePollParams = struct {
    client_id: []const u8,
    device_code: []const u8,
};
const MicrosoftRefreshParams = struct {
    client_id: []const u8,
    refresh_token: []const u8,
};
pub fn main(init:std.process.Init)!void{
 const a=init.arena.allocator();const args=try init.minimal.args.toSlice(a);const b=try std.Io.Dir.cwd().readFileAlloc(init.io,args[2],a,.limited(65536));
 var buf:[4096]u8=undefined;var w:std.Io.File.Writer=.init(.stdout(),init.io,&buf);var j:std.json.Stringify=.{.writer=&w.interface};
 run(a,args[1],b,&j) catch |e| try j.write(.{.@"error"=@errorName(e)});try w.interface.writeByte('\n');try w.interface.flush();
}
fn output(comptime T:type,a:std.mem.Allocator,v:std.json.Value,j:*std.json.Stringify)!void{const p=try std.json.parseFromValue(T,a,v,.{.ignore_unknown_fields=true});defer p.deinit();try j.write(.{.value=p.value});}
fn run(a:std.mem.Allocator,mode:[]const u8,b:[]const u8,j:*std.json.Stringify)!void{
 const v=try std.json.parseFromSlice(std.json.Value,a,b,.{});defer v.deinit();
 if(std.mem.eql(u8,mode,"0"))return output(core.authentication.Config,a,v.value,j);
 if(std.mem.eql(u8,mode,"1"))return output(MicrosoftDevicePollParams,a,v.value,j);
 if(std.mem.eql(u8,mode,"3"))return output(core.authentication.MicrosoftAccess,a,v.value,j);
 return output(MicrosoftRefreshParams,a,v.value,j);
}
