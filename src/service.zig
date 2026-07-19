const std = @import("std");
const artifact_store = @import("install/artifact_store.zig");
const authentication = @import("auth/authentication.zig");
const authentication_http = @import("auth/http.zig");
const account_store = @import("auth/account_store.zig");
const asset_index = @import("install/asset_index.zig");
const content_registry = @import("mods/content_registry.zig");
const core = @import("root.zig");
const fabric = @import("loaders/fabric.zig");
const forge = @import("loaders/forge.zig");
const install_plan = @import("install/plan.zig");
const install_executor = @import("install/installer.zig");
const modrinth = @import("mods/modrinth.zig");
const neoforge = @import("loaders/neoforge.zig");
const version_manifest = @import("install/version_manifest.zig");
const version_metadata = @import("install/version_metadata.zig");
const zip_reader = @import("install/zip_reader.zig");

const max_manifest_bytes = 2 * 1024 * 1024;
const max_metadata_bytes = 4 * 1024 * 1024;
const max_asset_index_bytes = 16 * 1024 * 1024;
const max_sha1_sidecar_bytes = 4 * 1024;

pub const VersionSelection = struct {
    id: []const u8,
    fabric_loader: ?[]const u8 = null,
    neoforge_version: ?[]const u8 = null,
    forge_version: ?[]const u8 = null,
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

pub const ForgeResolvedMetadata = struct {
    parent: std.json.Parsed(version_metadata.Metadata),
    version_profile: std.json.Parsed(forge.VersionProfile),
    install_profile: std.json.Parsed(forge.InstallProfile),
    overlay: forge.Overlay,
    installer_sha1: []const u8,
    installer_size: u64,
    allocator: std.mem.Allocator,
};

pub const ResolvedMetadata = union(enum) {
    vanilla: std.json.Parsed(version_metadata.Metadata),
    fabric: FabricResolvedMetadata,
    neoforge: NeoForgeResolvedMetadata,
    forge: ForgeResolvedMetadata,

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
            .forge => |*document| {
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
            .forge => |*document| &document.overlay.value,
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
        var loaders: u8 = 0;
        if (selection.fabric_loader != null) loaders += 1;
        if (selection.neoforge_version != null) loaders += 1;
        if (selection.forge_version != null) loaders += 1;
        if (loaders > 1) return error.LoaderConflict;
        if (loaders == 0) {
            return .{ .vanilla = try self.fetchVersionMetadata(selection.id, source) };
        }
        if (selection.neoforge_version) |neoforge_version| return self.resolveNeoForgeMetadata(selection.id, neoforge_version, source);
        if (selection.forge_version) |forge_version| return self.resolveForgeMetadata(selection.id, forge_version, source);

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

    fn resolveForgeMetadata(
        self: Service,
        minecraft_version: []const u8,
        forge_version: []const u8,
        source: core.DownloadSource,
    ) !ResolvedMetadata {
        try forge.validateForgeVersion(forge_version);
        var parent = try self.fetchVersionMetadata(minecraft_version, source);
        errdefer parent.deinit();

        const installer_url = try forge.installerUrlAlloc(self.allocator, minecraft_version, forge_version);
        defer self.allocator.free(installer_url);
        var installer = try self.fetchBody(installer_url, neoforge.max_installer_bytes);
        defer installer.deinit();

        const version_source = (try zip_reader.readEntryAlloc(
            self.allocator,
            installer.content,
            "version.json",
            neoforge.max_version_profile_bytes,
        )) orelse return error.ForgeVersionProfileMissing;
        defer self.allocator.free(version_source);
        const install_source = (try zip_reader.readEntryAlloc(
            self.allocator,
            installer.content,
            "install_profile.json",
            neoforge.max_install_profile_bytes,
        )) orelse return error.ForgeInstallProfileMissing;
        defer self.allocator.free(install_source);

        var install_profile = try forge.parseInstallProfile(self.allocator, install_source);
        errdefer install_profile.deinit();
        const patched_coordinate = try neoforge.patchedClientCoordinate(install_profile.value);
        var version_profile = try forge.parseVersionProfile(self.allocator, version_source, patched_coordinate);
        errdefer version_profile.deinit();

        if (!std.mem.eql(u8, install_profile.value.minecraft, minecraft_version)) return error.ForgeParentMismatch;
        if (!std.mem.eql(u8, version_profile.value.inheritsFrom, minecraft_version)) return error.ForgeParentMismatch;
        const expected_id = try forge.profileIdAlloc(self.allocator, minecraft_version, forge_version);
        defer self.allocator.free(expected_id);
        if (!std.mem.eql(u8, version_profile.value.id, expected_id)) return error.ForgeProfileIdMismatch;
        if (!std.mem.eql(u8, install_profile.value.version, expected_id)) return error.ForgeProfileIdMismatch;

        const patched_path = try neoforge.patchedClientPathAlloc(self.allocator, install_profile.value);
        defer self.allocator.free(patched_path);
        const overlay = try forge.merge(
            self.allocator,
            parent.value,
            version_profile.value,
            patched_coordinate,
            patched_path,
        );

        var installer_digest: [std.crypto.hash.Sha1.digest_length]u8 = undefined;
        std.crypto.hash.Sha1.hash(installer.content, &installer_digest, .{});
        const installer_sha1 = try std.fmt.allocPrint(self.allocator, "{s}", .{
            std.fmt.bytesToHex(installer_digest, .lower),
        });
        errdefer self.allocator.free(installer_sha1);

        return .{ .forge = .{
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
            .forge => |*document| {
                var extras = try self.buildForgeExtras(selection.id, selection.forge_version.?, document);
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

    const InstallerExtras = struct {
        allocator: std.mem.Allocator,
        arena: *std.heap.ArenaAllocator,
        files: []install_plan.File,

        fn deinit(extras: *InstallerExtras) void {
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
    ) !InstallerExtras {
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

    fn buildForgeExtras(
        self: Service,
        minecraft_version: []const u8,
        forge_version: []const u8,
        document: *const ForgeResolvedMetadata,
    ) !InstallerExtras {
        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const scratch = arena.allocator();

        const profile_libraries = document.install_profile.value.libraries;
        const files = try scratch.alloc(install_plan.File, profile_libraries.len + 1);
        const installer_url = try forge.installerUrlAlloc(scratch, minecraft_version, forge_version);
        const support_dir = try forge.supportDirAlloc(scratch, minecraft_version, forge_version);
        files[0] = .{
            .path = try std.fmt.allocPrint(scratch, "{s}/installer.jar", .{support_dir}),
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

    pub const ModInstallOptions = struct {
        workers: u8 = 8,
        retries: u8 = 2,
    };

    pub const InstalledContent = struct {
        file: []const u8,
        sha1: []const u8,
        size: u64,
        slug: ?[]const u8,
        as_dependency: bool,
    };

    pub const ContentInstallSummary = struct {
        arena: *std.heap.ArenaAllocator,
        installed: []InstalledContent,
        skipped: usize,

        pub fn deinit(summary: *ContentInstallSummary) void {
            const allocator = summary.arena.child_allocator;
            summary.arena.deinit();
            allocator.destroy(summary.arena);
            summary.* = undefined;
        }
    };

    const ResolvedMod = struct {
        project_id: []const u8,
        slug: ?[]const u8,
        version: modrinth.Version,
        file: modrinth.VersionFile,
        as_dependency: bool,
    };

    /// Resolves a Modrinth project and its required dependencies against the
    /// instance's Minecraft version and loader, downloads every file through
    /// the verified content-addressed pipeline, and registers the results in
    /// the instance content registry.
    pub fn installModrinthProject(
        self: Service,
        minecraft_dir: std.Io.Dir,
        store: std.Io.Dir,
        game_version: []const u8,
        loader: modrinth.Loader,
        project: []const u8,
        version_id: ?[]const u8,
        options: ModInstallOptions,
        reporter: ?install_executor.Reporter,
    ) !ContentInstallSummary {
        try modrinth.validateProjectRef(project);
        try modrinth.validateGameVersion(game_version);
        if (version_id) |id| try modrinth.validateVersionId(id);

        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const scratch = arena.allocator();

        var registry_document = try content_registry.load(self.allocator, self.io, minecraft_dir);
        defer registry_document.deinit();

        var resolved: std.ArrayList(ResolvedMod) = .empty;
        var visited: std.StringHashMap(void) = .init(scratch);
        var skipped: usize = 0;
        try self.resolveModrinthMod(
            scratch,
            game_version,
            loader,
            project,
            version_id,
            false,
            registry_document.value,
            &resolved,
            &visited,
            &skipped,
            0,
        );

        const files = try scratch.alloc(install_plan.File, resolved.items.len);
        var total_size: u64 = 0;
        for (resolved.items, 0..) |mod, index| {
            const path = try std.fmt.allocPrint(scratch, "mods/{s}", .{mod.file.filename});
            if (content_registry.findByFile(registry_document.value, path)) |existing| {
                if (!std.ascii.eqlIgnoreCase(existing.sha1, mod.file.hashes.sha1)) {
                    return error.ContentFileConflict;
                }
            }
            files[index] = .{
                .path = path,
                .url = mod.file.url,
                .fallback_url = null,
                .sha1 = mod.file.hashes.sha1,
                .size = mod.file.size,
                .usage = .{ .mod = true },
            };
            total_size = std.math.add(u64, total_size, mod.file.size) catch return error.DownloadSizeOverflow;
        }

        const host = try install_plan.Target.host();
        _ = try install_executor.execute(
            self.allocator,
            self.io,
            self.environ_map,
            minecraft_dir,
            store,
            .{
                .version_id = game_version,
                .source = .official,
                .target = host,
                .files = files,
                .classpath = &.{},
                .native_extractions = &.{},
                .asset_projections = &.{},
                .total_download_size = total_size,
            },
            .{ .workers = options.workers, .retries = options.retries },
            reporter,
        );

        const old_entries = registry_document.value.entries;
        const merged = try scratch.alloc(content_registry.Entry, old_entries.len + resolved.items.len);
        @memcpy(merged[0..old_entries.len], old_entries);
        const installed = try scratch.alloc(InstalledContent, resolved.items.len);
        for (resolved.items, 0..) |mod, index| {
            const file_path = files[index].path;
            merged[old_entries.len + index] = .{
                .kind = .mod,
                .file = file_path,
                .sha1 = mod.file.hashes.sha1,
                .size = mod.file.size,
                .source = .{
                    .provider = .modrinth,
                    .project_id = try scratch.dupe(u8, mod.project_id),
                    .version_id = try scratch.dupe(u8, mod.version.id),
                    .slug = if (mod.slug) |slug| try scratch.dupe(u8, slug) else null,
                },
                .enabled = true,
                .as_dependency = mod.as_dependency,
            };
            installed[index] = .{
                .file = file_path,
                .sha1 = mod.file.hashes.sha1,
                .size = mod.file.size,
                .slug = mod.slug,
                .as_dependency = mod.as_dependency,
            };
        }
        try content_registry.save(self.io, minecraft_dir, .{ .entries = merged });

        return .{
            .arena = arena,
            .installed = installed,
            .skipped = skipped,
        };
    }

    fn resolveModrinthMod(
        self: Service,
        scratch: std.mem.Allocator,
        game_version: []const u8,
        loader: modrinth.Loader,
        project: []const u8,
        version_id: ?[]const u8,
        as_dependency: bool,
        registry: content_registry.Registry,
        resolved: *std.ArrayList(ResolvedMod),
        visited: *std.StringHashMap(void),
        skipped: *usize,
        depth: u8,
    ) anyerror!void {
        if (depth > 32) return error.ModDependencyTooDeep;
        if (resolved.items.len >= 256) return error.TooManyModDependencies;

        const project_document = try self.fetchModrinthProject(scratch, project);
        const project_value = project_document.value;

        if (visited.contains(project_value.id)) return;
        try visited.put(try scratch.dupe(u8, project_value.id), {});
        if (content_registry.findByProject(registry, .modrinth, project_value.id) != null) {
            skipped.* += 1;
            return;
        }

        const version = if (version_id) |id| blk: {
            const document = try self.fetchModrinthVersion(scratch, id);
            if (!std.mem.eql(u8, document.value.project_id, project_value.id)) {
                return error.ModrinthVersionProjectMismatch;
            }
            if (!modrinth.versionSupports(document.value, game_version, loader)) {
                return error.IncompatibleModVersion;
            }
            break :blk document.value;
        } else blk: {
            const document = try self.fetchModrinthVersions(scratch, project_value.id, game_version, loader);
            break :blk modrinth.selectVersion(document.value, game_version, loader) orelse
                return error.NoCompatibleModVersion;
        };

        const file = modrinth.selectFile(version) orelse return error.ModFileSelectionFailed;
        try resolved.append(scratch, .{
            .project_id = try scratch.dupe(u8, project_value.id),
            .slug = try scratch.dupe(u8, project_value.slug),
            .version = version,
            .file = file,
            .as_dependency = as_dependency,
        });

        for (version.dependencies) |dependency| {
            if (dependency.dependency_type != .required) continue;
            if (dependency.project_id) |dependency_project| {
                try self.resolveModrinthMod(
                    scratch,
                    game_version,
                    loader,
                    dependency_project,
                    null,
                    true,
                    registry,
                    resolved,
                    visited,
                    skipped,
                    depth + 1,
                );
                continue;
            }
            if (dependency.version_id) |dependency_version| {
                const document = try self.fetchModrinthVersion(scratch, dependency_version);
                try self.resolveModrinthMod(
                    scratch,
                    game_version,
                    loader,
                    document.value.project_id,
                    dependency_version,
                    true,
                    registry,
                    resolved,
                    visited,
                    skipped,
                    depth + 1,
                );
            }
        }
    }

    fn fetchModrinthProject(self: Service, scratch: std.mem.Allocator, project: []const u8) !std.json.Parsed(modrinth.Project) {
        const url = try modrinth.projectUrlAlloc(scratch, project);
        var body = try self.fetchBody(url, modrinth.max_project_bytes);
        defer body.deinit();
        return modrinth.parseProject(scratch, body.content);
    }

    fn fetchModrinthVersion(self: Service, scratch: std.mem.Allocator, version_id: []const u8) !std.json.Parsed(modrinth.Version) {
        const url = try modrinth.versionUrlAlloc(scratch, version_id);
        var body = try self.fetchBody(url, modrinth.max_version_bytes);
        defer body.deinit();
        return modrinth.parseVersion(scratch, body.content);
    }

    fn fetchModrinthVersions(
        self: Service,
        scratch: std.mem.Allocator,
        project: []const u8,
        game_version: []const u8,
        loader: modrinth.Loader,
    ) !std.json.Parsed([]modrinth.Version) {
        const url = try modrinth.projectVersionsUrlAlloc(scratch, project, game_version, loader);
        var body = try self.fetchBody(url, modrinth.max_versions_list_bytes);
        defer body.deinit();
        return modrinth.parseVersions(scratch, body.content);
    }

    pub const ContentList = struct {
        arena: *std.heap.ArenaAllocator,
        document: content_registry.Document,
        unmanaged: []const []const u8,

        pub fn deinit(list: *ContentList) void {
            const allocator = list.arena.child_allocator;
            list.document.deinit();
            list.arena.deinit();
            allocator.destroy(list.arena);
            list.* = undefined;
        }
    };

    /// Lists registered content and unmanaged files inside the instance mods
    /// directory. Unmanaged files are candidates for `adoptInstanceContent`.
    pub fn listInstanceContent(self: Service, minecraft_dir: std.Io.Dir) !ContentList {
        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const scratch = arena.allocator();

        var document = try content_registry.load(self.allocator, self.io, minecraft_dir);
        errdefer document.deinit();

        const unmanaged = try self.scanUnmanagedMods(scratch, minecraft_dir, document.value);
        return .{
            .arena = arena,
            .document = document,
            .unmanaged = unmanaged,
        };
    }

    fn scanUnmanagedMods(
        self: Service,
        scratch: std.mem.Allocator,
        minecraft_dir: std.Io.Dir,
        registry: content_registry.Registry,
    ) ![]const []const u8 {
        var unmanaged: std.ArrayList([]const u8) = .empty;
        const mods_dir = minecraft_dir.openDir(self.io, "mods", .{ .iterate = true }) catch |err| switch (err) {
            error.FileNotFound => return unmanaged.toOwnedSlice(scratch),
            else => |other| return other,
        };
        defer mods_dir.close(self.io);
        var iterator = mods_dir.iterate();
        while (try iterator.next(self.io)) |entry| {
            if (entry.kind != .file) continue;
            if (!std.mem.endsWith(u8, entry.name, ".jar")) continue;
            const path = try std.fmt.allocPrint(scratch, "mods/{s}", .{entry.name});
            if (content_registry.findByFile(registry, path) != null) continue;
            try unmanaged.append(scratch, path);
        }
        return unmanaged.toOwnedSlice(scratch);
    }

    pub const ContentRemoval = struct {
        arena: *std.heap.ArenaAllocator,
        entry: content_registry.Entry,

        pub fn deinit(removal: *ContentRemoval) void {
            const allocator = removal.arena.child_allocator;
            removal.arena.deinit();
            allocator.destroy(removal.arena);
            removal.* = undefined;
        }
    };

    /// Removes a registered content entry by Modrinth project reference (id or
    /// slug) or by registry file path, deleting the instance hard link. The
    /// shared store object remains until garbage collection reclaims it.
    pub fn removeInstanceContent(
        self: Service,
        minecraft_dir: std.Io.Dir,
        project: ?[]const u8,
        file: ?[]const u8,
    ) !ContentRemoval {
        if ((project == null) == (file == null)) return error.ContentSelectorRequired;
        if (project) |reference| try modrinth.validateProjectRef(reference);
        if (file) |path| {
            try install_plan.validateRelativePath(path);
            if (path.len > 512) return error.InvalidContentFile;
        }

        var document = try content_registry.load(self.allocator, self.io, minecraft_dir);
        defer document.deinit();

        var match_index: ?usize = null;
        for (document.value.entries, 0..) |entry, index| {
            if (project) |reference| {
                if (entry.source.provider != .modrinth) continue;
                if (entry.source.project_id) |id| {
                    if (std.mem.eql(u8, id, reference)) {
                        match_index = index;
                        break;
                    }
                }
                if (entry.source.slug) |slug| {
                    if (std.mem.eql(u8, slug, reference)) {
                        match_index = index;
                        break;
                    }
                }
            } else if (std.mem.eql(u8, entry.file, file.?)) {
                match_index = index;
                break;
            }
        }
        const index = match_index orelse return error.ContentNotFound;
        const removed = document.value.entries[index];

        minecraft_dir.deleteFile(self.io, removed.file) catch |err| switch (err) {
            error.FileNotFound => {},
            else => |other| return other,
        };

        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const scratch = arena.allocator();

        const kept = try scratch.alloc(content_registry.Entry, document.value.entries.len - 1);
        @memcpy(kept[0..index], document.value.entries[0..index]);
        @memcpy(kept[index..], document.value.entries[index + 1 ..]);
        try content_registry.save(self.io, minecraft_dir, .{ .entries = kept });

        const owned_entry: content_registry.Entry = .{
            .kind = removed.kind,
            .file = try scratch.dupe(u8, removed.file),
            .sha1 = try scratch.dupe(u8, removed.sha1),
            .size = removed.size,
            .source = .{
                .provider = removed.source.provider,
                .project_id = if (removed.source.project_id) |id| try scratch.dupe(u8, id) else null,
                .version_id = if (removed.source.version_id) |id| try scratch.dupe(u8, id) else null,
                .slug = if (removed.source.slug) |slug| try scratch.dupe(u8, slug) else null,
            },
            .enabled = removed.enabled,
            .as_dependency = removed.as_dependency,
        };
        return .{ .arena = arena, .entry = owned_entry };
    }

    pub const ContentAdoption = struct {
        arena: *std.heap.ArenaAllocator,
        adopted: []InstalledContent,

        pub fn deinit(adoption: *ContentAdoption) void {
            const allocator = adoption.arena.child_allocator;
            adoption.arena.deinit();
            allocator.destroy(adoption.arena);
            adoption.* = undefined;
        }
    };

    /// Adopts unmanaged mod files into the content-addressed store: each file
    /// is hashed, imported into the store, re-linked as a hard link, and
    /// registered with the manual provider.
    pub fn adoptInstanceContent(
        self: Service,
        minecraft_dir: std.Io.Dir,
        store: std.Io.Dir,
    ) !ContentAdoption {
        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const scratch = arena.allocator();

        var document = try content_registry.load(self.allocator, self.io, minecraft_dir);
        defer document.deinit();

        const unmanaged = try self.scanUnmanagedMods(scratch, minecraft_dir, document.value);
        const adopted = try scratch.alloc(InstalledContent, unmanaged.len);
        var adopted_count: usize = 0;
        const merged = try scratch.alloc(content_registry.Entry, document.value.entries.len + unmanaged.len);
        @memcpy(merged[0..document.value.entries.len], document.value.entries);

        for (unmanaged) |path| {
            const file = minecraft_dir.openFile(self.io, path, .{}) catch |err| switch (err) {
                error.FileNotFound => continue,
                else => |other| return other,
            };
            defer file.close(self.io);
            const digest = try artifact_store.hashFile(file, self.io);
            const stat_size = (try file.stat(self.io)).size;
            if (stat_size == 0 or stat_size > content_registry.max_entry_size) return error.InvalidContentSize;
            const sha1 = try std.fmt.allocPrint(scratch, "{s}", .{std.fmt.bytesToHex(digest, .lower)});

            var object_path_buffer: [64]u8 = undefined;
            const object_path = try artifact_store.objectPath(sha1, &object_path_buffer);
            _ = try artifact_store.importObject(self.allocator, self.io, store, object_path, minecraft_dir, path);
            _ = try artifact_store.materialize(self.allocator, self.io, store, object_path, minecraft_dir, path, true);

            merged[document.value.entries.len + adopted_count] = .{
                .kind = .mod,
                .file = path,
                .sha1 = sha1,
                .size = stat_size,
                .source = .{ .provider = .manual },
                .enabled = true,
                .as_dependency = false,
            };
            adopted[adopted_count] = .{
                .file = path,
                .sha1 = sha1,
                .size = stat_size,
                .slug = null,
                .as_dependency = false,
            };
            adopted_count += 1;
        }
        try content_registry.save(self.io, minecraft_dir, .{
            .entries = merged[0 .. document.value.entries.len + adopted_count],
        });

        return .{
            .arena = arena,
            .adopted = adopted[0..adopted_count],
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
            if (err == error.TlsInitializationFailed) {
                body = std.Io.Writer.fixed(storage);
                const fallback_status: std.http.Status = @enumFromInt(try @import("install/https_fallback.zig").fetch(
                    self.allocator,
                    self.io,
                    url,
                    &body,
                ));
                if (body.end > max_bytes) return error.ResponseBodyTooLarge;
                if (fallback_status.class() != .success) return error.UnsuccessfulHttpStatus;
                return .{
                    .allocator = self.allocator,
                    .storage = storage,
                    .content = storage[0..body.end],
                };
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
