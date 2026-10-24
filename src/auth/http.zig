const std = @import("std");
const authentication = @import("authentication.zig");

pub const StdHttpTransport = struct {
    io: std.Io,
    environ_map: ?*const std.process.Environ.Map,
    user_agent: []const u8 = "jmcl-core/0.1.0-dev",

    const RequestData = struct {
        url: []const u8,
        method: std.http.Method,
        payload: ?[]const u8,
        content_type: ?[]const u8,
        bearer_token: ?[]const u8,
        max_response_bytes: usize,
    };

    pub fn transport(transport_adapter: *StdHttpTransport) authentication.Transport {
        return .{
            .context = transport_adapter,
            .request_fn = request,
        };
    }

    fn request(
        raw_context: *anyopaque,
        allocator: std.mem.Allocator,
        http_request: authentication.HttpRequest,
    ) authentication.TransportError!authentication.HttpResponse {
        const context: *StdHttpTransport = @ptrCast(@alignCast(raw_context));
        const timeout_ms = requestTimeout(http_request);
        if (timeout_ms == 0) return error.Timeout;

        const InternalError = authentication.TransportError || error{Canceled};
        const InternalResult = InternalError!authentication.HttpResponse;
        const Outcome = union(enum) {
            request: InternalResult,
            timeout: bool,
        };
        var outcome_buffer: [2]Outcome = undefined;
        var select = std.Io.Select(Outcome).init(context.io, &outcome_buffer);
        select.async(.request, fetch, .{ context, allocator, http_request });
        select.async(.timeout, waitForTimeout, .{ context.io, timeout_ms });

        const first = select.await() catch {
            drain(&select, allocator);
            return error.Canceled;
        };
        switch (first) {
            .request => |result| {
                drain(&select, allocator);
                return result catch |err| switch (err) {
                    error.Canceled => error.Canceled,
                    else => |other| other,
                };
            },
            .timeout => |expired| {
                drain(&select, allocator);
                return if (expired) error.Timeout else error.NetworkError;
            },
        }
    }

    fn fetch(
        context: *StdHttpTransport,
        allocator: std.mem.Allocator,
        http_request: authentication.HttpRequest,
    ) (authentication.TransportError || error{Canceled})!authentication.HttpResponse {
        var proxy_arena = std.heap.ArenaAllocator.init(allocator);
        defer proxy_arena.deinit();

        var client: std.http.Client = .{
            .allocator = allocator,
            .io = context.io,
        };
        defer client.deinit();
        if (context.environ_map) |environment| {
            client.initDefaultProxies(proxy_arena.allocator(), environment) catch |err| return mapFetchError(err);
        }

        const request_data: RequestData = switch (http_request) {
            .form_post => |value| .{
                .url = value.url,
                .method = std.http.Method.POST,
                .payload = @as(?[]const u8, value.body),
                .content_type = @as(?[]const u8, "application/x-www-form-urlencoded"),
                .bearer_token = @as(?[]const u8, null),
                .max_response_bytes = value.max_response_bytes,
            },
            .json_post => |value| .{
                .url = value.url,
                .method = std.http.Method.POST,
                .payload = @as(?[]const u8, value.body),
                .content_type = @as(?[]const u8, "application/json"),
                .bearer_token = @as(?[]const u8, null),
                .max_response_bytes = value.max_response_bytes,
            },
            .bearer_get => |value| .{
                .url = value.url,
                .method = std.http.Method.GET,
                .payload = @as(?[]const u8, null),
                .content_type = @as(?[]const u8, null),
                .bearer_token = @as(?[]const u8, value.bearer_token),
                .max_response_bytes = value.max_response_bytes,
            },
        };
        const authorization = if (request_data.bearer_token) |token| authorization: {
            for (token) |byte| {
                if (byte < 0x21 or byte > 0x7e) return error.InvalidResponse;
            }
            break :authorization try std.fmt.allocPrint(allocator, "Bearer {s}", .{token});
        } else null;
        defer if (authorization) |value| {
            std.crypto.secureZero(u8, value);
            allocator.free(value);
        };

        const storage_len = std.math.add(usize, request_data.max_response_bytes, 1) catch return error.ResponseTooLarge;
        const storage = try allocator.alloc(u8, storage_len);
        errdefer {
            std.crypto.secureZero(u8, storage);
            allocator.free(storage);
        }
        var body = std.Io.Writer.fixed(storage);
        const result = client.fetch(.{
            .location = .{ .url = request_data.url },
            .method = request_data.method,
            .payload = request_data.payload,
            .response_writer = &body,
            .redirect_behavior = .unhandled,
            .headers = .{
                .authorization = if (authorization) |value| .{ .override = value } else .default,
                .content_type = if (request_data.content_type) |value| .{ .override = value } else .omit,
                .user_agent = .{ .override = context.user_agent },
                .accept_encoding = .{ .override = "identity" },
            },
            .extra_headers = &.{.{ .name = "accept", .value = "application/json" }},
        }) catch |err| {
            if (err == error.WriteFailed and body.end > request_data.max_response_bytes) {
                return error.ResponseTooLarge;
            }
            return mapFetchError(err);
        };
        if (body.end > request_data.max_response_bytes) return error.ResponseTooLarge;
        return .{
            .status = result.status,
            .storage = storage,
            .body_len = body.end,
        };
    }

    fn waitForTimeout(io: std.Io, timeout_ms: u32) bool {
        std.Io.sleep(io, .fromMilliseconds(timeout_ms), .awake) catch return false;
        return true;
    }

    fn requestTimeout(http_request: authentication.HttpRequest) u32 {
        return switch (http_request) {
            inline else => |request_value| request_value.timeout_ms,
        };
    }

    fn drain(select: anytype, allocator: std.mem.Allocator) void {
        while (select.cancel()) |outcome| switch (outcome) {
            .request => |result| if (result) |response_value| {
                var response = response_value;
                response.deinit(allocator);
            } else |_| {},
            .timeout => {},
        };
    }

    fn mapFetchError(err: anyerror) (authentication.TransportError || error{Canceled}) {
        const name = @errorName(err);
        return switch (err) {
            error.OutOfMemory => error.OutOfMemory,
            error.Canceled => error.Canceled,
            else => if (std.mem.startsWith(u8, name, "Http") or
                err == error.UnsupportedCompressionMethod or err == error.StreamTooLong)
                error.InvalidResponse
            else
                error.NetworkError,
        };
    }
};

