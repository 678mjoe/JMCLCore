const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;

const StoreGcParams = struct {
    store_directory: []const u8 = core.artifact_store.default_directory,
    dry_run: bool = true,
    min_age_seconds: u64 = 24 * 60 * 60,
};

pub fn writeGcResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params: StoreGcParams = .{};
    var parsed_params: ?std.json.Parsed(StoreGcParams) = null;
    defer if (parsed_params) |parsed| parsed.deinit();
    if (params_value) |value| {
        parsed_params = std.json.parseFromValue(StoreGcParams, service.allocator, value, .{
            .ignore_unknown_fields = true,
        }) catch {
            try writeError(writer, request_id, "INVALID_PARAMS", "store.gc parameters are invalid");
            return;
        };
        params = parsed_params.?.value;
    }
    if (params.store_directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "store.gc store_directory must not be empty");
        return;
    }
    const root = core.artifact_store.openExistingRoot(service.io, params.store_directory) catch {
        try writeError(writer, request_id, "STORE_ROOT_ERROR", "Failed to open the artifact store");
        return;
    };
    defer root.close(service.io);
    const summary = core.artifact_store.collect(service.allocator, service.io, root, .{
        .dry_run = params.dry_run,
        .min_age_seconds = params.min_age_seconds,
    }) catch {
        try writeError(writer, request_id, "STORE_GC_FAILED", "Failed to collect artifact store objects");
        return;
    };
    if (summary.errors != 0) {
        try writeError(writer, request_id, "STORE_GC_PARTIAL", "One or more artifact objects could not be collected safely");
        return;
    }
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("dry_run");
    try json.write(params.dry_run);
    try json.objectField("min_age_seconds");
    try json.write(params.min_age_seconds);
    try json.objectField("summary");
    try json.write(summary);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}
