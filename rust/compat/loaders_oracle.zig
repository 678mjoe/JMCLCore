const std=@import("std");const core=@import("jmcl_core");
const Collector=struct{writer:*std.Io.Writer,
 fn callback(raw:*anyopaque,event:core.neoforge_executor.Event)void{const self:*@This()=@ptrCast(@alignCast(raw));switch(event){.processor_started=>|p|{var j:std.json.Stringify=.{.writer=self.writer};j.write(.{.processor=p}) catch unreachable;self.writer.writeByte('\n') catch unreachable;}}}
};
pub fn main(init:std.process.Init)!void{const args=try init.minimal.args.toSlice(init.arena.allocator());var buffer:[4096]u8=undefined;var out:std.Io.File.Writer=.init(.stdout(),init.io,&buffer);var json:std.json.Stringify=.{.writer=&out.interface};run(init,args,&out.interface,&json) catch |err|try json.write(.{.@"error"=@errorName(err)});try out.interface.writeByte('\n');try out.interface.flush();}
fn run(init:std.process.Init,args:[]const []const u8,writer:*std.Io.Writer,json:*std.json.Stringify)!void{
 const a=init.arena.allocator();if(std.mem.eql(u8,args[1],"--path-probe")){return pathProbe(init,args[2],json);}var root=try core.installer.openRoot(init.io,args[2]);defer root.close(init.io);var store=try core.installer.openRoot(init.io,args[3]);defer store.close(init.io);
 var env=std.process.Environ.Map.init(a);defer env.deinit();const explicit=[1][]const u8{args[4]};var collector=Collector{.writer=writer};const reporter:core.neoforge_executor.Reporter=.{.context=&collector,.callback=Collector.callback};const opts:core.neoforge_executor.Options=.{.java_paths=&explicit,.required_major=17};
 const result=if(std.mem.eql(u8,args[1],"forge")) try core.forge_executor.run(a,init.io,&env,root,store,"1.20.4","49.2.8","1.20.4",opts,reporter)
 else if(std.mem.eql(u8,args[1],"forge-server")) try core.forge_executor.runServer(a,init.io,&env,root,store,"1.20.4","49.2.8","1.20.4",opts,reporter)
 else if(std.mem.eql(u8,args[1],"neo-server")) try core.neoforge_executor.runServer(a,init.io,&env,root,store,"21.1.133","1.20.4",opts,reporter)
 else try core.neoforge_executor.run(a,init.io,&env,root,store,"21.1.133","1.20.4",opts,reporter);
 try json.write(result);
}

// Isolated diagnostic only: the actual processor oracle remains unmodified.
fn pathProbe(init:std.process.Init,path:[]const u8,json:*std.json.Stringify)!void{
 const a=init.arena.allocator();var root=try core.installer.openRoot(init.io,path);defer root.close(init.io);
 const absolute=try core.hard_link.absolutePathAlloc(a,init.io,root,".");
 const mixed=try std.fs.path.join(a,&.{absolute,".jmcl-path-probe/nested"});
 const native=try a.dupe(u8,mixed);for(native)|*byte|{if(byte.*=='/')byte.*='\\';}
 const first:[]const u8=if(std.Io.Dir.cwd().createDirPath(init.io,mixed)) |_| "ok" else |err| @errorName(err);
 const second:[]const u8=if(std.Io.Dir.cwd().createDirPath(init.io,native)) |_| "ok" else |err| @errorName(err);
 defer root.deleteTree(init.io,".jmcl-path-probe") catch {};
 try json.write(.{.absolute=absolute,.mixed_path=mixed,.native_path=native,.mixed=first,.native=second});
}
