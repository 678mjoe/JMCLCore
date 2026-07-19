const std = @import("std");
const jmcl = @import("jmcl_core");

const cli_instance = @import("cli/instance.zig");
const cli_install = @import("cli/install.zig");
const cli_java = @import("cli/java.zig");
const cli_launch = @import("cli/launch.zig");
const cli_store = @import("cli/store.zig");
const cli_version = @import("cli/version.zig");

const max_request_bytes = 64 * 1024;

pub fn main(init: std.process.Init) !void {
    const args = try init.minimal.args.toSlice(init.arena.allocator());

    var stdout_buffer: [4096]u8 = undefined;
    var stdout_file: std.Io.File.Writer = .init(.stdout(), init.io, &stdout_buffer);
    const stdout = &stdout_file.interface;

    var stderr_buffer: [4096]u8 = undefined;
    var stderr_file: std.Io.File.Writer = .init(.stderr(), init.io, &stderr_buffer);
    const stderr = &stderr_file.interface;

    const exit_code = try run(init, args, stdout, stderr);
    try stdout.flush();
    try stderr.flush();
    if (exit_code != 0) std.process.exit(exit_code);
}

fn run(
    init: std.process.Init,
    args: []const []const u8,
    stdout: *std.Io.Writer,
    stderr: *std.Io.Writer,
) !u8 {
    var service = jmcl.Service.init(init.gpa, init.io, init.environ_map);

    if (args.len < 2 or std.mem.eql(u8, args[1], "help") or std.mem.eql(u8, args[1], "--help")) {
        try writeHelp(stdout);
        return 0;
    }

    const command = args[1];
    const rest = args[2..];
    if (std.mem.eql(u8, command, "version")) return cli_version.run(&service, rest, stdout, stderr);
    if (std.mem.eql(u8, command, "install")) return cli_install.run(&service, rest, stdout, stderr);
    if (std.mem.eql(u8, command, "store")) return cli_store.run(&service, rest, stdout, stderr);
    if (std.mem.eql(u8, command, "java")) return cli_java.run(&service, rest, stdout, stderr);
    if (std.mem.eql(u8, command, "instance")) return cli_instance.run(&service, rest, stdout, stderr);
    if (std.mem.eql(u8, command, "launch")) return cli_launch.run(&service, rest, stdout, stderr);

    if (std.mem.eql(u8, command, "rpc")) {
        if (args.len != 2) {
            try stderr.writeAll("usage: jmcl-core rpc\n");
            return 2;
        }
        try runRpc(init, &service, stdout);
        return 0;
    }

    try stderr.print("unknown command: {s}\n", .{command});
    try stderr.writeAll("run `jmcl-core --help` for usage\n");
    return 2;
}

fn runRpc(init: std.process.Init, service: *jmcl.Service, stdout: *std.Io.Writer) !void {
    var stdin_buffer: [max_request_bytes]u8 = undefined;
    defer std.crypto.secureZero(u8, &stdin_buffer);
    var stdin_file: std.Io.File.Reader = .init(.stdin(), init.io, &stdin_buffer);
    const stdin = &stdin_file.interface;

    while (true) {
        const maybe_line = stdin.takeDelimiter('\n') catch |err| switch (err) {
            error.StreamTooLong => {
                _ = try stdin.discardDelimiterInclusive('\n');
                std.crypto.secureZero(u8, &stdin_buffer);
                try jmcl.protocol.handleLine(service, "", stdout);
                try stdout.flush();
                continue;
            },
            else => return err,
        };
        const raw_line = maybe_line orelse break;
        defer std.crypto.secureZero(u8, @constCast(raw_line));
        const line = if (std.mem.endsWith(u8, raw_line, "\r"))
            raw_line[0 .. raw_line.len - 1]
        else
            raw_line;
        if (line.len == 0) continue;

        try jmcl.protocol.handleLine(service, line, stdout);
        try stdout.flush();
    }
}

fn writeHelp(writer: *std.Io.Writer) std.Io.Writer.Error!void {
    try writer.writeAll(
        \\JMCL Core
        \\
        \\Usage:
        \\  jmcl-core version [--json]  Show core and protocol version
        \\  jmcl-core version list      Fetch and list Minecraft versions
        \\  jmcl-core version show ID   Fetch detailed version metadata
        \\  jmcl-core install plan ID   Generate an immutable install plan
        \\  jmcl-core install execute ID Download and verify static files
        \\  jmcl-core install prepare ID Prepare natives and legacy assets
        \\  jmcl-core store gc          Reclaim unlinked cached artifacts
        \\  jmcl-core instance <command>  Manage isolated instances
        \\  jmcl-core java detect         Detect local Java runtimes
        \\  jmcl-core java select ID      Select a compatible Java runtime
        \\  jmcl-core launch execute ID   Launch Minecraft
        \\  jmcl-core rpc               Start a JSON Lines RPC session
        \\  jmcl-core help              Show this help
        \\
    );
}

test "help includes available commands" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();

    try writeHelp(&output.writer);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "jmcl-core rpc") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "jmcl-core launch execute") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "jmcl-core instance") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "jmcl-core store gc") != null);
}

test {
    _ = cli_instance;
    _ = cli_install;
    _ = cli_java;
    _ = cli_launch;
    _ = cli_store;
    _ = cli_version;
}
