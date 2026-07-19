const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;
const writeSourceField = protocol.writeSourceField;

const JavaDetectParams = struct {
    paths: []const []const u8 = &.{},
};

const JavaSelectParams = struct {
    id: []const u8,
    source: core.DownloadSource = .official,
    arch: ?core.install_plan.Architecture = null,
    paths: []const []const u8 = &.{},
};

pub fn writeDetectResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params: JavaDetectParams = .{};
    var parsed_params: ?std.json.Parsed(JavaDetectParams) = null;
    defer if (parsed_params) |parsed| parsed.deinit();
    if (params_value) |value| {
        parsed_params = std.json.parseFromValue(JavaDetectParams, service.allocator, value, .{
            .ignore_unknown_fields = true,
        }) catch {
            try writeError(writer, request_id, "INVALID_PARAMS", "java.detect paths must be strings");
            return;
        };
        params = parsed_params.?.value;
    }

    var detection = core.java_runtime.detect(
        service.allocator,
        service.io,
        service.environ_map orelse {
            try writeError(writer, request_id, "ENVIRONMENT_UNAVAILABLE", "Process environment is unavailable");
            return;
        },
        params.paths,
    ) catch |err| {
        switch (err) {
            error.JavaNotFound => try writeError(writer, request_id, "JAVA_NOT_FOUND", "No Java executable candidates were found"),
            else => try writeError(writer, request_id, "JAVA_DETECTION_FAILED", "Failed to discover local Java runtimes"),
        }
        return;
    };
    defer detection.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("runtimes");
    try json.write(detection.runtimes);
    try json.objectField("failures");
    try json.write(detection.failures);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeSelectResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "java.select requires a version id");
        return;
    };
    var params = std.json.parseFromValue(JavaSelectParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "java.select parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.id.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "version id must not be empty");
        return;
    }

    var metadata = service.fetchVersionMetadata(params.value.id, params.value.source) catch |err| {
        switch (err) {
            error.VersionNotFound => try writeError(writer, request_id, "VERSION_NOT_FOUND", "The requested version is not in the selected manifest"),
            else => try writeError(writer, request_id, "METADATA_ERROR", "Failed to resolve the Java requirement"),
        }
        return;
    };
    defer metadata.deinit();
    const required_major = core.java_runtime.requiredMajor(metadata.value) catch {
        try writeError(writer, request_id, "INVALID_METADATA", "Version metadata contains an invalid Java requirement");
        return;
    };
    const host = core.install_plan.Target.host() catch {
        try writeError(writer, request_id, "UNSUPPORTED_PLATFORM", "The host platform is not supported");
        return;
    };
    const required_arch = params.value.arch orelse host.arch;

    var detection = core.java_runtime.detect(
        service.allocator,
        service.io,
        service.environ_map orelse {
            try writeError(writer, request_id, "ENVIRONMENT_UNAVAILABLE", "Process environment is unavailable");
            return;
        },
        params.value.paths,
    ) catch {
        try writeError(writer, request_id, "JAVA_NOT_FOUND", "No Java executable candidates were found");
        return;
    };
    defer detection.deinit();
    const runtime = core.java_runtime.selectCompatible(detection, required_major, required_arch) catch {
        try writeError(writer, request_id, "JAVA_INCOMPATIBLE", "No Java runtime matches the required major version and architecture");
        return;
    };

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try writeSourceField(&json, params.value.source);
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("version_id");
    try json.write(params.value.id);
    try json.objectField("required_major");
    try json.write(required_major);
    try json.objectField("required_arch");
    try json.write(required_arch);
    try json.objectField("runtime");
    try json.write(runtime);
    try json.objectField("probe_failures");
    try json.write(detection.failures);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}
