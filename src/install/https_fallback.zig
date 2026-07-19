//! Minimal HTTPS GET fallback used when the primary HTTP stack cannot
//! complete a TLS handshake with a host (for example TLS 1.2 servers with
//! ECDSA certificates, which the standard library client does not support).
//! Requests are made with the vendored TLS client in `vendor/tls_client.zig`.
//!
//! The implementation is deliberately small: HTTP/1.1 with `Connection:
//! close`, absolute-URL redirects, and `Transfer-Encoding: chunked` support.
//! Callers remain responsible for response size and integrity checks.

const std = @import("std");
const vendor_tls = @import("../vendor/tls.zig");

pub const max_redirects: u8 = 5;
pub const max_header_bytes: usize = 32 * 1024;
pub const max_status_line_bytes: usize = 1024;

/// Performs an HTTPS GET of `url`, following up to `max_redirects`
/// absolute-URL redirects, and streams the response body into `writer`.
/// Returns the final response status code.
pub fn fetch(
    allocator: std.mem.Allocator,
    io: std.Io,
    url: []const u8,
    writer: *std.Io.Writer,
) !u16 {
    var current = try std.fmt.allocPrint(allocator, "{s}", .{url});
    defer allocator.free(current);

    var redirects: u8 = 0;
    while (true) {
        const outcome = try fetchOnce(allocator, io, .get, current, writer, null);
        switch (outcome) {
            .status => |code| return code,
            .location => |next| {
                redirects += 1;
                if (redirects > max_redirects) {
                    allocator.free(next);
                    return error.TooManyRedirects;
                }
                allocator.free(current);
                current = next;
            },
        }
    }
}

pub const HeadResult = struct {
    status: u16,
    content_length: ?u64,
};

/// Performs an HTTPS HEAD of `url`, following up to `max_redirects`
/// absolute-URL redirects, and returns the final status code along with
/// the Content-Length header when the server sends one.
pub fn fetchHead(allocator: std.mem.Allocator, io: std.Io, url: []const u8) !HeadResult {
    var current = try std.fmt.allocPrint(allocator, "{s}", .{url});
    defer allocator.free(current);

    var redirects: u8 = 0;
    while (true) {
        var discard_buffer: [4096]u8 = undefined;
        var discard: std.Io.Writer.Discarding = .init(&discard_buffer);
        var headers: Headers = .{};
        const outcome = try fetchOnce(allocator, io, .head, current, &discard.writer, &headers);
        switch (outcome) {
            .status => |code| return .{ .status = code, .content_length = headers.content_length },
            .location => |next| {
                redirects += 1;
                if (redirects > max_redirects) {
                    allocator.free(next);
                    return error.TooManyRedirects;
                }
                allocator.free(current);
                current = next;
            },
        }
    }
}

const Method = enum { get, head };

const Headers = struct {
    content_length: ?u64 = null,
};

const Outcome = union(enum) {
    status: u16,
    /// Owned by the caller; free with the allocator passed to `fetch`.
    location: []u8,
};

