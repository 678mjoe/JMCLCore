const std = @import("std");
const artifact_store = @import("../install/artifact_store.zig");
const hard_link = @import("../install/hard_link.zig");
const install_plan = @import("../install/plan.zig");
const java_runtime = @import("../java/java_runtime.zig");
const neoforge = @import("neoforge.zig");
const zip_reader = @import("../install/zip_reader.zig");

const Sha1 = std.crypto.hash.Sha1;

pub const max_processor_jar_bytes: usize = 64 * 1024 * 1024;
pub const max_manifest_bytes: usize = 64 * 1024;
pub const max_embedded_bytes: usize = 64 * 1024 * 1024;
pub const max_process_output_bytes: usize = 16 * 1024 * 1024;

pub const Options = struct {
    java_paths: []const []const u8 = &.{},
    required_major: u16 = 0,
    expected_patched_sha1: ?[]const u8 = null,
    side: neoforge.Side = .client,
    minecraft_jar_relative: ?[]const u8 = null,
};

pub const Status = enum { patched, cached };

pub const Summary = struct {
    status: Status,
    processors_run: usize,
    /// Owned by the caller; free with the allocator passed to `run`.
    patched_sha1: []const u8,
    patched_size: u64,
};

pub const Event = union(enum) {
    processor_started: ProcessorStarted,
};

pub const ProcessorStarted = struct {
    index: usize,
    total: usize,
    name: []const u8,
};

pub const Reporter = struct {
    context: *anyopaque,
    callback: *const fn (context: *anyopaque, event: Event) void,

    fn emit(reporter: ?Reporter, event: Event) void {
        if (reporter) |value| value.callback(value.context, event);
    }
};

const State = struct {
    patched_sha1: []const u8,
    patched_size: u64,
};

const ResolverContext = struct {
    map: std.StringHashMap([]const u8),
    libraries_directory: []const u8,

    fn resolve(raw: *anyopaque, key: []const u8) ?[]const u8 {
        const context: *ResolverContext = @ptrCast(@alignCast(raw));
        return context.map.get(key);
    }

    fn artifact(raw: *anyopaque, coordinate: []const u8, allocator: std.mem.Allocator) ![]u8 {
        const context: *ResolverContext = @ptrCast(@alignCast(raw));
        const relative = try neoforge.artifactPathAlloc(allocator, coordinate);
        defer allocator.free(relative);
        return std.fs.path.join(allocator, &.{ context.libraries_directory, relative });
    }
};

pub fn run(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: ?*const std.process.Environ.Map,
    minecraft_dir: std.Io.Dir,
    store: std.Io.Dir,
    neoforge_version: []const u8,
    version_id: []const u8,
    options: Options,
    reporter: ?Reporter,
) !Summary {
    try neoforge.validateVersionId(neoforge_version);
    const support_relative = try std.fmt.allocPrint(allocator, ".jmcl/neoforge/{s}", .{neoforge_version});
    defer allocator.free(support_relative);
    const minecraft_jar_relative = try std.fmt.allocPrint(allocator, "versions/{s}/{s}.jar", .{ version_id, version_id });
    defer allocator.free(minecraft_jar_relative);
    var resolved_options = options;
    resolved_options.side = .client;
    resolved_options.minecraft_jar_relative = minecraft_jar_relative;
    return runLayout(
        allocator,
        io,
        environ_map,
        minecraft_dir,
        store,
        support_relative,
        version_id,
        resolved_options,
        reporter,
        "neoforge",
    );
}

pub fn runServer(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: ?*const std.process.Environ.Map,
    server_dir: std.Io.Dir,
    store: std.Io.Dir,
    neoforge_version: []const u8,
    version_id: []const u8,
    options: Options,
    reporter: ?Reporter,
) !Summary {
    try neoforge.validateVersionId(neoforge_version);
    const support_relative = try std.fmt.allocPrint(allocator, ".jmcl/neoforge-server/{s}", .{neoforge_version});
    defer allocator.free(support_relative);
    var resolved_options = options;
    resolved_options.side = .server;
    resolved_options.minecraft_jar_relative = "server.jar";
    return runLayout(
        allocator,
        io,
        environ_map,
        server_dir,
        store,
        support_relative,
        version_id,
        resolved_options,
        reporter,
        "neoforge",
    );
}

