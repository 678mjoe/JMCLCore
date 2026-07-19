const std = @import("std");

pub const schema_version: u32 = 1;
pub const default_root = "accounts";
const max_manifest_bytes = 16 * 1024;
const max_accounts = 4096;

/// Public account metadata. Authentication credentials intentionally do not
/// belong in this store; callers must use an operating-system secret store.
pub const Account = struct {
    schema_version: u32 = schema_version,
    id: []const u8,
    player_name: []const u8,
    client_id: []const u8,
    xuid: ?[]const u8 = null,
};

pub const Document = std.json.Parsed(Account);

const DiskAccount = struct {
    schema_version: u32,
    id: []const u8,
    player_name: []const u8,
    client_id: []const u8,
    xuid: ?[]const u8 = null,
};

pub const List = struct {
    allocator: std.mem.Allocator,
    items: []Document,

    pub fn deinit(value: *List) void {
        for (value.items) |*document| document.deinit();
        value.allocator.free(value.items);
        value.* = undefined;
    }
};

pub fn openRoot(io: std.Io, path: []const u8) !std.Io.Dir {
    try validateRootName(path);
    return std.Io.Dir.cwd().createDirPathOpen(io, path, .{
        .open_options = .{
            .iterate = true,
            .follow_symlinks = false,
        },
    });
}

pub fn save(io: std.Io, root: std.Io.Dir, account: Account) !void {
    try validate(account);
    // Closing this advisory lock releases it even if the owner exits unexpectedly.
    const lock = try root.createFile(io, ".registry.lock", .{
        .read = true,
        .truncate = false,
        .lock = .exclusive,
    });
    defer lock.close(io);
    var path_buffer: [40]u8 = undefined;
    const path = try manifestPath(account.id, &path_buffer);
    const exists = existing: {
        const file = root.openFile(io, path, .{
            .mode = .read_only,
            .allow_directory = false,
            .follow_symlinks = false,
            .resolve_beneath = true,
        }) catch |err| switch (err) {
            error.FileNotFound => break :existing false,
            else => |other| return other,
        };
        file.close(io);
        break :existing true;
    };
    if (!exists) try ensureWriteCapacity(io, root);
    var atomic_file = try root.createFileAtomic(io, path, .{ .replace = true });
    defer atomic_file.deinit(io);
    var buffer: [4096]u8 = undefined;
    var writer: std.Io.File.Writer = .init(atomic_file.file, io, &buffer);
    var json: std.json.Stringify = .{ .writer = &writer.interface };
    try json.write(account);
    try writer.interface.writeByte('\n');
    try writer.interface.flush();
    try atomic_file.file.sync(io);
    try atomic_file.replace(io);
}

pub fn load(allocator: std.mem.Allocator, io: std.Io, root: std.Io.Dir, id: []const u8) !Document {
    try validateId(id);
    var path_buffer: [40]u8 = undefined;
    const path = try manifestPath(id, &path_buffer);
    const file = try root.openFile(io, path, .{
        .mode = .read_only,
        .allow_directory = false,
        .follow_symlinks = false,
        .resolve_beneath = true,
    });
    defer file.close(io);
    const stat = try file.stat(io);
    if (stat.size > max_manifest_bytes) return error.AccountManifestTooLarge;
    const size: usize = std.math.cast(usize, stat.size) orelse return error.AccountManifestTooLarge;
    const bytes = try allocator.alloc(u8, size);
    defer allocator.free(bytes);
    var offset: usize = 0;
    while (offset < bytes.len) {
        const destinations = [_][]u8{bytes[offset..]};
        const read = file.readStreaming(io, &destinations) catch |err| switch (err) {
            error.EndOfStream => return error.AccountManifestInvalid,
            else => |other| return other,
        };
        if (read == 0) return error.AccountManifestInvalid;
        offset += read;
    }
    var document = std.json.parseFromSlice(DiskAccount, allocator, bytes, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_manifest_bytes,
    }) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return error.AccountManifestInvalid,
    };
    errdefer document.deinit();
    const account: Account = .{
        .schema_version = document.value.schema_version,
        .id = document.value.id,
        .player_name = document.value.player_name,
        .client_id = document.value.client_id,
        .xuid = document.value.xuid,
    };
    validate(account) catch return error.AccountManifestInvalid;
    if (account.schema_version != schema_version) return error.UnsupportedAccountSchema;
    if (!std.mem.eql(u8, id, account.id)) return error.AccountIdMismatch;
    return .{ .arena = document.arena, .value = account };
}

