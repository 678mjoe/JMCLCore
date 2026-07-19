const std = @import("std");
const builtin = @import("builtin");
const max_windows_path = 32 * 1024;

extern "kernel32" fn CreateHardLinkW(
    new_file_name: [*:0]const u16,
    existing_file_name: [*:0]const u16,
    security_attributes: ?*anyopaque,
) callconv(.winapi) std.os.windows.BOOL;
extern "kernel32" fn GetFinalPathNameByHandleW(
    file: std.os.windows.HANDLE,
    file_path: [*]u16,
    file_path_size: std.os.windows.DWORD,
    flags: std.os.windows.DWORD,
) callconv(.winapi) std.os.windows.DWORD;
extern "kernel32" fn GetFileAttributesW(file_name: [*:0]const u16) callconv(.winapi) std.os.windows.DWORD;
extern "kernel32" fn SetFileAttributesW(
    file_name: [*:0]const u16,
    attributes: std.os.windows.DWORD,
) callconv(.winapi) std.os.windows.BOOL;
extern "kernel32" fn FlushFileBuffers(file: std.os.windows.HANDLE) callconv(.winapi) std.os.windows.BOOL;
extern "kernel32" fn GetFileInformationByHandleEx(
    file: std.os.windows.HANDLE,
    file_information_class: std.os.windows.DWORD,
    file_information: *anyopaque,
    buffer_size: std.os.windows.DWORD,
) callconv(.winapi) std.os.windows.BOOL;
extern "kernel32" fn SetFileInformationByHandle(
    file: std.os.windows.HANDLE,
    file_information_class: std.os.windows.DWORD,
    file_information: *const anyopaque,
    buffer_size: std.os.windows.DWORD,
) callconv(.winapi) std.os.windows.BOOL;
extern "kernel32" fn ReadFile(
    file: std.os.windows.HANDLE,
    buffer: [*]u8,
    bytes_to_read: std.os.windows.DWORD,
    bytes_read: *std.os.windows.DWORD,
    overlapped: *WindowsOverlapped,
) callconv(.winapi) std.os.windows.BOOL;
extern "kernel32" fn GetOverlappedResult(
    file: std.os.windows.HANDLE,
    overlapped: *WindowsOverlapped,
    bytes_transferred: *std.os.windows.DWORD,
    wait: std.os.windows.BOOL,
) callconv(.winapi) std.os.windows.BOOL;

const WindowsOverlapped = extern struct {
    internal: usize = 0,
    internal_high: usize = 0,
    offset: std.os.windows.DWORD,
    offset_high: std.os.windows.DWORD,
    event: ?std.os.windows.HANDLE = null,
};

const WindowsFileBasicInfo = extern struct {
    creation_time: i64,
    last_access_time: i64,
    last_write_time: i64,
    change_time: i64,
    file_attributes: std.os.windows.DWORD,
};

const WindowsFileDispositionInfo = extern struct {
    delete_file: u8,
};

const file_basic_information: std.os.windows.DWORD = 0;
const file_disposition_information: std.os.windows.DWORD = 4;

pub fn create(
    allocator: std.mem.Allocator,
    io: std.Io,
    source_dir: std.Io.Dir,
    source_path: []const u8,
    destination_dir: std.Io.Dir,
    destination_path: []const u8,
) !void {
    if (builtin.os.tag == .windows) {
        return createWindows(allocator, io, source_dir, source_path, destination_dir, destination_path);
    }
    return source_dir.hardLink(source_path, destination_dir, destination_path, io, .{}) catch |err| switch (err) {
        error.PermissionDenied => error.AccessDenied,
        else => |other| other,
    };
}