/// Runs a mod-loader installer processor chain whose installer archive and
/// state live under `support_relative` inside the Minecraft directory.
pub fn runLayout(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: ?*const std.process.Environ.Map,
    minecraft_dir: std.Io.Dir,
    store: std.Io.Dir,
    support_relative: []const u8,
    version_id: []const u8,
    options: Options,
    reporter: ?Reporter,
    loader_name: []const u8,
) !Summary {
    try neoforge.validateRelativePath(support_relative);
    try neoforge.validateVersionId(version_id);

    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();
    const scratch = arena.allocator();

    const installer_relative = try std.fmt.allocPrint(scratch, "{s}/installer.jar", .{support_relative});
    const installer_bytes = minecraft_dir.readFileAlloc(io, installer_relative, allocator, .limited(neoforge.max_installer_bytes + 1)) catch |err| switch (err) {
        error.FileNotFound => return error.InstallerArchiveMissing,
        error.StreamTooLong => return error.InstallerArchiveTooLarge,
        else => |other| return other,
    };
    defer allocator.free(installer_bytes);
    if (installer_bytes.len > neoforge.max_installer_bytes) return error.InstallerArchiveTooLarge;

    const install_source = (try zip_reader.readEntryAlloc(
        scratch,
        installer_bytes,
        "install_profile.json",
        neoforge.max_install_profile_bytes,
    )) orelse return error.NeoForgeInstallProfileMissing;
    var install_document = try neoforge.parseInstallProfile(scratch, install_source);
    defer install_document.deinit();
    const profile = install_document.value;

    const patched_relative = try neoforge.patchedPathAlloc(scratch, profile, options.side);

    if (try readState(allocator, io, minecraft_dir, support_relative)) |state| {
        if (artifact_store.verify(minecraft_dir, io, patched_relative, state.patched_sha1, state.patched_size) catch false) {
            return .{
                .status = .cached,
                .processors_run = 0,
                .patched_sha1 = state.patched_sha1,
                .patched_size = state.patched_size,
            };
        }
    }

    const environment = environ_map orelse return error.EnvironmentUnavailable;
    const host = try install_plan.Target.host();
    var java = try resolveJava(allocator, io, environment, options.java_paths, options.required_major, host.arch);
    defer java.deinit();

    try minecraft_dir.createDirPath(io, support_relative);
    const work_relative = try std.fmt.allocPrint(scratch, "{s}/work", .{support_relative});
    minecraft_dir.deleteTree(io, work_relative) catch {};
    try minecraft_dir.createDirPath(io, work_relative);
    defer minecraft_dir.deleteTree(io, work_relative) catch {};

    const root_absolute = try absolutePathAlloc(scratch, io, minecraft_dir);
    const libraries_absolute = try std.fs.path.join(scratch, &.{ root_absolute, "libraries" });
    const installer_absolute = try std.fs.path.join(scratch, &.{ root_absolute, installer_relative });
    const minecraft_jar_relative = options.minecraft_jar_relative orelse
        try std.fmt.allocPrint(scratch, "versions/{s}/{s}.jar", .{ version_id, version_id });
    try neoforge.validateRelativePath(minecraft_jar_relative);
    const minecraft_jar_absolute = try std.fs.path.join(scratch, &.{ root_absolute, minecraft_jar_relative });

    var map: std.StringHashMap([]const u8) = .init(scratch);
    try putBasePlaceholders(&map, options.side, root_absolute, libraries_absolute, installer_absolute, minecraft_jar_absolute);

    var data_iterator = profile.data.map.iterator();
    while (data_iterator.next()) |entry| {
        const value = neoforge.dataValueForSide(entry.value_ptr.*, options.side);
        switch (neoforge.classifyDataValue(value)) {
            .literal => |literal| try map.put(entry.key_ptr.*, literal),
            .coordinate => |coordinate| {
                const relative = try neoforge.artifactPathAlloc(scratch, coordinate);
                if (std.fs.path.dirname(relative)) |parent| {
                    const absolute_parent = try std.fs.path.join(scratch, &.{ libraries_absolute, parent });
                    try std.Io.Dir.cwd().createDirPath(io, absolute_parent);
                }
                try map.put(entry.key_ptr.*, try std.fs.path.join(scratch, &.{ libraries_absolute, relative }));
            },
            .embedded => |embedded| {
                const content = (try zip_reader.readEntryAlloc(scratch, installer_bytes, embedded, max_embedded_bytes)) orelse
                    return error.InstallerEmbeddedDataMissing;
                const destination = try std.fmt.allocPrint(scratch, "{s}/{s}", .{ work_relative, entry.key_ptr.* });
                try std.Io.Dir.cwd().createDirPath(io, try std.fs.path.join(scratch, &.{ root_absolute, work_relative }));
                try minecraft_dir.writeFile(io, .{ .sub_path = destination, .data = content });
                try map.put(entry.key_ptr.*, try std.fs.path.join(scratch, &.{ root_absolute, destination }));
            },
        }
    }

    var resolver_context: ResolverContext = .{
        .map = map,
        .libraries_directory = libraries_absolute,
    };
    const resolver: neoforge.PlaceholderResolver = .{
        .context = &resolver_context,
        .resolve = ResolverContext.resolve,
        .artifact = ResolverContext.artifact,
    };

    const applicable = try collectApplicableProcessors(scratch, profile, options.side);

    for (applicable, 0..) |processor, index| {
        Reporter.emit(reporter, .{ .processor_started = .{
            .index = index + 1,
            .total = applicable.len,
            .name = processor.jar,
        } });
        try runProcessor(
            allocator,
            io,
            environment,
            scratch,
            minecraft_dir,
            root_absolute,
            libraries_absolute,
            java.executable,
            processor,
            resolver,
            loader_name,
        );
    }

    const patched_file = minecraft_dir.openFile(io, patched_relative, .{}) catch |err| switch (err) {
        error.FileNotFound => return error.PatchedClientMissing,
        else => |other| return other,
    };
    defer patched_file.close(io);
    const patched_digest = try artifact_store.hashFile(patched_file, io);
    const patched_stat = try patched_file.stat(io);
    const patched_sha1 = try std.fmt.allocPrint(allocator, "{s}", .{std.fmt.bytesToHex(patched_digest, .lower)});
    defer allocator.free(patched_sha1);
    if (options.expected_patched_sha1) |expected| {
        try neoforge.validateSha1(expected);
        if (!std.ascii.eqlIgnoreCase(patched_sha1, expected)) return error.PatchedHashMismatch;
    }

    var object_path_buffer: [64]u8 = undefined;
    const object_path = try artifact_store.objectPath(patched_sha1, &object_path_buffer);
    _ = try artifact_store.importObject(allocator, io, store, object_path, minecraft_dir, patched_relative);
    _ = try artifact_store.materialize(allocator, io, store, object_path, minecraft_dir, patched_relative, true);
    try writeState(io, minecraft_dir, support_relative, patched_sha1, patched_stat.size);

    return .{
        .status = .patched,
        .processors_run = applicable.len,
        .patched_sha1 = try allocator.dupe(u8, patched_sha1),
        .patched_size = patched_stat.size,
    };
}

