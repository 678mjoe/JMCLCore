const std = @import("std");
const core = @import("../root.zig");

const auth = @import("auth.zig");
const install = @import("install.zig");
const java = @import("java.zig");
const launch = @import("launch.zig");
const content = @import("content.zig");
const instance = @import("instance.zig");
const server = @import("server.zig");
const modpack = @import("modpack.zig");
const store = @import("store.zig");
const version = @import("version.zig");

const Request = struct {
    protocol: u32,
    id: []const u8,
    method: []const u8,
    params: ?std.json.Value = null,
};

pub const WipingAllocator = struct {
    child: std.mem.Allocator,

    pub fn allocator(wiping: *WipingAllocator) std.mem.Allocator {
        return .{
            .ptr = wiping,
            .vtable = &.{
                .alloc = allocate,
                .resize = resize,
                .remap = remap,
                .free = free,
            },
        };
    }

    fn allocate(raw_context: *anyopaque, len: usize, alignment: std.mem.Alignment, return_address: usize) ?[*]u8 {
        const wiping: *WipingAllocator = @ptrCast(@alignCast(raw_context));
        return wiping.child.rawAlloc(len, alignment, return_address);
    }

    fn resize(raw_context: *anyopaque, memory: []u8, alignment: std.mem.Alignment, new_len: usize, return_address: usize) bool {
        _ = raw_context;
        _ = memory;
        _ = alignment;
        _ = new_len;
        _ = return_address;
        return false;
    }

    fn remap(raw_context: *anyopaque, memory: []u8, alignment: std.mem.Alignment, new_len: usize, return_address: usize) ?[*]u8 {
        _ = raw_context;
        _ = memory;
        _ = alignment;
        _ = new_len;
        _ = return_address;
        return null;
    }

    fn free(raw_context: *anyopaque, memory: []u8, alignment: std.mem.Alignment, return_address: usize) void {
        const wiping: *WipingAllocator = @ptrCast(@alignCast(raw_context));
        std.crypto.secureZero(u8, memory);
        wiping.child.rawFree(memory, alignment, return_address);
    }
};