pub fn list(allocator: std.mem.Allocator, io: std.Io, root: std.Io.Dir) !List {
    var documents: std.ArrayList(Document) = .empty;
    errdefer {
        for (documents.items) |*document| document.deinit();
        documents.deinit(allocator);
    }
    var iterator = root.iterate();
    while (try iterator.next(io)) |entry| {
        if (entry.kind != .file and entry.kind != .unknown) continue;
        const id = accountIdFromManifestPath(entry.name) orelse continue;
        if (documents.items.len == max_accounts) return error.TooManyAccounts;
        var document = load(allocator, io, root, id) catch |err| switch (err) {
            error.OutOfMemory => return error.OutOfMemory,
            error.AccountManifestTooLarge, error.AccountManifestInvalid, error.AccountIdMismatch, error.UnsupportedAccountSchema => continue,
            else => |other| return other,
        };
        documents.append(allocator, document) catch |err| {
            document.deinit();
            return err;
        };
    }
    std.sort.heap(Document, documents.items, {}, lessThanDocument);
    return .{
        .allocator = allocator,
        .items = try documents.toOwnedSlice(allocator),
    };
}

pub fn delete(allocator: std.mem.Allocator, io: std.Io, root: std.Io.Dir, id: []const u8) !void {
    var document = try load(allocator, io, root, id);
    document.deinit();
    var path_buffer: [40]u8 = undefined;
    try root.deleteFile(io, try manifestPath(id, &path_buffer));
}

pub fn validate(account: Account) !void {
    try validateId(account.id);
    if (account.schema_version != schema_version) return error.UnsupportedAccountSchema;
    try validatePlayerName(account.player_name);
    try validateClientId(account.client_id);
    if (account.xuid) |xuid| try validateXuid(xuid);
}

pub fn validateId(id: []const u8) !void {
    if (id.len != 32) return error.InvalidAccountId;
    for (id) |byte| {
        if (!(byte >= '0' and byte <= '9') and !(byte >= 'a' and byte <= 'f')) {
            return error.InvalidAccountId;
        }
    }
}

fn validatePlayerName(name: []const u8) !void {
    if (name.len == 0 or name.len > 16) return error.InvalidPlayerName;
    for (name) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '_') return error.InvalidPlayerName;
    }
}

fn validateClientId(client_id: []const u8) !void {
    if (client_id.len != 36) return error.InvalidClientId;
    for (client_id, 0..) |byte, index| {
        const separator = index == 8 or index == 13 or index == 18 or index == 23;
        if (separator) {
            if (byte != '-') return error.InvalidClientId;
        } else if (!std.ascii.isHex(byte)) {
            return error.InvalidClientId;
        }
    }
}

fn validateXuid(xuid: []const u8) !void {
    if (xuid.len == 0 or xuid.len > 32) return error.InvalidXuid;
    for (xuid) |byte| if (byte < '0' or byte > '9') return error.InvalidXuid;
}

fn manifestPath(id: []const u8, buffer: *[40]u8) ![]const u8 {
    try validateId(id);
    return std.fmt.bufPrint(buffer, "{s}.json", .{id});
}