fn putBasePlaceholders(
    map: *std.StringHashMap([]const u8),
    side: neoforge.Side,
    root_absolute: []const u8,
    libraries_absolute: []const u8,
    installer_absolute: []const u8,
    minecraft_jar_absolute: []const u8,
) !void {
    try map.put("SIDE", @tagName(side));
    try map.put("ROOT", root_absolute);
    try map.put("LIBRARY_DIR", libraries_absolute);
    try map.put("INSTALLER", installer_absolute);
    try map.put("MINECRAFT_JAR", minecraft_jar_absolute);
}

fn collectApplicableProcessors(
    allocator: std.mem.Allocator,
    profile: neoforge.InstallProfile,
    side: neoforge.Side,
) ![]neoforge.Processor {
    var applicable: std.ArrayList(neoforge.Processor) = .empty;
    for (profile.processors) |processor| {
        if (neoforge.processorApplies(processor, side)) try applicable.append(allocator, processor);
    }
    return applicable.toOwnedSlice(allocator);
}

const JavaSelection = struct {
    arena: *std.heap.ArenaAllocator,
    executable: []const u8,

    fn deinit(selection: *JavaSelection) void {
        const allocator = selection.arena.child_allocator;
        selection.arena.deinit();
        allocator.destroy(selection.arena);
        selection.* = undefined;
    }
};

