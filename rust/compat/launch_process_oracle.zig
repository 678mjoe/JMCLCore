const std=@import("std");const core=@import("jmcl_core");
const Collector=struct{
 json:*std.json.Stringify,
 fn callback(raw:*anyopaque,event:core.game_process.Event)void{const self:*@This()=@ptrCast(@alignCast(raw));switch(event){.started=>{},.log=>|log|{var event_json:std.json.Stringify=.{.writer=self.json.writer};event_json.write(.{.stream=log.stream,.bytes=log.bytes,.end=log.end}) catch unreachable;self.json.writer.writeByte('\n') catch unreachable;}}}
};
pub fn main(init:std.process.Init)!void{
 const a=init.arena.allocator();const args=try init.minimal.args.toSlice(a);var buffer:[4096]u8=undefined;var out:std.Io.File.Writer=.init(.stdout(),init.io,&buffer);var json:std.json.Stringify=.{.writer=&out.interface};var collector=Collector{.json=&json};
 var env=std.process.Environ.Map.init(a);try env.put("HOME","fixture-home");try env.put("FAKE_TOKEN","fake-secret");try env.put("JAVA_TOOL_OPTIONS","bad");
 const r=try core.game_process.run(a,init.io,&env,args[1..],"/tmp",&.{"fake-secret"},.{.context=&collector,.callback=Collector.callback});try json.write(r);try out.interface.writeByte('\n');try out.interface.flush();
}
