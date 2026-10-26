const std = @import("std");
pub fn main(init: std.process.Init) !void {
 const args=try init.minimal.args.toSlice(init.arena.allocator());
 const b=try std.Io.Dir.cwd().readFileAlloc(init.io,args[1],init.arena.allocator(),.limited(16384));
 var outbuf:[4096]u8=undefined;var out=std.Io.File.Writer.init(.stdout(),init.io,&outbuf);var j:std.json.Stringify=.{.writer=&out.interface};
 probe(b,init.arena.allocator(),&j) catch |e| try j.write(.{.parse_error=@errorName(e)});
 try out.interface.writeByte('\n');try out.interface.flush();
}
fn probe(b:[]const u8,a:std.mem.Allocator,j:*std.json.Stringify)!void {
 const uri=try std.Uri.parse(b);
 var target:std.Io.Writer.Allocating=.init(a);defer target.deinit();
 try uri.writeToStream(&target.writer,.{.path=true,.query=true});
 var authority:std.Io.Writer.Allocating=.init(a);defer authority.deinit();
 try uri.writeToStream(&authority.writer,.{.authority=true});
 var hostbuf:[std.Io.net.HostName.max_len]u8=undefined;
 const host_error:?[]const u8=if(uri.getHost(&hostbuf)) |_| null else |e| @errorName(e);
 try j.write(.{.scheme=uri.scheme,.target=target.written(),.authority=authority.written(),.host_error=host_error});
}