fn resolveJava(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    explicit: []const []const u8,
    required_major: u16,
    architecture: install_plan.Architecture,
) !JavaSelection {
    const arena = try allocator.create(std.heap.ArenaAllocator);
    errdefer allocator.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(allocator);
    errdefer arena.deinit();

    const detection = try java_runtime.detect(arena.allocator(), io, environ_map, explicit);
    if (required_major != 0) {
        if (java_runtime.selectCompatible(detection, required_major, architecture)) |runtime| {
            return .{ .arena = arena, .executable = runtime.executable };
        } else |_| {}
    }
    for (detection.runtimes) |runtime| {
        if (runtime.major_version >= 8) return .{ .arena = arena, .executable = runtime.executable };
    }
    return error.NoCompatibleJava;
}

fn runProcessor(
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: *const std.process.Environ.Map,
    scratch: std.mem.Allocator,
    minecraft_dir: std.Io.Dir,
    root_absolute: []const u8,
    libraries_absolute: []const u8,
    java_executable: []const u8,
    processor: neoforge.Processor,
    resolver: neoforge.PlaceholderResolver,
    loader_name: []const u8,
) !void {
    const separator: []const u8 = if (@import("builtin").os.tag == .windows) ";" else ":";
    var classpath: std.ArrayList([]const u8) = .empty;
    const jar_relative = try neoforge.artifactPathAlloc(scratch, processor.jar);
    try classpath.append(scratch, try std.fs.path.join(scratch, &.{ libraries_absolute, jar_relative }));
    for (processor.classpath) |dependency| {
        const relative = try neoforge.artifactPathAlloc(scratch, dependency);
        try classpath.append(scratch, try std.fs.path.join(scratch, &.{ libraries_absolute, relative }));
    }
    for (classpath.items) |entry| {
        const relative = entry[libraries_absolute.len + 1 ..];
        _ = minecraft_dir.statFile(io, try std.fs.path.join(scratch, &.{ "libraries", relative }), .{}) catch |err| switch (err) {
            error.FileNotFound => return error.ProcessorDependencyMissing,
            else => |other| return other,
        };
    }
    const classpath_joined = try std.mem.join(scratch, separator, classpath.items);

    const jar_bytes = minecraft_dir.readFileAlloc(
        io,
        try std.fs.path.join(scratch, &.{ "libraries", jar_relative }),
        scratch,
        .limited(max_processor_jar_bytes + 1),
    ) catch |err| switch (err) {
        error.StreamTooLong => return error.ProcessorJarTooLarge,
        else => |other| return other,
    };
    const main_class = try readMainClass(scratch, jar_bytes);

    var argv: std.ArrayList([]const u8) = .empty;
    try argv.append(scratch, java_executable);
    try argv.append(scratch, "-cp");
    try argv.append(scratch, classpath_joined);
    try argv.append(scratch, main_class);
    for (processor.args) |argument| {
        try argv.append(scratch, try neoforge.substituteArgumentAlloc(scratch, argument, resolver));
    }

    const result = try std.process.run(allocator, io, .{
        .argv = argv.items,
        .environ_map = environ_map,
        .cwd = .{ .path = root_absolute },
        .stdout_limit = .limited(max_process_output_bytes),
        .stderr_limit = .limited(max_process_output_bytes),
    });
    defer allocator.free(result.stdout);
    defer allocator.free(result.stderr);
    switch (result.term) {
        .exited => |code| {
            if (code != 0) {
                std.debug.print("{s} processor {s} exited {d}\n{s}\n", .{ loader_name, processor.jar, code, result.stderr });
                return error.ProcessorFailed;
            }
        },
        else => return error.ProcessorFailed,
    }
}

