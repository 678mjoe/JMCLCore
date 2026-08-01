const std = @import("std");
const core = @import("../root.zig");
const version_manifest = @import("../install/version_manifest.zig");
const protocol = @import("protocol.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;
const writeSourceField = protocol.writeSourceField;

const VersionListParams = struct {
    type: ?version_manifest.VersionType = null,
    limit: usize = 20,
    source: core.DownloadSource = .official,
};

const VersionGetParams = struct {
    id: []const u8,
    source: core.DownloadSource = .official,
};

pub fn writeListResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params: VersionListParams = .{};
    var parsed_params: ?std.json.Parsed(VersionListParams) = null;
    defer if (parsed_params) |parsed| parsed.deinit();

    if (params_value) |value| {
        parsed_params = std.json.parseFromValue(VersionListParams, service.allocator, value, .{
            .ignore_unknown_fields = true,
        }) catch {
            try writeError(writer, id, "INVALID_PARAMS", "version.list params must contain a valid type and limit");
            return;
        };
        params = parsed_params.?.value;
    }

    if (params.limit == 0 or params.limit > 1000) {
        try writeError(writer, id, "INVALID_PARAMS", "limit must be between 1 and 1000");
        return;
    }

    var manifest = service.fetchVersionManifest(params.source) catch |err| {
        switch (err) {
            error.MetadataFetchTimeout => try writeError(writer, id, "METADATA_TIMEOUT", "Timed out fetching the version manifest"),
            error.ResponseBodyTooLarge => try writeError(writer, id, "RESPONSE_TOO_LARGE", "Version manifest exceeds the size limit"),
            error.UnsuccessfulHttpStatus => try writeError(writer, id, "HTTP_STATUS_ERROR", "The selected download source returned an unsuccessful HTTP status"),
            error.SyntaxError, error.UnexpectedToken, error.UnknownField, error.InvalidEnumTag, error.MissingField => try writeError(writer, id, "INVALID_MANIFEST", "Mojang returned an invalid version manifest"),
            else => try writeError(writer, id, "NETWORK_ERROR", "Failed to fetch the version manifest from the selected source"),
        }
        return;
    };
    defer manifest.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, id, "result");
    try writeSourceField(&json, params.source);
    try json.objectField("result");
    try version_manifest.writeSelection(&json, manifest.value, .{
        .kind = params.type,
        .limit = params.limit,
    });
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeGetResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "version.get requires an id");
        return;
    };
    var params = std.json.parseFromValue(VersionGetParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "version.get requires a string id");
        return;
    };
    defer params.deinit();

    if (params.value.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "version id must not be empty");
        return;
    }

    var metadata = service.fetchVersionMetadata(params.value.id, params.value.source) catch |err| {
        switch (err) {
            error.MetadataFetchTimeout => try writeError(writer, request_id, "METADATA_TIMEOUT", "Timed out fetching version metadata"),
            error.VersionNotFound => try writeError(writer, request_id, "VERSION_NOT_FOUND", "The requested version is not in the Mojang manifest"),
            error.MetadataHashMismatch => try writeError(writer, request_id, "INTEGRITY_ERROR", "Version metadata failed SHA-1 verification"),
            error.MetadataIdMismatch => try writeError(writer, request_id, "INVALID_METADATA", "Version metadata contains an unexpected id"),
            error.ResponseBodyTooLarge => try writeError(writer, request_id, "RESPONSE_TOO_LARGE", "Mojang metadata exceeds the size limit"),
            error.UnsuccessfulHttpStatus => try writeError(writer, request_id, "HTTP_STATUS_ERROR", "The selected download source returned an unsuccessful HTTP status"),
            error.SyntaxError, error.UnexpectedToken, error.UnknownField, error.InvalidEnumTag, error.MissingField => try writeError(writer, request_id, "INVALID_METADATA", "Mojang returned invalid version metadata"),
            else => try writeError(writer, request_id, "NETWORK_ERROR", "Failed to fetch version metadata from the selected source"),
        }
        return;
    };
    defer metadata.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try writeSourceField(&json, params.value.source);
    try json.objectField("result");
    try json.write(metadata.value);
    try json.endObject();
    try writer.writeByte('\n');
}