test "rejects a zero authentication timeout before network access" {
    var adapter: StdHttpTransport = .{
        .io = std.testing.io,
        .environ_map = null,
    };
    try std.testing.expectError(error.Timeout, adapter.transport().postForm(std.testing.allocator, .{
        .url = "https://example.invalid",
        .body = "client_id=test",
        .timeout_ms = 0,
    }));
    try std.testing.expectError(error.Timeout, adapter.transport().postJson(std.testing.allocator, .{
        .url = "https://example.invalid",
        .body = "{}",
        .timeout_ms = 0,
    }));
    try std.testing.expectError(error.Timeout, adapter.transport().getBearer(std.testing.allocator, .{
        .url = "https://example.invalid",
        .bearer_token = "secret",
        .timeout_ms = 0,
    }));
}

test "rejects an overflowing response limit before network access" {
    var adapter: StdHttpTransport = .{
        .io = std.testing.io,
        .environ_map = null,
    };
    try std.testing.expectError(error.ResponseTooLarge, adapter.transport().postForm(std.testing.allocator, .{
        .url = "https://example.invalid",
        .body = "client_id=test",
        .max_response_bytes = std.math.maxInt(usize),
    }));
}

test "rejects unsafe bearer header bytes before network access" {
    var adapter: StdHttpTransport = .{
        .io = std.testing.io,
        .environ_map = null,
    };
    try std.testing.expectError(error.InvalidResponse, adapter.transport().getBearer(std.testing.allocator, .{
        .url = "https://example.invalid",
        .bearer_token = "token\r\ninjected: value",
    }));
}

// Exercises the production adapter without contacting an account service.
const LoopbackHttpFixture = struct {
    io: std.Io,
    listener: *std.Io.net.Server,
    stall: bool = false,

    fn serve(fixture: *LoopbackHttpFixture) !void {
        var stream = try fixture.listener.accept(fixture.io);
        defer stream.close(fixture.io);
        var read_buffer: [4096]u8 = undefined;
        var write_buffer: [4096]u8 = undefined;
        var reader = stream.reader(fixture.io, &read_buffer);
        var writer = stream.writer(fixture.io, &write_buffer);
        var server: std.http.Server = .init(&reader.interface, &writer.interface);
        var request = try server.receiveHead();
        if (fixture.stall) try std.Io.sleep(fixture.io, .fromSeconds(10), .awake);
        try request.respond("{\"fixture\":true}", .{ .keep_alive = false });
    }
};

fn checkLoopbackHttp(host: []const u8) !void {
    const io = std.testing.io;
    const allocator = std.testing.allocator;
    const address = try std.Io.net.IpAddress.parse("127.0.0.1", 0);
    var listener = try address.listen(io, .{ .reuse_address = true });
    defer listener.deinit(io);
    var fixture: LoopbackHttpFixture = .{ .io = io, .listener = &listener };
    var serving = try io.concurrent(LoopbackHttpFixture.serve, .{&fixture});
    defer _ = serving.cancel(io) catch {};
    const url = try std.fmt.allocPrint(allocator, "http://{s}:{d}/synthetic", .{ host, listener.socket.address.getPort() });
    defer allocator.free(url);
    var adapter: StdHttpTransport = .{ .io = io, .environ_map = null };
    var response = try adapter.transport().postForm(allocator, .{
        .url = url,
        .body = "client_id=synthetic-fixture",
        .timeout_ms = 2000,
    });
    defer response.deinit(allocator);
    try serving.await(io);
    try std.testing.expectEqual(std.http.Status.ok, response.status);
    try std.testing.expectEqualStrings("{\"fixture\":true}", try response.body());
}

test "authentication HTTP adapter posts to numeric loopback and cancels timer" {
    try checkLoopbackHttp("127.0.0.1");
}

test "authentication HTTP adapter resolves localhost and cancels timer" {
    try checkLoopbackHttp("localhost");
}

test "authentication HTTP adapter times out a stalled loopback response" {
    const io = std.testing.io;
    const allocator = std.testing.allocator;
    const address = try std.Io.net.IpAddress.parse("127.0.0.1", 0);
    var listener = try address.listen(io, .{ .reuse_address = true });
    defer listener.deinit(io);
    var fixture: LoopbackHttpFixture = .{ .io = io, .listener = &listener, .stall = true };
    var serving = try io.concurrent(LoopbackHttpFixture.serve, .{&fixture});
    defer _ = serving.cancel(io) catch {};
    const url = try std.fmt.allocPrint(allocator, "http://127.0.0.1:{d}/synthetic", .{listener.socket.address.getPort()});
    defer allocator.free(url);
    var adapter: StdHttpTransport = .{ .io = io, .environ_map = null };
    try std.testing.expectError(error.Timeout, adapter.transport().postForm(allocator, .{
        .url = url,
        .body = "client_id=synthetic-fixture",
        .timeout_ms = 100,
    }));
}
