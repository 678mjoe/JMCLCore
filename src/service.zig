const std = @import("std");
const authentication = @import("authentication.zig");
const authentication_http = @import("authentication_http.zig");
const account_store = @import("account_store.zig");
const asset_index = @import("asset_index.zig");
const core = @import("root.zig");
const fabric = @import("fabric.zig");
const install_plan = @import("install_plan.zig");
const neoforge = @import("neoforge.zig");
const version_manifest = @import("version_manifest.zig");
const version_metadata = @import("version_metadata.zig");
const zip_reader = @import("zip_reader.zig");

const max_manifest_bytes = 2 * 1024 * 1024;
const max_metadata_bytes = 4 * 1024 * 1024;
const max_asset_index_bytes = 16 * 1024 * 1024;
const max_sha1_sidecar_bytes = 4 * 1024;

pub const VersionSelection = struct {
    id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
};


const FabricResolvedMetadata = struct {
    parent: std.json.Parsed(version_metadata.Metadata),
    profile: std.json.Parsed(fabric.Profile),
    overlay: fabric.Overlay,
};

pub const NeoForgeResolvedMetadata = struct {
    parent: std.json.Parsed(version_metadata.Metadata),
    version_profile: std.json.Parsed(neoforge.VersionProfile),
    install_profile: std.json.Parsed(neoforge.InstallProfile),
    overlay: neoforge.Overlay,
    installer_sha1: []const u8,
    installer_size: u64,
    allocator: std.mem.Allocator,
};

pub const ResolvedMetadata = union(enum) {
    vanilla: std.json.Parsed(version_metadata.Metadata),
    fabric: FabricResolvedMetadata,
    neoforge: NeoForgeResolvedMetadata,

    pub fn deinit(resolved: *ResolvedMetadata) void {
        switch (resolved.*) {
            .vanilla => |*document| document.deinit(),
            .fabric => |*document| {
                document.overlay.deinit();
                document.profile.deinit();
                document.parent.deinit();
            },
            .neoforge => |*document| {
                document.overlay.deinit();
                document.install_profile.deinit();
                document.version_profile.deinit();
                document.parent.deinit();
                document.allocator.free(document.installer_sha1);
            },
        }
        resolved.* = undefined;
    }

    pub fn value(resolved: *const ResolvedMetadata) *const version_metadata.Metadata {
        return switch (resolved.*) {
            .vanilla => |*document| &document.value,
            .fabric => |*document| &document.overlay.value,
            .neoforge => |*document| &document.overlay.value,
        };
    }
};

const FabricIntegrities = struct {
    allocator: std.mem.Allocator,
    hashes: []u8,
    values: []fabric.LibraryIntegrity,

    fn deinit(integrities: *FabricIntegrities) void {
        integrities.allocator.free(integrities.hashes);
        integrities.allocator.free(integrities.values);
        integrities.* = undefined;
    }
};

const ResponseBody = struct {
    allocator: std.mem.Allocator,
    storage: []u8,
    content: []const u8,

    fn deinit(body: *ResponseBody) void {
        body.allocator.free(body.storage);
        body.* = undefined;
    }
};