pub fn handleLine(
    service: *core.Service,
    line: []const u8,
    writer: *std.Io.Writer,
) std.Io.Writer.Error!void {
    var request_allocator: WipingAllocator = .{ .child = service.allocator };
    var parsed = std.json.parseFromSlice(Request, request_allocator.allocator(), line, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, null, "INVALID_REQUEST", "Request must be valid JSON with protocol, id, and method fields");
        return;
    };
    defer parsed.deinit();

    const request = parsed.value;
    if (request.id.len == 0) {
        try writeError(writer, null, "INVALID_REQUEST", "Request id must not be empty");
        return;
    }
    if (request.protocol != core.protocol_version) {
        try writeError(writer, request.id, "UNSUPPORTED_PROTOCOL", "Unsupported protocol version");
        return;
    }

    if (std.mem.eql(u8, request.method, "ping")) {
        try writePingResult(writer, request.id);
    } else if (std.mem.eql(u8, request.method, "core.version")) {
        try writeVersionResult(writer, request.id);
    } else if (std.mem.eql(u8, request.method, "version.list")) {
        try version.writeListResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "version.get")) {
        try version.writeGetResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "install.plan")) {
        try install.writePlanResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "install.execute")) {
        try install.writeExecuteResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "install.prepare")) {
        try install.writePrepareResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "store.gc")) {
        try store.writeGcResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "mods.list")) {
        try content.writeListResultWithKind(.mod, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "mods.install")) {
        try content.writeInstallResultWithKind(.mod, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "mods.set-version")) {
        try content.writeSetVersionResultWithKind(.mod, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "mods.enable")) {
        try content.writeSetEnabledResultWithKind(.mod, service, writer, request.id, request.params, true);
    } else if (std.mem.eql(u8, request.method, "mods.disable")) {
        try content.writeSetEnabledResultWithKind(.mod, service, writer, request.id, request.params, false);
    } else if (std.mem.eql(u8, request.method, "mods.remove")) {
        try content.writeRemoveResultWithKind(.mod, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "mods.adopt")) {
        try content.writeAdoptResultWithKind(.mod, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "resourcepacks.list")) {
        try content.writeListResultWithKind(.resourcepack, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "resourcepacks.install")) {
        try content.writeInstallResultWithKind(.resourcepack, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "resourcepacks.set-version")) {
        try content.writeSetVersionResultWithKind(.resourcepack, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "resourcepacks.enable")) {
        try content.writeSetEnabledResultWithKind(.resourcepack, service, writer, request.id, request.params, true);
    } else if (std.mem.eql(u8, request.method, "resourcepacks.disable")) {
        try content.writeSetEnabledResultWithKind(.resourcepack, service, writer, request.id, request.params, false);
    } else if (std.mem.eql(u8, request.method, "resourcepacks.remove")) {
        try content.writeRemoveResultWithKind(.resourcepack, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "resourcepacks.adopt")) {
        try content.writeAdoptResultWithKind(.resourcepack, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "shaderpacks.list")) {
        try content.writeListResultWithKind(.shaderpack, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "shaderpacks.install")) {
        try content.writeInstallResultWithKind(.shaderpack, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "shaderpacks.set-version")) {
        try content.writeSetVersionResultWithKind(.shaderpack, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "shaderpacks.enable")) {
        try content.writeSetEnabledResultWithKind(.shaderpack, service, writer, request.id, request.params, true);
    } else if (std.mem.eql(u8, request.method, "shaderpacks.disable")) {
        try content.writeSetEnabledResultWithKind(.shaderpack, service, writer, request.id, request.params, false);
    } else if (std.mem.eql(u8, request.method, "shaderpacks.remove")) {
        try content.writeRemoveResultWithKind(.shaderpack, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "shaderpacks.adopt")) {
        try content.writeAdoptResultWithKind(.shaderpack, service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "modpack.install")) {
        try modpack.writeInstallResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "instance.create")) {
        try instance.writeCreateResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "instance.list")) {
        try instance.writeListResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "instance.get")) {
        try instance.writeGetResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "instance.delete")) {
        try instance.writeDeleteResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "server.create")) {
        try server.writeCreateResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "server.list")) {
        try server.writeListResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "server.get")) {
        try server.writeGetResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "server.delete")) {
        try server.writeDeleteResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "java.detect")) {
        try java.writeDetectResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "java.select")) {
        try java.writeSelectResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "launch.plan")) {
        try launch.writePlanResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "launch.execute")) {
        try launch.writeExecuteResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "auth.microsoft.device.begin")) {
        try auth.writeMicrosoftDeviceBeginResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "auth.microsoft.device.poll")) {
        try auth.writeMicrosoftDevicePollResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "auth.microsoft.refresh")) {
        try auth.writeMicrosoftRefreshResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "auth.minecraft.refresh_exchange")) {
        try auth.writeMinecraftRefreshExchangeResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "auth.minecraft.exchange")) {
        try auth.writeMinecraftExchangeResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "account.microsoft.save")) {
        try auth.writeAccountSaveResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "account.microsoft.list")) {
        try auth.writeAccountListResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "account.microsoft.get")) {
        try auth.writeAccountGetResult(service, writer, request.id, request.params);
    } else if (std.mem.eql(u8, request.method, "account.microsoft.delete")) {
        try auth.writeAccountDeleteResult(service, writer, request.id, request.params);
    } else {
        try writeError(writer, request.id, "METHOD_NOT_FOUND", "Unknown RPC method");
    }
}

pub fn writeVersionDocument(writer: *std.Io.Writer) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = writer };
    try json.beginObject();
    try json.objectField("protocol");
    try json.write(core.protocol_version);
    try json.objectField("name");
    try json.write(core.name);
    try json.objectField("version");
    try json.write(core.version);
    try json.endObject();
    try writer.writeByte('\n');
}

fn writePingResult(writer: *std.Io.Writer, id: []const u8) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("pong");
    try json.write(true);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeVersionResult(writer: *std.Io.Writer, id: []const u8) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("name");
    try json.write(core.name);
    try json.objectField("version");
    try json.write(core.version);
    try json.objectField("protocol");
    try json.write(core.protocol_version);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeError(
    writer: *std.Io.Writer,
    id: ?[]const u8,
    code: []const u8,
    message: []const u8,
) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = writer };
    try json.beginObject();
    try json.objectField("protocol");
    try json.write(core.protocol_version);
    try json.objectField("id");
    try json.write(id);
    try json.objectField("event");
    try json.write("error");
    try json.objectField("error");
    try json.beginObject();
    try json.objectField("code");
    try json.write(code);
    try json.objectField("message");
    try json.write(message);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeEventHeader(json: *std.json.Stringify, id: []const u8, event: []const u8) std.Io.Writer.Error!void {
    try json.beginObject();
    try json.objectField("protocol");
    try json.write(core.protocol_version);
    try json.objectField("id");
    try json.write(id);
    try json.objectField("event");
    try json.write(event);
}