fn readMainClass(allocator: std.mem.Allocator, jar_bytes: []const u8) ![]const u8 {
    const manifest = (try zip_reader.readEntryAlloc(allocator, jar_bytes, "META-INF/MANIFEST.MF", max_manifest_bytes)) orelse
        return error.ProcessorManifestMissing;
    defer allocator.free(manifest);
    var logical: std.Io.Writer.Allocating = .init(allocator);
    defer logical.deinit();
    var lines = std.mem.splitAny(u8, manifest, "\n");
    var found = false;
    while (lines.next()) |raw_line| {
        const line = std.mem.trimEnd(u8, raw_line, "\r");
        if (std.mem.startsWith(u8, line, " ") and found) {
            try logical.writer.writeAll(line[1..]);
            continue;
        }
        if (found) break;
        if (std.mem.startsWith(u8, line, "Main-Class:")) {
            try logical.writer.writeAll(std.mem.trim(u8, line["Main-Class:".len..], " \t"));
            found = true;
        }
    }
    if (!found) return error.ProcessorMainClassMissing;
    const main_class = logical.written();
    if (main_class.len == 0 or main_class.len > 512) return error.InvalidProcessorMainClass;
    for (main_class) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '.' and byte != '_' and byte != '$') {
            return error.InvalidProcessorMainClass;
        }
    }
    return logical.toOwnedSlice();
}

fn statePathAlloc(allocator: std.mem.Allocator, support_relative: []const u8) ![]u8 {
    return std.fmt.allocPrint(allocator, "{s}/state.json", .{support_relative});
}

fn readState(
    allocator: std.mem.Allocator,
    io: std.Io,
    minecraft_dir: std.Io.Dir,
    support_relative: []const u8,
) !?State {
    const path = try statePathAlloc(allocator, support_relative);
    defer allocator.free(path);
    const bytes = minecraft_dir.readFileAlloc(io, path, allocator, .limited(4096)) catch |err| switch (err) {
        error.FileNotFound => return null,
        else => |other| return other,
    };
    defer allocator.free(bytes);
    var document = std.json.parseFromSlice(State, allocator, bytes, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = 4096,
    }) catch return null;
    defer document.deinit();
    neoforge.validateSha1(document.value.patched_sha1) catch return null;
    if (document.value.patched_size == 0 or document.value.patched_size > neoforge.max_library_size) return null;
    return .{
        .patched_sha1 = try allocator.dupe(u8, document.value.patched_sha1),
        .patched_size = document.value.patched_size,
    };
}

fn writeState(
    io: std.Io,
    minecraft_dir: std.Io.Dir,
    support_relative: []const u8,
    patched_sha1: []const u8,
    patched_size: u64,
) !void {
    var path_buffer: [512]u8 = undefined;
    const path = try std.fmt.bufPrint(&path_buffer, "{s}/state.json", .{support_relative});
    var atomic = try minecraft_dir.createFileAtomic(io, path, .{ .make_path = true, .replace = true });
    defer atomic.deinit(io);
    var buffer: [256]u8 = undefined;
    const content = try std.fmt.bufPrint(
        &buffer,
        "{{\"patched_sha1\":\"{s}\",\"patched_size\":{d}}}\n",
        .{ patched_sha1, patched_size },
    );
    try atomic.file.writeStreamingAll(io, content);
    try atomic.replace(io);
}

fn absolutePathAlloc(allocator: std.mem.Allocator, io: std.Io, dir: std.Io.Dir) ![]u8 {
    return hard_link.absolutePathAlloc(allocator, io, dir, ".");
}

fn buildManifestJar(allocator: std.mem.Allocator, manifest: []const u8) ![]u8 {
    var output: std.Io.Writer.Allocating = .init(allocator);
    defer output.deinit();
    const name = "META-INF/MANIFEST.MF";
    try writeInt(&output.writer, u32, 0x04034b50);
    try writeInt(&output.writer, u16, 20);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u32, 0);
    try writeInt(&output.writer, u32, @intCast(manifest.len));
    try writeInt(&output.writer, u32, @intCast(manifest.len));
    try writeInt(&output.writer, u16, @intCast(name.len));
    try writeInt(&output.writer, u16, 0);
    try output.writer.writeAll(name);
    try output.writer.writeAll(manifest);
    const central_offset: u32 = @intCast(output.written().len);
    try writeInt(&output.writer, u32, 0x02014b50);
    try writeInt(&output.writer, u16, 20);
    try writeInt(&output.writer, u16, 20);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u32, 0);
    try writeInt(&output.writer, u32, @intCast(manifest.len));
    try writeInt(&output.writer, u32, @intCast(manifest.len));
    try writeInt(&output.writer, u16, @intCast(name.len));
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u32, 0);
    try writeInt(&output.writer, u32, 0);
    try output.writer.writeAll(name);
    const central_size: u32 = @intCast(output.written().len - central_offset);
    try writeInt(&output.writer, u32, 0x06054b50);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u16, 0);
    try writeInt(&output.writer, u16, 1);
    try writeInt(&output.writer, u16, 1);
    try writeInt(&output.writer, u32, central_size);
    try writeInt(&output.writer, u32, central_offset);
    try writeInt(&output.writer, u16, 0);
    return output.toOwnedSlice();
}

