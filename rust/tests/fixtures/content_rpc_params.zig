const std=@import("std");const core=@import("jmcl_core");
const ContentInstallParams = struct {
    directory: []const u8,
    minecraft_version: []const u8,
    loader: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    api_key: ?[]const u8 = null,
    project: []const u8,
    version_id: ?[]const u8 = null,
    with_dependencies: bool = true,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};
const ContentSetVersionParams = struct {
    directory: []const u8,
    minecraft_version: []const u8,
    loader: ?[]const u8 = null,
    project: []const u8,
    version_id: []const u8,
    api_key: ?[]const u8 = null,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};
const DatapackInstallParams = struct {
    directory: []const u8,
    world: []const u8,
    minecraft_version: []const u8,
    loader: ?[]const u8 = null,
    provider: ?[]const u8 = null,
    api_key: ?[]const u8 = null,
    project: []const u8,
    version_id: ?[]const u8 = null,
    with_dependencies: bool = true,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};
const DatapackSetVersionParams = struct {
    directory: []const u8,
    world: []const u8,
    minecraft_version: []const u8,
    loader: ?[]const u8 = null,
    project: []const u8,
    version_id: []const u8,
    api_key: ?[]const u8 = null,
    store_directory: []const u8 = core.artifact_store.default_directory,
    workers: u8 = 8,
    retries: u8 = 2,
};
pub fn main(init:std.process.Init)!void{
 const a=init.arena.allocator();const args=try init.minimal.args.toSlice(a);
 const bytes=try std.Io.Dir.cwd().readFileAlloc(init.io,args[2],a,.limited(65536));
 var buf:[4096]u8=undefined;var writer:std.Io.File.Writer=.init(.stdout(),init.io,&buf);var j:std.json.Stringify=.{.writer=&writer.interface};
 run(a,args[1],bytes,&j) catch |e| try j.write(.{.@"error"=@errorName(e)});
 try writer.interface.writeByte('\n');try writer.interface.flush();
}
fn output(comptime T:type,a:std.mem.Allocator,value:std.json.Value,j:*std.json.Stringify)!void{
 const parsed=try std.json.parseFromValue(T,a,value,.{.ignore_unknown_fields=true});defer parsed.deinit();try j.write(.{.value=parsed.value});
}
fn run(a:std.mem.Allocator,mode:[]const u8,bytes:[]const u8,j:*std.json.Stringify)!void{
 const value=try std.json.parseFromSlice(std.json.Value,a,bytes,.{});defer value.deinit();
 if(std.mem.eql(u8,mode,"00"))return output(ContentInstallParams,a,value.value,j);
 if(std.mem.eql(u8,mode,"01"))return output(ContentSetVersionParams,a,value.value,j);
 if(std.mem.eql(u8,mode,"10"))return output(DatapackInstallParams,a,value.value,j);
 return output(DatapackSetVersionParams,a,value.value,j);
}
