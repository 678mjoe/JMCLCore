const std = @import("std");
const core = @import("jmcl_core");
pub fn main(init: std.process.Init) !void {
    const args = try init.minimal.args.toSlice(init.arena.allocator());
    var buffer: [4096]u8 = undefined;
    var writer: std.Io.File.Writer = .init(.stdout(), init.io, &buffer);
    var json: std.json.Stringify = .{ .writer = &writer.interface };
    run(init, args, &json) catch |e| try json.write(.{ .@"error" = @errorName(e) });
    try writer.interface.writeByte('\n');
    try writer.interface.flush();
}
fn run(init: std.process.Init, args: []const []const u8, json: *std.json.Stringify) !void {
    const a = init.arena.allocator();
    const bytes = try std.Io.Dir.cwd().readFileAlloc(init.io, args[2], a, .limited(4 * 1024 * 1024));
    if (std.mem.eql(u8, args[1], "project")) {
        const p = try core.curseforge.parseMod(a, bytes); defer p.deinit();
        try json.write(.{ .name = p.value.data.name });
    } else if (std.mem.eql(u8, args[1], "select")) {
        const p = try core.curseforge.parseFilesPage(a, bytes); defer p.deinit();
        const selected = if (std.mem.eql(u8,args[3],"none")) core.curseforge.selectFile(p.value.data) else core.curseforge.selectFileWithExtension(p.value.data,args[3]);
        try json.write(if(selected) |f| f.id else @as(?u32,null));
    } else {
        const p = try core.curseforge.parseFile(a, bytes); defer p.deinit();
        const f = p.value.data;
        if (std.mem.eql(u8,args[1],"supports")) {
            const loader: ?core.modrinth.Loader = if(std.mem.eql(u8,args[4],"none")) null else if(std.mem.eql(u8,args[4],"fabric")) .fabric else if(std.mem.eql(u8,args[4],"forge")) .forge else if(std.mem.eql(u8,args[4],"neoforge")) .neoforge else .minecraft;
            try json.write(core.curseforge.fileSupports(f.*, args[3], loader));
        } else {
            try json.write(.{ .date=f.fileDate, .games=f.gameVersions, .sha1=core.curseforge.sha1Of(f.*) });
        }
    }
}