fn writeInt(writer: *std.Io.Writer, comptime T: type, value: T) !void {
    var bytes: [@sizeOf(T)]u8 = undefined;
    std.mem.writeInt(T, &bytes, value, .little);
    try writer.writeAll(&bytes);
}

test "parses Main-Class from a jar manifest" {
    const allocator = std.testing.allocator;
    const jar = try buildManifestJar(
        allocator,
        "Manifest-Version: 1.0\r\nMain-Class: net.neoforged.installertools.ConsoleTool\r\nOther: value\r\n",
    );
    defer allocator.free(jar);
    const main_class = try readMainClass(allocator, jar);
    defer allocator.free(main_class);
    try std.testing.expectEqualStrings("net.neoforged.installertools.ConsoleTool", main_class);

    const missing = try buildManifestJar(allocator, "Manifest-Version: 1.0\r\n");
    defer allocator.free(missing);
    try std.testing.expectError(error.ProcessorMainClassMissing, readMainClass(allocator, missing));
}

test "base placeholders follow the requested side" {
    const allocator = std.testing.allocator;
    var server_map: std.StringHashMap([]const u8) = .init(allocator);
    defer server_map.deinit();
    try putBasePlaceholders(
        &server_map,
        .server,
        "/srv",
        "/srv/libraries",
        "/srv/.jmcl/neoforge-server/21.1.133/installer.jar",
        "/srv/server.jar",
    );
    try std.testing.expectEqualStrings(@tagName(.server), server_map.get("SIDE").?);
    try std.testing.expectEqualStrings("/srv/server.jar", server_map.get("MINECRAFT_JAR").?);
    try std.testing.expectEqualStrings("/srv", server_map.get("ROOT").?);
    try std.testing.expectEqualStrings("/srv/libraries", server_map.get("LIBRARY_DIR").?);
    try std.testing.expectEqualStrings("/srv/.jmcl/neoforge-server/21.1.133/installer.jar", server_map.get("INSTALLER").?);

    var client_map: std.StringHashMap([]const u8) = .init(allocator);
    defer client_map.deinit();
    try putBasePlaceholders(
        &client_map,
        .client,
        "/game",
        "/game/libraries",
        "/game/.jmcl/neoforge/21.1.133/installer.jar",
        "/game/versions/1.21.1/1.21.1.jar",
    );
    try std.testing.expectEqualStrings(@tagName(.client), client_map.get("SIDE").?);
    try std.testing.expectEqualStrings("/game/versions/1.21.1/1.21.1.jar", client_map.get("MINECRAFT_JAR").?);
}

test "processor selection follows the requested side" {
    const allocator = std.testing.allocator;
    const both: neoforge.Processor = .{ .jar = "example:both:1.0" };
    const server_only: neoforge.Processor = .{ .jar = "example:server:1.0", .sides = &.{@tagName(.server)} };
    const client_only: neoforge.Processor = .{ .jar = "example:client:1.0", .sides = &.{@tagName(.client)} };
    const profile: neoforge.InstallProfile = .{
        .spec = 1,
        .version = "21.1.133",
        .minecraft = "1.21.1",
        .data = .{},
        .processors = &.{ both, server_only, client_only },
    };

    const server_list = try collectApplicableProcessors(allocator, profile, .server);
    defer allocator.free(server_list);
    try std.testing.expectEqual(@as(usize, 2), server_list.len);
    try std.testing.expectEqualStrings("example:both:1.0", server_list[0].jar);
    try std.testing.expectEqualStrings("example:server:1.0", server_list[1].jar);

    const client_list = try collectApplicableProcessors(allocator, profile, .client);
    defer allocator.free(client_list);
    try std.testing.expectEqual(@as(usize, 2), client_list.len);
    try std.testing.expectEqualStrings("example:both:1.0", client_list[0].jar);
    try std.testing.expectEqualStrings("example:client:1.0", client_list[1].jar);
}