pub const Service = struct {
    allocator: std.mem.Allocator,
    io: std.Io,
    environ_map: ?*const std.process.Environ.Map,

    pub fn init(
        allocator: std.mem.Allocator,
        io: std.Io,
        environ_map: ?*const std.process.Environ.Map,
    ) Service {
        return .{
            .allocator = allocator,
            .io = io,
            .environ_map = environ_map,
        };
    }

    pub fn fetchVersionManifest(
        self: Service,
        source: core.DownloadSource,
    ) !std.json.Parsed(version_manifest.Manifest) {
        var body = try self.fetchBody(source.manifestUrl(), max_manifest_bytes);
        defer body.deinit();

        return version_manifest.parse(self.allocator, body.content);
    }

    pub fn beginMicrosoftDeviceAuthorization(
        self: Service,
        config: authentication.Config,
    ) !authentication.DeviceAuthorizationResult {
        var adapter: authentication_http.StdHttpTransport = .{
            .io = self.io,
            .environ_map = self.environ_map,
            .user_agent = core.name ++ "/" ++ core.version,
        };
        return (authentication.MicrosoftOAuth{ .transport = adapter.transport() }).deviceBegin(self.allocator, config);
    }

    pub fn pollMicrosoftDeviceAuthorization(
        self: Service,
        client_id: []const u8,
        device_code: []const u8,
    ) !authentication.DevicePollResultOwned {
        var adapter: authentication_http.StdHttpTransport = .{
            .io = self.io,
            .environ_map = self.environ_map,
            .user_agent = core.name ++ "/" ++ core.version,
        };
        return (authentication.MicrosoftOAuth{ .transport = adapter.transport() }).devicePoll(
            self.allocator,
            client_id,
            device_code,
        );
    }

    pub fn refreshMicrosoftAuthorization(
        self: Service,
        client_id: []const u8,
        refresh_token: []const u8,
    ) !authentication.OAuthCredentialResult {
        var adapter: authentication_http.StdHttpTransport = .{
            .io = self.io,
            .environ_map = self.environ_map,
            .user_agent = core.name ++ "/" ++ core.version,
        };
        return (authentication.MicrosoftOAuth{ .transport = adapter.transport() }).refresh(
            self.allocator,
            client_id,
            refresh_token,
        );
    }

    pub fn refreshMinecraftSession(
        self: Service,
        client_id: []const u8,
        refresh_token: []const u8,
    ) !authentication.RefreshExchangeResult {
        var adapter: authentication_http.StdHttpTransport = .{
            .io = self.io,
            .environ_map = self.environ_map,
            .user_agent = core.name ++ "/" ++ core.version,
        };
        return (authentication.MicrosoftOAuth{ .transport = adapter.transport() }).refreshMinecraftSession(
            self.allocator,
            client_id,
            refresh_token,
        );
    }

    pub fn exchangeMinecraftSession(
        self: Service,
        microsoft: authentication.MicrosoftAccess,
    ) !authentication.SessionResult {
        var adapter: authentication_http.StdHttpTransport = .{
            .io = self.io,
            .environ_map = self.environ_map,
            .user_agent = core.name ++ "/" ++ core.version,
        };
        return (authentication.MicrosoftOAuth{ .transport = adapter.transport() }).minecraftExchange(
            self.allocator,
            microsoft,
        );
    }

    pub fn saveAccount(self: Service, directory: []const u8, account: account_store.Account) !void {
        const root = try account_store.openRoot(self.io, directory);
        defer root.close(self.io);
        try account_store.save(self.io, root, account);
    }

    pub fn loadAccount(self: Service, directory: []const u8, id: []const u8) !account_store.Document {
        const root = try account_store.openRoot(self.io, directory);
        defer root.close(self.io);
        return account_store.load(self.allocator, self.io, root, id);
    }

    pub fn listAccounts(self: Service, directory: []const u8) !account_store.List {
        const root = try account_store.openRoot(self.io, directory);
        defer root.close(self.io);
        return account_store.list(self.allocator, self.io, root);
    }

    pub fn deleteAccount(self: Service, directory: []const u8, id: []const u8) !void {
        const root = try account_store.openRoot(self.io, directory);
        defer root.close(self.io);
        try account_store.delete(self.allocator, self.io, root, id);
    }

    pub fn fetchVersionMetadata(
        self: Service,
        version_id: []const u8,
        source: core.DownloadSource,
    ) !std.json.Parsed(version_metadata.Metadata) {
        var manifest = try self.fetchVersionManifest(source);
        defer manifest.deinit();

        const selected = for (manifest.value.versions) |version| {
            if (std.mem.eql(u8, version.id, version_id)) break version;
        } else return error.VersionNotFound;

        const mirror_url = try source.rewriteUrl(self.allocator, selected.url);
        defer if (mirror_url) |url| self.allocator.free(url);

        var body = try self.fetchBody(mirror_url orelse selected.url, max_metadata_bytes);
        defer body.deinit();

        if (selected.sha1) |expected| {
            if (!version_metadata.verifySha1(body.content, expected)) return error.MetadataHashMismatch;
        }

        var metadata = try version_metadata.parse(self.allocator, body.content);
        errdefer metadata.deinit();
        if (!std.mem.eql(u8, metadata.value.id, version_id)) return error.MetadataIdMismatch;
        return metadata;
    }

    pub fn resolveMetadata(
        self: Service,
        selection: VersionSelection,
        source: core.DownloadSource,
    ) !ResolvedMetadata {
        if (selection.fabric_loader != null and selection.neoforge_version != null) return error.LoaderConflict;
        if (selection.fabric_loader == null and selection.neoforge_version == null) {
            return .{ .vanilla = try self.fetchVersionMetadata(selection.id, source) };
        }
        if (selection.neoforge_version) |neoforge_version| return self.resolveNeoForgeMetadata(selection.id, neoforge_version, source);

        var parent = try self.fetchVersionMetadata(selection.id, source);
        errdefer parent.deinit();
        var profile = try self.fetchFabricProfile(selection.id, selection.fabric_loader.?);
        errdefer profile.deinit();
        if (!std.mem.eql(u8, profile.value.inheritsFrom, selection.id)) return error.FabricParentMismatch;

        const expected_id = try std.fmt.allocPrint(
            self.allocator,
            "fabric-loader-{s}-{s}",
            .{ selection.fabric_loader.?, selection.id },
        );
        defer self.allocator.free(expected_id);
        if (!std.mem.eql(u8, profile.value.id, expected_id)) return error.FabricProfileIdMismatch;

        var integrities = try self.fetchFabricLibraryIntegrities(profile.value.libraries);
        defer integrities.deinit();
        const overlay = try fabric.merge(self.allocator, parent.value, profile.value, integrities.values);
        return .{ .fabric = .{
            .parent = parent,
            .profile = profile,
            .overlay = overlay,
        } };
    }

    pub fn resolveMetadataPreferred(
        self: Service,
        selection: VersionSelection,
        source: core.DownloadSource,
    ) !ResolvedMetadata {
        return self.resolveMetadata(selection, source) catch |primary_error| {
            if (source != .bmclapi) return primary_error;
            return self.resolveMetadata(selection, .official);
        };
    }

    pub fn fetchFabricProfile(
        self: Service,
        minecraft_version: []const u8,
        loader_version: []const u8,
    ) !std.json.Parsed(fabric.Profile) {
        const url = try fabric.profileUrlAlloc(self.allocator, minecraft_version, loader_version);
        defer self.allocator.free(url);
        var body = try self.fetchBody(url, fabric.max_profile_bytes);
        defer body.deinit();
        return fabric.parse(self.allocator, body.content);
    }

    fn resolveNeoForgeMetadata(
        self: Service,
        minecraft_version: []const u8,
        neoforge_version: []const u8,
        source: core.DownloadSource,
    ) !ResolvedMetadata {
        try neoforge.validateVersionId(neoforge_version);
        var parent = try self.fetchVersionMetadata(minecraft_version, source);
        errdefer parent.deinit();

        const installer_url = try neoforge.installerUrlAlloc(self.allocator, neoforge_version);
        defer self.allocator.free(installer_url);
        var installer = try self.fetchBody(installer_url, neoforge.max_installer_bytes);
        defer installer.deinit();

        const version_source = (try zip_reader.readEntryAlloc(
            self.allocator,
            installer.content,
            "version.json",
            neoforge.max_version_profile_bytes,
        )) orelse return error.NeoForgeVersionProfileMissing;
        defer self.allocator.free(version_source);
        const install_source = (try zip_reader.readEntryAlloc(
            self.allocator,
            installer.content,
            "install_profile.json",
            neoforge.max_install_profile_bytes,
        )) orelse return error.NeoForgeInstallProfileMissing;
        defer self.allocator.free(install_source);

        var version_profile = try neoforge.parseVersionProfile(self.allocator, version_source);
        errdefer version_profile.deinit();
        var install_profile = try neoforge.parseInstallProfile(self.allocator, install_source);
        errdefer install_profile.deinit();

        if (!std.mem.eql(u8, install_profile.value.minecraft, minecraft_version)) return error.NeoForgeParentMismatch;
        if (!std.mem.eql(u8, version_profile.value.inheritsFrom, minecraft_version)) return error.NeoForgeParentMismatch;
        const expected_id = try std.fmt.allocPrint(self.allocator, "neoforge-{s}", .{neoforge_version});
        defer self.allocator.free(expected_id);
        if (!std.mem.eql(u8, version_profile.value.id, expected_id)) return error.NeoForgeProfileIdMismatch;
        if (!std.mem.eql(u8, install_profile.value.version, expected_id)) return error.NeoForgeProfileIdMismatch;

        const patched_path = try neoforge.patchedClientPathAlloc(self.allocator, install_profile.value);
        defer self.allocator.free(patched_path);
        const overlay = try neoforge.merge(self.allocator, parent.value, version_profile.value, patched_path);

        var installer_digest: [std.crypto.hash.Sha1.digest_length]u8 = undefined;
        std.crypto.hash.Sha1.hash(installer.content, &installer_digest, .{});
        const installer_sha1 = try std.fmt.allocPrint(self.allocator, "{s}", .{
            std.fmt.bytesToHex(installer_digest, .lower),
        });
        errdefer self.allocator.free(installer_sha1);

        return .{ .neoforge = .{
            .parent = parent,
            .version_profile = version_profile,
            .install_profile = install_profile,
            .overlay = overlay,
            .installer_sha1 = installer_sha1,
            .installer_size = installer.content.len,
            .allocator = self.allocator,
        } };
    }

    pub fn createInstallPlan(
        self: Service,
        version_id: []const u8,
        source: core.DownloadSource,
        target: install_plan.Target,
    ) !install_plan.Plan {
        return self.createInstallPlanForSelection(.{ .id = version_id }, source, target);
    }

    pub fn createInstallPlanForSelection(
        self: Service,
        selection: VersionSelection,
        source: core.DownloadSource,
        target: install_plan.Target,
    ) !install_plan.Plan {
        return self.createInstallPlanFromSource(selection, source, source, target) catch |primary_error| {
            if (source != .bmclapi) return primary_error;
            return self.createInstallPlanFromSource(selection, .official, source, target);
        };
    }

    pub fn fetchVersionMetadataPreferred(
        self: Service,
        version_id: []const u8,
        source: core.DownloadSource,
    ) !std.json.Parsed(version_metadata.Metadata) {
        return self.fetchVersionMetadata(version_id, source) catch |primary_error| {
            if (source != .bmclapi) return primary_error;
            return self.fetchVersionMetadata(version_id, .official);
        };
    }

    fn createInstallPlanFromSource(
        self: Service,
        selection: VersionSelection,
        metadata_source: core.DownloadSource,
        download_source: core.DownloadSource,
        target: install_plan.Target,
    ) !install_plan.Plan {
        var metadata_document = try self.resolveMetadata(selection, metadata_source);
        defer metadata_document.deinit();

        const metadata = metadata_document.value();
        const asset_document: ?std.json.Parsed(asset_index.Index) = if (metadata.assetIndex) |descriptor|
            try self.fetchAssetIndex(descriptor, metadata_source)
        else
            null;
        defer if (asset_document) |document| document.deinit();

        switch (metadata_document) {
            .neoforge => |*document| {
                var extras = try self.buildNeoForgeExtras(selection.neoforge_version.?, document);
                defer extras.deinit();
                return install_plan.buildWithExtras(
                    self.allocator,
                    metadata.*,
                    if (asset_document) |document_value| document_value.value else null,
                    download_source,
                    target,
                    extras.files,
                );
            },
            else => return install_plan.build(
                self.allocator,
                metadata.*,
                if (asset_document) |document| document.value else null,
                download_source,
                target,
            ),
        }
    }

    const NeoForgeExtras = struct {
        allocator: std.mem.Allocator,
        arena: *std.heap.ArenaAllocator,
        files: []install_plan.File,

        fn deinit(extras: *NeoForgeExtras) void {
            const allocator = extras.arena.child_allocator;
            extras.arena.deinit();
            allocator.destroy(extras.arena);
            extras.* = undefined;
        }
    };

    fn buildNeoForgeExtras(
        self: Service,
        neoforge_version: []const u8,
        document: *const NeoForgeResolvedMetadata,
    ) !NeoForgeExtras {
        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const scratch = arena.allocator();

        const profile_libraries = document.install_profile.value.libraries;
        const files = try scratch.alloc(install_plan.File, profile_libraries.len + 1);
        const installer_url = try neoforge.installerUrlAlloc(scratch, neoforge_version);
        files[0] = .{
            .path = try std.fmt.allocPrint(scratch, ".jmcl/neoforge/{s}/installer.jar", .{neoforge_version}),
            .url = installer_url,
            .fallback_url = null,
            .sha1 = document.installer_sha1,
            .size = document.installer_size,
            .usage = .{ .library = true },
        };
        for (profile_libraries, 1..) |library, index| {
            const artifact = library.downloads.?.artifact.?;
            const artifact_path = try neoforge.artifactPathAlloc(scratch, library.name);
            files[index] = .{
                .path = try std.fmt.allocPrint(scratch, "libraries/{s}", .{artifact_path}),
                .url = artifact.url,
                .fallback_url = null,
                .sha1 = artifact.sha1,
                .size = artifact.size,
                .usage = .{ .library = true },
            };
        }
        return .{
            .allocator = self.allocator,
            .arena = arena,
            .files = files,
        };
    }

    fn fetchFabricLibraryIntegrities(self: Service, libraries: []const fabric.Library) !FabricIntegrities {
        const hashes = try self.allocator.alloc(u8, try std.math.mul(usize, libraries.len, 40));
        errdefer self.allocator.free(hashes);
        const values = try self.allocator.alloc(fabric.LibraryIntegrity, libraries.len);
        errdefer self.allocator.free(values);

        for (libraries, 0..) |library, index| {
            const artifact_url = try fabric.artifactUrlAlloc(self.allocator, library.url, library.name);
            defer self.allocator.free(artifact_url);
            const sha1 = hashes[index * 40 ..][0..40];
            if (library.sha1) |provided| {
                try fabric.validateSha1(provided);
                @memcpy(sha1, provided);
            } else {
                try self.fetchFabricSha1(artifact_url, sha1);
            }
            const size = if (library.size) |provided| provided else try self.fetchContentLength(artifact_url);
            if (size == 0 or size > fabric.max_library_size) return error.InvalidFabricLibrarySize;
            values[index] = .{ .sha1 = sha1, .size = size };
        }
        return .{
            .allocator = self.allocator,
            .hashes = hashes,
            .values = values,
        };
    }

    fn fetchFabricSha1(self: Service, artifact_url: []const u8, output: []u8) !void {
        if (output.len != 40) return error.InvalidFabricSha1Destination;
        const sidecar_url = try std.fmt.allocPrint(self.allocator, "{s}.sha1", .{artifact_url});
        defer self.allocator.free(sidecar_url);
        var body = try self.fetchBody(sidecar_url, max_sha1_sidecar_bytes);
        defer body.deinit();
        const token_end = std.mem.indexOfAny(u8, body.content, " \t\r\n") orelse body.content.len;
        const token = body.content[0..token_end];
        try fabric.validateSha1(token);
        @memcpy(output, token);
    }

    fn fetchContentLength(self: Service, url: []const u8) !u64 {
        var proxy_arena = std.heap.ArenaAllocator.init(self.allocator);
        defer proxy_arena.deinit();
        var client: std.http.Client = .{
            .allocator = self.allocator,
            .io = self.io,
        };
        defer client.deinit();
        if (self.environ_map) |environ_map| try client.initDefaultProxies(proxy_arena.allocator(), environ_map);

        const uri = try std.Uri.parse(url);
        var request = try client.request(.HEAD, uri, .{
            .redirect_behavior = .unhandled,
            .headers = .{ .user_agent = .{ .override = core.name ++ "/" ++ core.version } },
        });
        defer request.deinit();
        try request.sendBodiless();
        var header_buffer: [8 * 1024]u8 = undefined;
        const response = try request.receiveHead(&header_buffer);
        if (response.head.status.class() != .success) return error.UnsuccessfulHttpStatus;
        return response.head.content_length orelse error.MissingContentLength;
    }

    pub fn fetchAssetIndex(
        self: Service,
        descriptor: version_metadata.AssetIndex,
        source: core.DownloadSource,
    ) !std.json.Parsed(asset_index.Index) {
        const mirror_url = try source.rewriteUrl(self.allocator, descriptor.url);
        defer if (mirror_url) |url| self.allocator.free(url);

        var body = try self.fetchBody(mirror_url orelse descriptor.url, max_asset_index_bytes);
        defer body.deinit();
        if (!asset_index.verifySha1(body.content, descriptor.sha1)) return error.AssetIndexHashMismatch;
        return asset_index.parse(self.allocator, body.content);
    }

    fn fetchBody(self: Service, url: []const u8, max_bytes: usize) !ResponseBody {
        var proxy_arena = std.heap.ArenaAllocator.init(self.allocator);
        defer proxy_arena.deinit();

        var client: std.http.Client = .{
            .allocator = self.allocator,
            .io = self.io,
        };
        defer client.deinit();

        if (self.environ_map) |environ_map| {
            try client.initDefaultProxies(proxy_arena.allocator(), environ_map);
        }

        const storage = try self.allocator.alloc(u8, max_bytes + 1);
        errdefer self.allocator.free(storage);
        var body = std.Io.Writer.fixed(storage);

        const result = client.fetch(.{
            .location = .{ .url = url },
            .response_writer = &body,
            .redirect_behavior = std.http.Client.Request.RedirectBehavior.init(3),
            .headers = .{
                .user_agent = .{ .override = core.name ++ "/" ++ core.version },
            },
        }) catch |err| {
            if (err == error.WriteFailed and body.end > max_bytes) {
                return error.ResponseBodyTooLarge;
            }
            return err;
        };

        if (body.end > max_bytes) return error.ResponseBodyTooLarge;
        if (result.status.class() != .success) return error.UnsuccessfulHttpStatus;

        return .{
            .allocator = self.allocator,
            .storage = storage,
            .content = storage[0..body.end],
        };
    }
};
