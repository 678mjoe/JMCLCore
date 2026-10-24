// Pure fixture adapter; never accesses network, credentials, or user instances.
const std = @import("std");
const core = @import("jmcl_core");
pub fn main(init: std.process.Init) !void {
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    var buffer: [4096]u8 = undefined;
    var output: std.Io.File.Writer = .init(.stdout(), init.io, &buffer);
    var json: std.json.Stringify = .{ .writer = &output.interface };
    run(init, args, &json) catch |err| try json.write(.{ .@"error" = @errorName(err) });
    try output.interface.writeByte('\n');
    try output.interface.flush();
}
fn run(init: std.process.Init,args: []const []const u8,json:*std.json.Stringify) !void {
    if(args.len!=3) return error.InvalidArguments;
    const a=init.arena.allocator();
    const bytes=try std.Io.Dir.cwd().readFileAlloc(init.io,args[2],a,.limited(20*1024*1024));
    if(std.mem.eql(u8,args[1],"mr")) {
        const p=try core.mrpack.parse(a,bytes);defer p.deinit();
        const loader=try core.mrpack.loaderSelection(p.value.dependencies);
        var shipped:usize=0;for(p.value.files)|f|{if(core.mrpack.shipsToClient(f))shipped+=1;}
        try json.write(.{.name=p.value.name,.version=p.value.dependencies.minecraft,.loader=loader,.shipped=shipped});
    } else if(std.mem.eql(u8,args[1],"project")) {
        const p=try core.curseforge.parseMod(a,bytes);defer p.deinit();
        try json.write(.{.class_id=p.value.data.classId,.slug=p.value.data.slug});
    } else if(std.mem.eql(u8,args[1],"file")) {
        const p=try core.curseforge.parseFile(a,bytes);defer p.deinit();const f=p.value.data;
        try json.write(.{.id=f.id,.project=f.modId,.name=f.fileName,.sha1=core.curseforge.sha1Of(f.*),.size=f.fileLength,.url=f.downloadUrl});
    } else if(std.mem.eql(u8,args[1],"url")) {
        const p=try core.curseforge.parseDownloadUrl(a,bytes);defer p.deinit();try json.write(p.value.data);
    } else {
        const p=try core.curseforge_pack.parse(a,bytes);defer p.deinit();
        const loader=try core.curseforge_pack.loaderSelection(p.value.minecraft);
        var shipped:usize=0;for(p.value.files)|f|{if(f.required)shipped+=1;}
        try json.write(.{.name=p.value.name,.version=p.value.minecraft.version,.loader=loader,.shipped=shipped});
    }
}
