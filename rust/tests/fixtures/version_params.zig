// Pure dynamic request-schema adapter. Declarations mirror the private structs
// in pinned protocol/version.zig and protocol/install.zig. No Service is built.
const std = @import("std");
const core = @import("jmcl_core");
const List = struct { type: ?core.version_manifest.VersionType = null, limit: usize = 20, source: core.DownloadSource = .official };
const Get = struct { id: []const u8, source: core.DownloadSource = .official };
const Resolve = struct { id: []const u8, fabric_loader: ?[]const u8 = null, neoforge_version: ?[]const u8 = null, forge_version: ?[]const u8 = null, source: core.DownloadSource = .official };
const Plan = struct { id: []const u8, fabric_loader: ?[]const u8 = null, neoforge_version: ?[]const u8 = null, forge_version: ?[]const u8 = null, source: core.DownloadSource = .official, os: ?core.install_plan.OperatingSystem = null, arch: ?core.install_plan.Architecture = null };
fn emit(comptime T: type, a: std.mem.Allocator, value: std.json.Value, json: *std.json.Stringify) !void {
    const parsed = std.json.parseFromValue(T,a,value,.{.ignore_unknown_fields=true}) catch { try json.write(.{.accepted=false}); return; };
    defer parsed.deinit();
    const p=parsed.value;
    try json.beginObject();
    try json.objectField("accepted"); try json.write(true);
    try json.objectField("source"); try json.write(@tagName(p.source));
    if (T==List) {
        try json.objectField("type"); try json.write(if(p.type) |t| @tagName(t) else @as(?[]const u8,null));
        try json.objectField("limit"); try json.write(p.limit);
    } else {
        try json.objectField("id"); try json.write(p.id);
    }
    if(T==Resolve or T==Plan) {
        try json.objectField("fabric_loader"); try json.write(p.fabric_loader);
        try json.objectField("neoforge_version"); try json.write(p.neoforge_version);
        try json.objectField("forge_version"); try json.write(p.forge_version);
    }
    if(T==Plan) {
        try json.objectField("os"); try json.write(if(p.os) |v| @tagName(v) else @as(?[]const u8,null));
        try json.objectField("arch"); try json.write(if(p.arch) |v| @tagName(v) else @as(?[]const u8,null));
    }
    try json.endObject();
}
pub fn main(init: std.process.Init) !void {
    const a=init.arena.allocator(); const args=try init.minimal.args.toSlice(a);
    const input=try std.Io.Dir.cwd().readFileAlloc(init.io,args[2],a,.limited(8192));
    const value=try std.json.parseFromSlice(std.json.Value,a,input,.{.ignore_unknown_fields=true});
    defer value.deinit();
    var buf:[4096]u8=undefined; var out=std.Io.File.Writer.init(.stdout(),init.io,&buf);
    var json=std.json.Stringify{.writer=&out.interface};
    if(std.mem.eql(u8,args[1],"version.list")) try emit(List,a,value.value,&json)
    else if(std.mem.eql(u8,args[1],"version.get")) try emit(Get,a,value.value,&json)
    else if(std.mem.eql(u8,args[1],"version.resolve")) try emit(Resolve,a,value.value,&json)
    else if(std.mem.eql(u8,args[1],"install.plan")) try emit(Plan,a,value.value,&json)
    else return error.InvalidArguments;
    try out.interface.writeByte('\n'); try out.interface.flush();
}
