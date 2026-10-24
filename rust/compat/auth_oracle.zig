// Pinned protocol and auth logic, with only its HTTP adapter import replaced.
const std=@import("std");const core=@import("jmcl_core");const fake=core.authentication_http;
pub fn main(init:std.process.Init)!void{
 const a=init.arena.allocator();const args=try init.minimal.args.toSlice(a);
 const bytes=try std.Io.Dir.cwd().readFileAlloc(init.io,args[1],a,.limited(2*1024*1024));
 const Fixture=struct{request:[]const u8,responses:[]const fake.Step};
 var parsed=try std.json.parseFromSlice(Fixture,a,bytes,.{});defer parsed.deinit();fake.steps=parsed.value.responses;
 const Header=struct{method:[]const u8};var header=try std.json.parseFromSlice(Header,a,parsed.value.request,.{.ignore_unknown_fields=true});defer header.deinit();
 const method=header.value.method;if(!std.mem.eql(u8,method,"auth.microsoft.device.begin") and !std.mem.eql(u8,method,"auth.microsoft.device.poll") and !std.mem.eql(u8,method,"auth.microsoft.refresh") and !std.mem.eql(u8,method,"auth.minecraft.exchange") and !std.mem.eql(u8,method,"auth.minecraft.refresh_exchange"))return error.NotAnAuthFixture;
 var service=core.Service.init(a,init.io,null);var result:std.Io.Writer.Allocating=.init(a);defer result.deinit();
 try core.protocol.handleLine(&service,parsed.value.request,&result.writer);
 var buf:[4096]u8=undefined;var out:std.Io.File.Writer=.init(.stdout(),init.io,&buf);
 try out.interface.writeAll("{\"response\":");try out.interface.writeAll(result.written());try out.interface.writeAll(",\"requests\":");
 var json:std.json.Stringify=.{.writer=&out.interface};try json.write(fake.captured.items);try out.interface.writeAll("}\n");try out.interface.flush();
}