fn fetchOnce(
    allocator: std.mem.Allocator,
    io: std.Io,
    method: Method,
    url: []const u8,
    writer: *std.Io.Writer,
    out_headers: ?*Headers,
) !Outcome {
    const uri = std.Uri.parse(url) catch return error.InvalidFallbackUrl;
    if (!std.mem.eql(u8, uri.scheme, "https")) return error.InvalidFallbackUrl;
    const host_component = uri.host orelse return error.InvalidFallbackUrl;
    var host_buffer: [253]u8 = undefined;
    const host = host_component.toRaw(&host_buffer) catch return error.InvalidFallbackUrl;
    if (host.len == 0) return error.InvalidFallbackUrl;
    const port: u16 = uri.port orelse 443;

    var bundle: std.crypto.Certificate.Bundle = .empty;
    defer bundle.deinit(allocator);
    var bundle_lock: std.Io.RwLock = .init;
    try bundle.rescan(allocator, io, std.Io.Timestamp.now(io, .real));

    const hostname: std.Io.net.HostName = std.Io.net.HostName.init(host) catch
        return error.InvalidFallbackUrl;
    const stream = hostname.connect(io, port, .{ .mode = .stream, .protocol = .tcp }) catch |err| switch (err) {
        else => |other| return other,
    };
    defer stream.close(io);

    const tls_read_buffer = try allocator.alloc(u8, vendor_tls.Client.min_buffer_len);
    defer allocator.free(tls_read_buffer);
    var socket_read_buffer: [vendor_tls.Client.min_buffer_len]u8 = undefined;
    var socket_write_buffer: [vendor_tls.Client.min_buffer_len]u8 = undefined;
    var stream_writer = stream.writer(io, &socket_write_buffer);
    var stream_reader = stream.reader(io, &socket_read_buffer);

    var entropy: [vendor_tls.Client.Options.entropy_len]u8 = undefined;
    try io.randomSecure(&entropy);
    var client = try vendor_tls.Client.init(
        &stream_reader.interface,
        &stream_writer.interface,
        .{
            .host = .{ .explicit = host },
            .ca = .{ .bundle = .{
                .gpa = allocator,
                .io = io,
                .lock = &bundle_lock,
                .bundle = &bundle,
            } },
            .read_buffer = tls_read_buffer,
            .write_buffer = &socket_write_buffer,
            .entropy = &entropy,
            .realtime_now = std.Io.Timestamp.now(io, .real),
            .allow_truncation_attacks = true,
        },
    );

    var request_buffer: [4 * 1024]u8 = undefined;
    const target = if (uri.query) |query| blk: {
        const combined = try std.fmt.allocPrint(allocator, "{s}?{s}", .{ componentText(uri.path), componentText(query) });
        if (combined.len == 0) {
            allocator.free(combined);
            break :blk try allocator.dupe(u8, "/");
        }
        break :blk combined;
    } else if (componentText(uri.path).len == 0)
        try allocator.dupe(u8, "/")
    else
        try allocator.dupe(u8, componentText(uri.path));
    defer allocator.free(target);
    const request = std.fmt.bufPrint(
        &request_buffer,
        "{s} {s} HTTP/1.1\r\nHost: {s}\r\nUser-Agent: jmcl-core/0.1.0-dev\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        .{ if (method == .head) "HEAD" else "GET", target, host },
    ) catch return error.InvalidFallbackUrl;
    try client.writer.writeAll(request);
    try client.writer.flush();
    try stream_writer.interface.flush();

    var header_bytes: usize = 0;
    const status_line = try readHeaderLine(&client.reader, &header_bytes, max_status_line_bytes);
    const status = try parseStatusLine(status_line);

    var chunked = false;
    var location: ?[]const u8 = null;
    while (true) {
        const line = try readHeaderLine(&client.reader, &header_bytes, max_header_bytes);
        if (line.len == 0) break;
        const colon = std.mem.indexOfScalar(u8, line, ':') orelse return error.InvalidFallbackHeader;
        const name = std.mem.trim(u8, line[0..colon], " \t");
        const value = std.mem.trim(u8, line[colon + 1 ..], " \t");
        if (std.ascii.eqlIgnoreCase(name, "transfer-encoding")) {
            if (std.ascii.eqlIgnoreCase(value, "chunked")) chunked = true;
        } else if (std.ascii.eqlIgnoreCase(name, "location")) {
            if (value.len == 0 or value.len > 2048) return error.InvalidFallbackRedirect;
            if (location) |previous| allocator.free(previous);
            location = try allocator.dupe(u8, value);
        } else if (std.ascii.eqlIgnoreCase(name, "content-length")) {
            if (out_headers) |headers| {
                headers.content_length = std.fmt.parseInt(u64, value, 10) catch null;
            }
        }
    }

    if (status == 301 or status == 302 or status == 303 or status == 307 or status == 308) {
        const redirect_target = location orelse return error.InvalidFallbackRedirect;
        defer allocator.free(redirect_target);
        const redirect_uri = std.Uri.parse(redirect_target) catch return error.InvalidFallbackRedirect;
        if (!std.mem.eql(u8, redirect_uri.scheme, "https")) return error.InvalidFallbackRedirect;
        if (redirect_uri.host == null) return error.InvalidFallbackRedirect;
        if (method == .get) {
            var discard_buffer: [4096]u8 = undefined;
            var discard: std.Io.Writer.Discarding = .init(&discard_buffer);
            try drainBody(&client.reader, &discard.writer, chunked);
        }
        return .{ .location = try allocator.dupe(u8, redirect_target) };
    }
    if (location) |owned| allocator.free(owned);

    if (method == .get) try drainBody(&client.reader, writer, chunked);
    return .{ .status = status };
}