pub fn writeSourceField(json: *std.json.Stringify, source: core.DownloadSource) std.Io.Writer.Error!void {
    try json.objectField("source");
    try json.write(@tagName(source));
}

test "ping request produces a result event" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-1\",\"method\":\"ping\"}",
        &output.writer,
    );

    try std.testing.expectEqualStrings(
        "{\"protocol\":1,\"id\":\"test-1\",\"event\":\"result\",\"result\":{\"pong\":true}}\n",
        output.written(),
    );
}

test "unknown methods produce a structured error" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-2\",\"method\":\"missing\"}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "METHOD_NOT_FOUND") != null);
}

test "malformed requests do not expose an id" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(&service, "not-json", &output.writer);

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "\"id\":null") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_REQUEST") != null);
}

test "version list validates its limit before fetching" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-3\",\"method\":\"version.list\",\"params\":{\"limit\":0}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "version get requires a non-empty id before fetching" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-4\",\"method\":\"version.get\",\"params\":{\"id\":\"\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "version list rejects an unknown download source" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-5\",\"method\":\"version.list\",\"params\":{\"source\":\"unknown\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "install plan validates its version id before fetching" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-6\",\"method\":\"install.plan\",\"params\":{\"id\":\"\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "install execute validates its directory before planning" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-7\",\"method\":\"install.execute\",\"params\":{\"id\":\"1.21.11\",\"directory\":\"\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "install prepare validates its directory before planning" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-8\",\"method\":\"install.prepare\",\"params\":{\"id\":\"1.6.4\",\"directory\":\"\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "store GC validates its store directory before collection" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"store-gc\",\"method\":\"store.gc\",\"params\":{\"store_directory\":\"\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "Java select validates its version id before fetching" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-9\",\"method\":\"java.select\",\"params\":{\"id\":\"\"}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "launch plan validates required paths before fetching" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-10\",\"method\":\"launch.plan\",\"params\":{\"id\":\"test\",\"directory\":\"\",\"natives_directory\":\"natives\",\"auth\":{\"player_name\":\"p\",\"uuid\":\"u\",\"access_token\":\"t\"}}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "launch execute validates required paths before fetching" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);

    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"test-11\",\"method\":\"launch.execute\",\"params\":{\"id\":\"test\",\"directory\":\"\",\"auth\":{\"player_name\":\"p\",\"uuid\":\"u\",\"access_token\":\"t\"}}}",
        &output.writer,
    );

    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
}

test "authentication RPC validation never echoes supplied secrets" {
    const cases = [_][]const u8{
        "{\"protocol\":1,\"id\":\"auth-1\",\"method\":\"auth.microsoft.device.begin\",\"params\":{\"client_id\":\"bad\\nclient\"}}",
        "{\"protocol\":1,\"id\":\"auth-2\",\"method\":\"auth.microsoft.device.poll\",\"params\":{\"client_id\":\"\",\"device_code\":\"device-secret\"}}",
        "{\"protocol\":1,\"id\":\"auth-3\",\"method\":\"auth.microsoft.refresh\",\"params\":{\"client_id\":\"\",\"refresh_token\":\"refresh-secret\"}}",
        "{\"protocol\":1,\"id\":\"auth-4\",\"method\":\"auth.minecraft.refresh_exchange\",\"params\":{\"client_id\":\"\",\"refresh_token\":\"combined-refresh-secret\"}}",
        "{\"protocol\":1,\"id\":\"auth-5\",\"method\":\"auth.minecraft.exchange\",\"params\":{\"client_id\":\"\",\"access_token\":\"access-secret\"}}",
    };
    const secrets = [_][]const u8{ "device-secret", "refresh-secret", "combined-refresh-secret", "access-secret", "bad\nclient" };
    for (cases) |line| {
        var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
        defer output.deinit();
        var service = core.Service.init(std.testing.allocator, std.testing.io, null);
        try handleLine(&service, line, &output.writer);
        try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_") != null);
        for (secrets) |secret| {
            try std.testing.expect(std.mem.indexOf(u8, output.written(), secret) == null);
        }
    }
}