fn createWindows(
    allocator: std.mem.Allocator,
    io: std.Io,
    source_dir: std.Io.Dir,
    source_path: []const u8,
    destination_dir: std.Io.Dir,
    destination_path: []const u8,
) !void {
    const windows = std.os.windows;
    _ = io;
    const source_w = try pathFromDirWindows(allocator, source_dir, source_path);
    defer allocator.free(source_w);
    const destination_w = try pathFromDirWindows(allocator, destination_dir, destination_path);
    defer allocator.free(destination_w);

    if (CreateHardLinkW(destination_w, source_w, null).toBool()) return;
    return switch (windows.GetLastError()) {
        .FILE_NOT_FOUND, .PATH_NOT_FOUND => error.FileNotFound,
        .ACCESS_DENIED, .PRIVILEGE_NOT_HELD => error.AccessDenied,
        .NOT_SAME_DEVICE => error.CrossDevice,
        .FILE_EXISTS, .ALREADY_EXISTS => error.PathAlreadyExists,
        .NOT_SUPPORTED, .INVALID_FUNCTION => error.OperationUnsupported,
        .TOO_MANY_LINKS => error.LinkQuotaExceeded,
        .DISK_FULL, .HANDLE_DISK_FULL => error.NoSpaceLeft,
        else => error.Unexpected,
    };
}

pub fn setReadOnly(
    allocator: std.mem.Allocator,
    io: std.Io,
    dir: std.Io.Dir,
    path: []const u8,
    read_only: bool,
) !void {
    if (builtin.os.tag != .windows) {
        const file = try dir.openFile(io, path, .{
            .mode = .read_only,
            .allow_directory = false,
            .follow_symlinks = false,
            .resolve_beneath = true,
        });
        defer file.close(io);
        return file.setPermissions(io, std.Io.File.Permissions.default_file.setReadOnly(read_only));
    }
    const windows = std.os.windows;
    const path_w = try pathFromDirWindows(allocator, dir, path);
    defer allocator.free(path_w);
    const invalid_attributes = std.math.maxInt(windows.DWORD);
    const current = GetFileAttributesW(path_w);
    if (current == invalid_attributes) return error.AccessDenied;
    const read_only_flag: windows.DWORD = 1;
    const updated = if (read_only) current | read_only_flag else current & ~read_only_flag;
    if (updated == current or SetFileAttributesW(path_w, updated).toBool()) return;
    return switch (windows.GetLastError()) {
        .FILE_NOT_FOUND, .PATH_NOT_FOUND => error.FileNotFound,
        .ACCESS_DENIED, .PRIVILEGE_NOT_HELD => error.AccessDenied,
        else => error.Unexpected,
    };
}

pub fn absolutePathAlloc(
    allocator: std.mem.Allocator,
    io: std.Io,
    dir: std.Io.Dir,
    relative_path: []const u8,
) ![:0]u8 {
    if (builtin.os.tag != .windows) return dir.realPathFileAlloc(io, relative_path, allocator);
    const wide = if (std.mem.eql(u8, relative_path, "."))
        try directoryPathWindows(allocator, dir)
    else
        try pathFromDirWindows(allocator, dir, relative_path);
    defer allocator.free(wide);
    return std.unicode.wtf16LeToWtf8AllocZ(allocator, wide);
}

pub fn syncFile(io: std.Io, file: std.Io.File) !void {
    if (builtin.os.tag != .windows) return file.sync(io);
    if (FlushFileBuffers(file.handle).toBool()) return;
    return switch (std.os.windows.GetLastError()) {
        .ACCESS_DENIED => error.AccessDenied,
        .INVALID_HANDLE => error.InvalidHandle,
        else => error.Unexpected,
    };
}