fn validateRootName(path: []const u8) !void {
    if (path.len == 0) return error.EmptyAccountsRoot;
    if (path.len > 64 or std.fs.path.isAbsolute(path) or std.mem.indexOfAny(u8, path, "/\\:") != null) {
        return error.InvalidAccountsRoot;
    }
    if (std.mem.eql(u8, path, ".") or std.mem.eql(u8, path, "..")) return error.InvalidAccountsRoot;
    for (path) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '_' and byte != '-') return error.InvalidAccountsRoot;
    }
}

fn ensureWriteCapacity(io: std.Io, root: std.Io.Dir) !void {
    var count: usize = 0;
    var iterator = root.iterate();
    while (try iterator.next(io)) |entry| {
        if (entry.kind != .file and entry.kind != .unknown) continue;
        if (accountIdFromManifestPath(entry.name) == null) continue;
        count += 1;
        if (count >= max_accounts) return error.TooManyAccounts;
    }
}

fn accountIdFromManifestPath(path: []const u8) ?[]const u8 {
    if (path.len != 37 or !std.mem.endsWith(u8, path, ".json")) return null;
    const id = path[0..32];
    validateId(id) catch return null;
    return id;
}

fn lessThanDocument(_: void, left: Document, right: Document) bool {
    return std.mem.lessThan(u8, left.value.id, right.value.id);
}

test "validates public account metadata" {
    try validate(.{
        .id = "0123456789abcdef0123456789abcdef",
        .player_name = "Player_1",
        .client_id = "00001111-aaaa-2222-bbbb-3333cccc4444",
        .xuid = "123456789",
    });
    try std.testing.expectError(error.InvalidAccountId, validate(.{
        .id = "upper0123456789abcdef0123456789a",
        .player_name = "Player",
        .client_id = "00001111-aaaa-2222-bbbb-3333cccc4444",
    }));
    try std.testing.expectError(error.InvalidPlayerName, validate(.{
        .id = "0123456789abcdef0123456789abcdef",
        .player_name = "Player Name",
        .client_id = "00001111-aaaa-2222-bbbb-3333cccc4444",
    }));
}

test "requires a schema version in stored account metadata" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try temporary.dir.createDir(io, "accounts", .default_dir);
    const root = try temporary.dir.openDir(io, "accounts", .{});
    defer root.close(io);
    const file = try root.createFile(io, "0123456789abcdef0123456789abcdef.json", .{});
    defer file.close(io);
    try std.Io.File.writeStreamingAll(file, io, "{\"id\":\"0123456789abcdef0123456789abcdef\",\"player_name\":\"Player\",\"client_id\":\"00001111-aaaa-2222-bbbb-3333cccc4444\"}");
    try std.testing.expectError(
        error.AccountManifestInvalid,
        load(std.testing.allocator, io, root, "0123456789abcdef0123456789abcdef"),
    );
}

test "saves lists loads and deletes public accounts" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    try temporary.dir.createDir(io, "accounts", .default_dir);
    const root = try temporary.dir.openDir(io, "accounts", .{ .iterate = true });
    defer root.close(io);

    try save(io, root, .{
        .id = "0123456789abcdef0123456789abcdef",
        .player_name = "Player",
        .client_id = "00001111-aaaa-2222-bbbb-3333cccc4444",
        .xuid = "123",
    });
    var document = try load(std.testing.allocator, io, root, "0123456789abcdef0123456789abcdef");
    defer document.deinit();
    try std.testing.expectEqualStrings("Player", document.value.player_name);

    var accounts = try list(std.testing.allocator, io, root);
    defer accounts.deinit();
    try std.testing.expectEqual(@as(usize, 1), accounts.items.len);
    try std.testing.expectEqualStrings("00001111-aaaa-2222-bbbb-3333cccc4444", accounts.items[0].value.client_id);

    try delete(std.testing.allocator, io, root, "0123456789abcdef0123456789abcdef");
    try std.testing.expectError(error.FileNotFound, load(std.testing.allocator, io, root, "0123456789abcdef0123456789abcdef"));
}
