const std = @import("std");
const jmcl = @import("jmcl_core");

pub fn writeSourceNotice(writer: *std.Io.Writer, source: jmcl.DownloadSource) std.Io.Writer.Error!void {
    if (source == .bmclapi) {
        try writer.print("# source\t{s}\n", .{source.displayName()});
    }
}