/// Deletes an already-open Windows file without resolving its pathname again.
/// This keeps a checked cache object from being swapped for a reparse point.
pub fn deleteReadOnlyWindows(file: std.Io.File) !void {
    if (builtin.os.tag != .windows) return error.OperationUnsupported;
    var basic: WindowsFileBasicInfo = undefined;
    if (!GetFileInformationByHandleEx(
        file.handle,
        file_basic_information,
        &basic,
        @sizeOf(WindowsFileBasicInfo),
    ).toBool()) return mapDeleteError(std.os.windows.GetLastError());
    const original_attributes = basic.file_attributes;
    basic.file_attributes &= ~@as(std.os.windows.DWORD, 1);
    if (!SetFileInformationByHandle(
        file.handle,
        file_basic_information,
        &basic,
        @sizeOf(WindowsFileBasicInfo),
    ).toBool()) return mapDeleteError(std.os.windows.GetLastError());

    const disposition: WindowsFileDispositionInfo = .{ .delete_file = 1 };
    if (SetFileInformationByHandle(
        file.handle,
        file_disposition_information,
        &disposition,
        @sizeOf(WindowsFileDispositionInfo),
    ).toBool()) return;

    const delete_error = std.os.windows.GetLastError();

    basic.file_attributes = original_attributes;
    if (!SetFileInformationByHandle(
        file.handle,
        file_basic_information,
        &basic,
        @sizeOf(WindowsFileBasicInfo),
    ).toBool()) return error.Unexpected;
    return mapDeleteError(delete_error);
}

/// Opens one basename below an already-open directory with the rights required
/// for handle-based attribute changes and deletion. Reparse points are opened,
/// never followed.
pub fn openForDeleteWindows(
    allocator: std.mem.Allocator,
    dir: std.Io.Dir,
    basename: []const u8,
) !std.Io.File {
    if (builtin.os.tag != .windows) return error.OperationUnsupported;
    if (basename.len == 0 or std.mem.indexOfAny(u8, basename, "/\\") != null) return error.InvalidRelativePath;
    const windows = std.os.windows;
    const name = try std.unicode.wtf8ToWtf16LeAllocZ(allocator, basename);
    defer allocator.free(name);
    var io_status: windows.IO_STATUS_BLOCK = undefined;
    var handle: windows.HANDLE = undefined;
    switch (windows.ntdll.NtCreateFile(
        &handle,
        .{
            .STANDARD = .{
                .RIGHTS = .{ .DELETE = true },
                .SYNCHRONIZE = true,
            },
            .SPECIFIC = .{ .FILE = .{
                .READ_ATTRIBUTES = true,
                .WRITE_ATTRIBUTES = true,
            } },
        },
        &.{
            .RootDirectory = dir.handle,
            .ObjectName = @constCast(&windows.UNICODE_STRING.init(name[0..name.len])),
        },
        &io_status,
        null,
        .{ .NORMAL = true },
        .VALID_FLAGS,
        .OPEN,
        .{
            .IO = .ASYNCHRONOUS,
            .NON_DIRECTORY_FILE = true,
            .OPEN_REPARSE_POINT = true,
        },
        null,
        0,
    )) {
        .SUCCESS => return .{ .handle = handle, .flags = .{ .nonblocking = false } },
        .OBJECT_NAME_NOT_FOUND, .OBJECT_PATH_NOT_FOUND => return error.FileNotFound,
        .ACCESS_DENIED, .USER_MAPPED_FILE => return error.AccessDenied,
        .SHARING_VIOLATION, .DELETE_PENDING => return error.PathBusy,
        .FILE_IS_A_DIRECTORY => return error.IsDir,
        .NOT_A_DIRECTORY => return error.NotDir,
        else => return error.Unexpected,
    }
}

fn mapDeleteError(last_error: std.os.windows.Win32Error) !void {
    return switch (last_error) {
        .FILE_NOT_FOUND, .PATH_NOT_FOUND => error.FileNotFound,
        .ACCESS_DENIED => error.AccessDenied,
        .SHARING_VIOLATION, .LOCK_VIOLATION => error.PathBusy,
        else => error.Unexpected,
    };
}

