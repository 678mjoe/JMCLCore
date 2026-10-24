//! Offline protocol adapter: network entry points panic, never access a host.
const std=@import("std");const core=@import("jmcl_core");
fn connect(_: ?*anyopaque,_:*const std.Io.net.IpAddress,_:std.Io.net.IpAddress.ConnectOptions) std.Io.net.IpAddress.ConnectError!std.Io.net.Socket {@panic("fixture attempted network connection");}
fn lookup(_: ?*anyopaque,_:std.Io.net.HostName,_:*std.Io.Queue(std.Io.net.HostName.LookupResult),_:std.Io.net.HostName.LookupOptions) std.Io.net.HostName.LookupError!void {@panic("fixture attempted DNS lookup");}
pub fn main(init:std.process.Init)!void{
 const a=init.arena.allocator();const args=try init.minimal.args.toSlice(a);var vtable=init.io.vtable.*;vtable.netConnectIp=connect;vtable.netLookup=lookup;const io:std.Io=.{.userdata=init.io.userdata,.vtable=&vtable};
 var env=std.process.Environ.Map.init(a);defer env.deinit();if(init.environ_map.get("JMCL_FIXTURE_JAVA_BIN")) |path| try env.put("PATH",path);if(std.mem.eql(u8,args[1],"install")){var isolated=init;isolated.io=io;isolated.environ_map=&env;return @import("jmcl_cli").main(isolated);}var service=core.Service.init(a,io,&env);var buffer:[4096]u8=undefined;var out:std.Io.File.Writer=.init(.stdout(),io,&buffer);try core.protocol.handleLine(&service,args[1],&out.interface);try out.interface.flush();
}