test "content install validation never echoes the supplied api key" {
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    const allocator = std.testing.allocator;
    const reaching_line = try std.fmt.allocPrint(allocator, "{{\"protocol\":1,\"id\":\"cf-5\",\"method\":\"shaderpacks.install\",\"params\":{{\"directory\":\".zig-cache/tmp/{s}/mc\",\"minecraft_version\":\"1.21.4\",\"provider\":\"curseforge\",\"api_key\":\"cf-secret-key\",\"project\":\"not-numeric\",\"store_directory\":\".zig-cache/tmp/{s}/store\"}}}}", .{ temporary.sub_path, temporary.sub_path });
    defer allocator.free(reaching_line);
    const cases = [_][]const u8{
        "{\"protocol\":1,\"id\":\"cf-1\",\"method\":\"mods.install\",\"params\":{\"directory\":\"/nonexistent/x\",\"minecraft_version\":\"1.20.4\",\"loader\":\"fabric\",\"provider\":\"curseforge\",\"project\":\"238222\"}}",
        "{\"protocol\":1,\"id\":\"cf-2\",\"method\":\"mods.install\",\"params\":{\"directory\":\"/nonexistent/x\",\"minecraft_version\":\"1.20.4\",\"loader\":\"fabric\",\"provider\":\"curseforge\",\"api_key\":\"bad key\",\"project\":\"238222\"}}",
        "{\"protocol\":1,\"id\":\"cf-3\",\"method\":\"mods.install\",\"params\":{\"directory\":\"/nonexistent/x\",\"minecraft_version\":\"1.20.4\",\"loader\":\"fabric\",\"provider\":\"modrinth\",\"api_key\":\"cf-secret-key\",\"project\":\"sodium\"}}",
        "{\"protocol\":1,\"id\":\"cf-4\",\"method\":\"mods.install\",\"params\":{\"directory\":\"/nonexistent/x\",\"minecraft_version\":\"1.20.4\",\"loader\":\"fabric\",\"provider\":\"unknown\",\"api_key\":\"cf-secret-key\",\"project\":\"238222\"}}",
        reaching_line,
    };
    for (cases) |line| {
        var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
        defer output.deinit();
        var service = core.Service.init(std.testing.allocator, std.testing.io, null);
        try handleLine(&service, line, &output.writer);
        try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
        try std.testing.expect(std.mem.indexOf(u8, output.written(), "cf-secret-key") == null);
        try std.testing.expect(std.mem.indexOf(u8, output.written(), "bad key") == null);
    }
}

test "account registry validation never stores or echoes credentials" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var service = core.Service.init(std.testing.allocator, std.testing.io, null);
    try handleLine(
        &service,
        "{\"protocol\":1,\"id\":\"account-1\",\"method\":\"account.microsoft.save\",\"params\":{\"account\":{\"id\":\"bad\",\"player_name\":\"Player\",\"client_id\":\"client\"},\"access_token\":\"must-not-persist\"}}",
        &output.writer,
    );
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "INVALID_PARAMS") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "must-not-persist") == null);
}

test "server methods reject non-Linux hosts" {
    if (@import("builtin").os.tag == .linux) return error.SkipZigTest;
    const methods = [_][]const u8{
        "server.create",
        "server.list",
        "server.get",
        "server.delete",
    };
    for (methods) |method| {
        var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
        defer output.deinit();
        var service = core.Service.init(std.testing.allocator, std.testing.io, null);
        const request = try std.fmt.allocPrint(
            std.testing.allocator,
            "{{\"protocol\":1,\"id\":\"server-gate\",\"method\":\"{s}\"}}",
            .{method},
        );
        defer std.testing.allocator.free(request);
        try handleLine(&service, request, &output.writer);
        try std.testing.expect(std.mem.indexOf(u8, output.written(), "UNSUPPORTED_PLATFORM") != null);
    }
}

test {
    _ = auth;
    _ = content;
    _ = install;
    _ = instance;
    _ = server;
    _ = java;
    _ = launch;
    _ = modpack;
    _ = store;
    _ = version;
}