pub fn readFileWindows(file: std.Io.File, buffer: []u8, offset: u64) !usize {
    if (builtin.os.tag != .windows) return error.OperationUnsupported;
    var bytes_read: std.os.windows.DWORD = 0;
    var overlapped: WindowsOverlapped = .{
        .offset = @truncate(offset),
        .offset_high = @truncate(offset >> 32),
    };
    const requested: std.os.windows.DWORD = @intCast(@min(buffer.len, std.math.maxInt(std.os.windows.DWORD)));
    if (ReadFile(file.handle, buffer.ptr, requested, &bytes_read, &overlapped).toBool()) return bytes_read;
    if (std.os.windows.GetLastError() == .IO_PENDING) {
        if (GetOverlappedResult(file.handle, &overlapped, &bytes_read, std.os.windows.BOOL.TRUE).toBool()) return bytes_read;
    }
    return switch (std.os.windows.GetLastError()) {
        .HANDLE_EOF, .BROKEN_PIPE => 0,
        .ACCESS_DENIED => error.AccessDenied,
        .INVALID_HANDLE => error.InvalidHandle,
        .OPERATION_ABORTED => error.Canceled,
        .INVALID_PARAMETER => error.InvalidArgument,
        else => error.Unexpected,
    };
}

fn pathFromDirWindows(allocator: std.mem.Allocator, dir: std.Io.Dir, relative_path: []const u8) ![:0]u16 {
    if (std.fs.path.isAbsolute(relative_path)) return error.AbsolutePathNotAllowed;
    var iterator = std.mem.splitAny(u8, relative_path, "/\\");
    while (iterator.next()) |component| {
        if (component.len == 0 or std.mem.eql(u8, component, ".") or std.mem.eql(u8, component, "..")) {
            return error.InvalidRelativePath;
        }
    }

    const root = try directoryPathWindows(allocator, dir);
    defer allocator.free(root);
    const root_len = root.len;
    const relative_w = try std.unicode.wtf8ToWtf16LeAllocZ(allocator, relative_path);
    defer allocator.free(relative_w);
    const needs_separator = root_len != 0 and root[root_len - 1] != '\\';
    const total_len = root_len + @intFromBool(needs_separator) + relative_w.len;
    if (total_len >= max_windows_path) return error.NameTooLong;
    const result = try allocator.allocSentinel(u16, total_len, 0);
    @memcpy(result[0..root_len], root[0..root_len]);
    var index: usize = root_len;
    if (needs_separator) {
        result[index] = '\\';
        index += 1;
    }
    for (relative_w[0..relative_w.len], index..) |character, output_index| {
        result[output_index] = if (character == '/') '\\' else character;
    }
    return result;
}

fn directoryPathWindows(allocator: std.mem.Allocator, dir: std.Io.Dir) ![:0]u16 {
    const buffer = try allocator.alloc(u16, max_windows_path);
    defer allocator.free(buffer);
    const length = GetFinalPathNameByHandleW(dir.handle, buffer.ptr, max_windows_path, 0);
    if (length == 0) return error.Unexpected;
    if (length >= buffer.len) return error.NameTooLong;
    return allocator.dupeZ(u16, buffer[0..length]);
}

test "creates a hard link" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    const source = try temporary.dir.createFile(io, "source", .{});
    try std.Io.File.writeStreamingAll(source, io, "shared");
    source.close(io);
    try create(std.testing.allocator, io, temporary.dir, "source", temporary.dir, "destination");
    const source_stat = try temporary.dir.statFile(io, "source", .{});
    const destination_stat = try temporary.dir.statFile(io, "destination", .{});
    try std.testing.expectEqual(source_stat.inode, destination_stat.inode);
    try std.testing.expect(source_stat.nlink >= 2);
}

test "absolute path allocation retains its sentinel" {
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();
    const path = try absolutePathAlloc(std.testing.allocator, io, temporary.dir, ".");
    defer std.testing.allocator.free(path);
    try std.testing.expect(path.len != 0);
    try std.testing.expectEqual(@as(u8, 0), path[path.len]);
}