fn componentText(component: std.Uri.Component) []const u8 {
    return switch (component) {
        .raw, .percent_encoded => |text| text,
    };
}

fn readHeaderLine(reader: *std.Io.Reader, total: *usize, max_total: usize) ![]const u8 {
    const raw = reader.takeDelimiterInclusive('\n') catch |err| switch (err) {
        error.StreamTooLong => return error.FallbackHeaderTooLarge,
        error.EndOfStream => return error.FallbackTruncated,
        error.ReadFailed => return error.FallbackReadFailed,
    };
    total.* += raw.len;
    if (total.* > max_total) return error.FallbackHeaderTooLarge;
    return std.mem.trimEnd(u8, raw, "\r\n");
}

fn parseStatusLine(line: []const u8) !u16 {
    if (line.len < 12 or !std.mem.startsWith(u8, line, "HTTP/")) return error.InvalidFallbackStatus;
    const space = std.mem.indexOfScalar(u8, line, ' ') orelse return error.InvalidFallbackStatus;
    if (space + 4 > line.len) return error.InvalidFallbackStatus;
    const digits = line[space + 1 .. space + 4];
    for (digits) |digit| {
        if (!std.ascii.isDigit(digit)) return error.InvalidFallbackStatus;
    }
    return std.fmt.parseInt(u16, digits, 10) catch return error.InvalidFallbackStatus;
}

fn drainBody(reader: *std.Io.Reader, writer: *std.Io.Writer, chunked: bool) !void {
    if (chunked) return drainChunked(reader, writer);
    _ = reader.streamRemaining(writer) catch |err| switch (err) {
        error.WriteFailed => return error.WriteFailed,
        else => return error.FallbackReadFailed,
    };
}

fn drainChunked(reader: *std.Io.Reader, writer: *std.Io.Writer) !void {
    var ignored: usize = 0;
    while (true) {
        const size_line = readHeaderLine(reader, &ignored, std.math.maxInt(usize)) catch
            return error.InvalidFallbackChunk;
        const size_text = size_line[0 .. std.mem.indexOfScalar(u8, size_line, ';') orelse size_line.len];
        const size = std.fmt.parseInt(u64, size_text, 16) catch return error.InvalidFallbackChunk;
        if (size == 0) {
            while (true) {
                const trailer = readHeaderLine(reader, &ignored, std.math.maxInt(usize)) catch
                    return error.InvalidFallbackChunk;
                if (trailer.len == 0) return;
            }
        }
        reader.streamExact64(writer, size) catch |err| switch (err) {
            error.WriteFailed => return error.WriteFailed,
            else => return error.InvalidFallbackChunk,
        };
        const ending = readHeaderLine(reader, &ignored, std.math.maxInt(usize)) catch
            return error.InvalidFallbackChunk;
        if (ending.len != 0) return error.InvalidFallbackChunk;
    }
}
