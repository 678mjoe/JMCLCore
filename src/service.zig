const std = @import("std");
const builtin = @import("builtin");
const artifact_store = @import("install/artifact_store.zig");
const authentication = @import("auth/authentication.zig");
const authentication_http = @import("auth/http.zig");
const account_store = @import("auth/account_store.zig");
const asset_index = @import("install/asset_index.zig");
const content_registry = @import("content/content_registry.zig");
const core = @import("root.zig");
const curseforge = @import("content/curseforge.zig");
const fabric = @import("loaders/fabric.zig");
const forge = @import("loaders/forge.zig");
const forge_legacy = @import("loaders/forge_legacy.zig");
const https_fallback = @import("install/https_fallback.zig");
const install_plan = @import("install/plan.zig");
const install_executor = @import("install/installer.zig");
const instance_store = @import("instance_store.zig");
const java_runtime = @import("install/java_runtime.zig");
const modrinth = @import("content/modrinth.zig");
const mrpack = @import("modpack/mrpack.zig");
const curseforge_pack = @import("modpack/curseforge_pack.zig");
const neoforge = @import("loaders/neoforge.zig");
const server_properties = @import("server/properties.zig");
const server_store = @import("server/server_store.zig");
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

pub const ForgeLegacyResolvedMetadata = struct {
    parent: std.json.Parsed(version_metadata.Metadata),
    profile: std.json.Parsed(forge_legacy.Profile),
    overlay: forge.Overlay,
};

pub const ResolvedMetadata = union(enum) {
    vanilla: std.json.Parsed(version_metadata.Metadata),
    fabric: FabricResolvedMetadata,
    neoforge: NeoForgeResolvedMetadata,
    forge: ForgeResolvedMetadata,
    forge_legacy: ForgeLegacyResolvedMetadata,

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
            .forge_legacy => |*document| {
                document.overlay.deinit();
                document.profile.deinit();
                document.parent.deinit();
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
            .forge_legacy => |*document| &document.overlay.value,
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
        var installer = self.fetchBody(installer_url, neoforge.max_installer_bytes) catch |err| blk: {
            if (err != error.UnsuccessfulHttpStatus) return err;
            const legacy_url = try forge.legacyInstallerUrlAlloc(self.allocator, minecraft_version, forge_version);
            defer self.allocator.free(legacy_url);
            break :blk try self.fetchBody(legacy_url, neoforge.max_installer_bytes);
        };
        defer installer.deinit();

        const install_source = (try zip_reader.readEntryAlloc(
            self.allocator,
            installer.content,
            "install_profile.json",
            neoforge.max_install_profile_bytes,
        )) orelse return error.ForgeInstallProfileMissing;
        defer self.allocator.free(install_source);

        if (try forge_legacy.isLegacyProfile(self.allocator, install_source)) {
            return self.resolveForgeLegacyMetadata(minecraft_version, parent, install_source);
        }

        const version_source = (try zip_reader.readEntryAlloc(
            self.allocator,
            installer.content,
            "version.json",
            neoforge.max_version_profile_bytes,
        )) orelse return error.ForgeVersionProfileMissing;
        defer self.allocator.free(version_source);

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

    /// Resolves a legacy Forge installer profile (Minecraft 1.7.10 through
    /// the 1.12.2-era installer format): no processor chain, the universal
    /// jar and its libraries are plain Maven artifacts. Every library gets
    /// its integrity from the profile checksums or the repository SHA-1
    /// sidecar, exactly like the Fabric library flow.
    fn resolveForgeLegacyMetadata(
        self: Service,
        minecraft_version: []const u8,
        parent: std.json.Parsed(version_metadata.Metadata),
        install_source: []const u8,
    ) !ResolvedMetadata {
        var profile = try forge_legacy.parseProfile(self.allocator, install_source);
        errdefer profile.deinit();
        if (!std.mem.eql(u8, profile.value.install.minecraft, minecraft_version)) return error.ForgeParentMismatch;
        if (profile.value.versionInfo.inheritsFrom) |parent_id| {
            if (!std.mem.eql(u8, parent_id, minecraft_version)) return error.ForgeParentMismatch;
        }

        const libraries = profile.value.versionInfo.libraries;
        const resolved = try self.allocator.alloc(forge_legacy.ResolvedLibrary, libraries.len);
        defer self.allocator.free(resolved);
        var initialized: usize = 0;
        defer for (resolved[0..initialized]) |library| {
            self.allocator.free(library.path);
            self.allocator.free(library.url);
            self.allocator.free(library.sha1);
        };
        for (libraries, 0..) |library, index| {
            resolved[index] = try self.resolveLegacyLibrary(
                library,
                profile.value.install.path,
            );
            initialized += 1;
        }

        const overlay = try forge_legacy.merge(self.allocator, parent.value, profile.value, resolved);
        return .{ .forge_legacy = .{
            .parent = parent,
            .profile = profile,
            .overlay = overlay,
        } };
    }

    fn resolveLegacyLibrary(
        self: Service,
        library: forge_legacy.Library,
        universal_name: []const u8,
    ) !forge_legacy.ResolvedLibrary {
        const base = try forge_legacy.normalizedBaseUrl(library.url);
        const is_universal = std.mem.eql(u8, library.name, universal_name);
        const coordinate = if (is_universal)
            try forge_legacy.universalCoordinateAlloc(self.allocator, library.name)
        else
            try self.allocator.dupe(u8, library.name);
        defer self.allocator.free(coordinate);

        const relative = try neoforge.artifactPathAlloc(self.allocator, coordinate);
        errdefer self.allocator.free(relative);
        const artifact_url = try forge_legacy.artifactUrlAlloc(self.allocator, base, coordinate);
        errdefer self.allocator.free(artifact_url);

        const declared: []const u8 = declared: {
            if (library.checksums) |checksums| break :declared checksums[0];
            var buffer: [40]u8 = undefined;
            self.fetchSha1Sidecar(artifact_url, &buffer) catch |err| {
                if (is_universal) return error.LegacyForgeArtifactUnavailable;
                return err;
            };
            break :declared &buffer;
        };
        const sha1 = try self.allocator.dupe(u8, declared);
        errdefer self.allocator.free(sha1);

        const size = self.fetchContentLength(artifact_url) catch |err| {
            if (is_universal) return error.LegacyForgeArtifactUnavailable;
            return err;
        };
        if (size == 0 or size > forge_legacy.max_library_size) return error.InvalidLibrarySize;
        return .{
            .path = relative,
            .url = artifact_url,
            .sha1 = sha1,
            .size = size,
        };
    }

    pub fn createInstallPlan(
        self: Service,
        version_id: []const u8,
        source: core.DownloadSource,
        target: install_plan.Target,
    ) !install_plan.Plan {
        return self.createInstallPlanForSelection(.{ .id = version_id }, source, target);
    }

    pub fn createServerInstallPlan(
        self: Service,
        version_id: []const u8,
        source: core.DownloadSource,
        target: install_plan.Target,
    ) !install_plan.Plan {
        return self.planServerInstall(.{
            .id = version_id,
            .name = version_id,
            .version_id = version_id,
            .source = source,
        }, target);
    }

    pub fn planServerInstall(
        self: Service,
        manifest: server_store.Manifest,
        target: install_plan.Target,
    ) !install_plan.Plan {
        var metadata = try self.fetchVersionMetadataPreferred(manifest.version_id, manifest.source);
        defer metadata.deinit();
        return install_plan.buildServer(self.allocator, metadata.value, manifest.source, target);
    }

    pub const ServerInstallSummary = struct {
        java_path: []const u8,
        java_major_version: u16,
        server_sha1: []const u8,
        server_size: u64,
    };

    pub fn executeServerInstall(
        self: Service,
        manifest: server_store.Manifest,
        server_dir: std.Io.Dir,
        plan: install_plan.Plan,
        store: std.Io.Dir,
        options: install_executor.Options,
        java_paths: []const []const u8,
        reporter: ?install_executor.Reporter,
    ) !ServerInstallSummary {
        if (comptime builtin.os.tag != .linux) return error.UnsupportedPlatform;

        var explicit_java: [1][]const u8 = undefined;
        const candidates: []const []const u8 = if (manifest.java_path) |path| blk: {
            explicit_java[0] = path;
            break :blk explicit_java[0..];
        } else java_paths;

        var detection = try java_runtime.detect(
            self.allocator,
            self.io,
            self.environ_map orelse return error.EnvironmentUnavailable,
            candidates,
        );
        defer detection.deinit();
        const runtime = try java_runtime.selectCompatible(
            detection,
            plan.value.java_major_version,
            plan.value.target.arch,
        );

        _ = try install_executor.execute(
            self.allocator,
            self.io,
            self.environ_map,
            server_dir,
            store,
            plan.value,
            options,
            reporter,
        );
        try server_properties.writeEula(self.io, server_dir);
        _ = try server_properties.ensureDefault(self.io, server_dir);

        const server_file = plan.value.files[0];
        try server_store.recordCompletedInstallation(
            self.io,
            server_dir,
            manifest,
            server_file.sha1,
            server_file.size,
            plan.value.java_major_version,
        );

        return .{
            .java_path = try plan.arena.allocator().dupe(u8, runtime.executable),
            .java_major_version = plan.value.java_major_version,
            .server_sha1 = server_file.sha1,
            .server_size = server_file.size,
        };
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
                var plan = try install_plan.buildWithExtras(
                    self.allocator,
                    metadata.*,
                    if (asset_document) |document_value| document_value.value else null,
                    download_source,
                    target,
                    extras.files,
                );
                plan.value.loader = .neoforge;
                return plan;
            },
            .forge => |*document| {
                var extras = try self.buildForgeExtras(selection.id, selection.forge_version.?, document);
                defer extras.deinit();
                var plan = try install_plan.buildWithExtras(
                    self.allocator,
                    metadata.*,
                    if (asset_document) |document_value| document_value.value else null,
                    download_source,
                    target,
                    extras.files,
                );
                plan.value.loader = .forge;
                return plan;
            },
            else => {
                var plan = try install_plan.build(
                    self.allocator,
                    metadata.*,
                    if (asset_document) |document| document.value else null,
                    download_source,
                    target,
                );
                plan.value.loader = switch (metadata_document) {
                    .fabric => .fabric,
                    .forge_legacy => .forge_legacy,
                    else => .none,
                };
                return plan;
            },
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

    pub const ContentInstallOptions = struct {
        workers: u8 = 8,
        retries: u8 = 2,
        with_dependencies: bool = true,
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

    const ResolvedContent = struct {
        project_id: []const u8,
        slug: ?[]const u8,
        version_id: []const u8,
        file_name: []const u8,
        url: []const u8,
        sha1: []const u8,
        size: u64,
        as_dependency: bool,
    };

    /// Resolves a Modrinth project and its required dependencies against the
    /// instance's Minecraft version and loader, downloads every file through
    /// the verified content-addressed pipeline, and registers the results in
    /// the instance content registry. Mods require the instance loader;
    /// resourcepacks resolve through the `minecraft` pseudo-loader;
    /// shaderpacks filter by game version only.
    pub fn installModrinthContent(
        self: Service,
        minecraft_dir: std.Io.Dir,
        store: std.Io.Dir,
        kind: content_registry.Kind,
        game_version: []const u8,
        loader: ?modrinth.Loader,
        project: []const u8,
        version_id: ?[]const u8,
        options: ContentInstallOptions,
        reporter: ?install_executor.Reporter,
    ) !ContentInstallSummary {
        try modrinth.validateProjectRef(project);
        try modrinth.validateGameVersion(game_version);
        if (version_id) |id| try modrinth.validateVersionId(id);
        const effective_loader: ?modrinth.Loader = switch (kind) {
            .mod => blk: {
                const value = loader orelse return error.ModLoaderRequired;
                if (value == .minecraft) return error.ModLoaderRequired;
                break :blk value;
            },
            .resourcepack => blk: {
                if (loader != null and loader.? != .minecraft) return error.InvalidContentLoader;
                break :blk .minecraft;
            },
            .shaderpack => blk: {
                if (loader != null) return error.InvalidContentLoader;
                break :blk null;
            },
        };

        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        defer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        defer arena.deinit();
        const scratch = arena.allocator();

        var registry_document = try content_registry.load(self.allocator, self.io, minecraft_dir);
        defer registry_document.deinit();

        var resolved: std.ArrayList(ResolvedContent) = .empty;
        var visited: std.StringHashMap(void) = .init(scratch);
        var skipped: usize = 0;
        try self.resolveModrinthContent(
            scratch,
            kind,
            game_version,
            effective_loader,
            project,
            version_id,
            false,
            options.with_dependencies,
            registry_document.value,
            &resolved,
            &visited,
            &skipped,
            0,
        );

        return self.installResolvedContent(
            minecraft_dir,
            store,
            kind,
            game_version,
            .modrinth,
            registry_document.value,
            resolved.items,
            skipped,
            options,
            reporter,
        );
    }

    /// Resolves a CurseForge project (numeric mod id) and its required
    /// dependencies against the instance's Minecraft version and loader,
    /// then downloads and registers like `installModrinthContent`. The API
    /// key is supplied by the caller and never persisted. Projects whose
    /// CurseForge class does not match the requested kind are rejected.
    pub fn installCurseforgeContent(
        self: Service,
        minecraft_dir: std.Io.Dir,
        store: std.Io.Dir,
        kind: content_registry.Kind,
        game_version: []const u8,
        loader: ?modrinth.Loader,
        api_key: []const u8,
        project: []const u8,
        file_id: ?[]const u8,
        options: ContentInstallOptions,
        reporter: ?install_executor.Reporter,
    ) !ContentInstallSummary {
        try curseforge.validateNumericId(project);
        try curseforge.validateApiKey(api_key);
        try modrinth.validateGameVersion(game_version);
        if (file_id) |id| try curseforge.validateNumericId(id);
        const effective_loader: ?modrinth.Loader = switch (kind) {
            .mod => blk: {
                const value = loader orelse return error.ModLoaderRequired;
                if (value == .minecraft) return error.ModLoaderRequired;
                break :blk value;
            },
            .resourcepack, .shaderpack => blk: {
                if (loader != null) return error.InvalidContentLoader;
                break :blk null;
            },
        };

        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        defer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        defer arena.deinit();
        const scratch = arena.allocator();

        var registry_document = try content_registry.load(self.allocator, self.io, minecraft_dir);
        defer registry_document.deinit();

        var resolved: std.ArrayList(ResolvedContent) = .empty;
        var visited: std.StringHashMap(void) = .init(scratch);
        var skipped: usize = 0;
        try self.resolveCurseforgeContent(
            scratch,
            kind,
            game_version,
            effective_loader,
            api_key,
            project,
            file_id,
            false,
            options.with_dependencies,
            registry_document.value,
            &resolved,
            &visited,
            &skipped,
            0,
        );

        return self.installResolvedContent(
            minecraft_dir,
            store,
            kind,
            game_version,
            .curseforge,
            registry_document.value,
            resolved.items,
            skipped,
            options,
            reporter,
        );
    }

    /// Shared install tail: conflict-checks resolved content against the
    /// registry, downloads through the verified content-addressed pipeline,
    /// and merges new registry entries atomically.
    fn installResolvedContent(
        self: Service,
        minecraft_dir: std.Io.Dir,
        store: std.Io.Dir,
        kind: content_registry.Kind,
        game_version: []const u8,
        provider: content_registry.Provider,
        registry: content_registry.Registry,
        resolved: []const ResolvedContent,
        skipped: usize,
        options: ContentInstallOptions,
        reporter: ?install_executor.Reporter,
    ) !ContentInstallSummary {
        const directory = kind.directory();

        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const scratch = arena.allocator();

        const files = try scratch.alloc(install_plan.File, resolved.len);
        var total_size: u64 = 0;
        for (resolved, 0..) |content, index| {
            const path = try std.fmt.allocPrint(scratch, "{s}/{s}", .{ directory, content.file_name });
            if (content_registry.findByFile(registry, path)) |existing| {
                if (!std.ascii.eqlIgnoreCase(existing.sha1, content.sha1)) {
                    return error.ContentFileConflict;
                }
            }
            files[index] = .{
                .path = path,
                .url = content.url,
                .fallback_url = null,
                .sha1 = content.sha1,
                .size = content.size,
                .usage = .{ .mod = kind == .mod, .resourcepack = kind == .resourcepack, .shaderpack = kind == .shaderpack },
            };
            total_size = std.math.add(u64, total_size, content.size) catch return error.DownloadSizeOverflow;
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

        const old_entries = registry.entries;
        const merged = try scratch.alloc(content_registry.Entry, old_entries.len + resolved.len);
        @memcpy(merged[0..old_entries.len], old_entries);
        const installed = try scratch.alloc(InstalledContent, resolved.len);
        for (resolved, 0..) |content, index| {
            const file_path = files[index].path;
            merged[old_entries.len + index] = .{
                .kind = kind,
                .file = file_path,
                .sha1 = content.sha1,
                .size = content.size,
                .source = .{
                    .provider = provider,
                    .project_id = try scratch.dupe(u8, content.project_id),
                    .version_id = try scratch.dupe(u8, content.version_id),
                    .slug = if (content.slug) |slug| try scratch.dupe(u8, slug) else null,
                },
                .enabled = true,
                .as_dependency = content.as_dependency,
            };
            installed[index] = .{
                .file = file_path,
                .sha1 = try scratch.dupe(u8, content.sha1),
                .size = content.size,
                .slug = if (content.slug) |slug| try scratch.dupe(u8, slug) else null,
                .as_dependency = content.as_dependency,
            };
        }
        try content_registry.save(self.io, minecraft_dir, .{ .entries = merged });

        return .{
            .arena = arena,
            .installed = installed,
            .skipped = skipped,
        };
    }

    fn resolveModrinthContent(
        self: Service,
        scratch: std.mem.Allocator,
        kind: content_registry.Kind,
        game_version: []const u8,
        loader: ?modrinth.Loader,
        project: []const u8,
        version_id: ?[]const u8,
        as_dependency: bool,
        with_dependencies: bool,
        registry: content_registry.Registry,
        resolved: *std.ArrayList(ResolvedContent),
        visited: *std.StringHashMap(void),
        skipped: *usize,
        depth: u8,
    ) anyerror!void {
        if (depth > 32) return error.ContentDependencyTooDeep;
        if (resolved.items.len >= 256) return error.TooManyContentDependencies;

        const project_document = try self.fetchModrinthProject(scratch, project);
        const project_value = project_document.value;
        const project_type = modrinth.ProjectType.parse(project_value.project_type) orelse
            return error.UnsupportedProjectType;
        const project_kind: content_registry.Kind = switch (project_type) {
            .mod => .mod,
            .resourcepack => .resourcepack,
            .shader => .shaderpack,
        };
        if (project_kind != kind) return error.ContentKindMismatch;

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
                return error.IncompatibleContentVersion;
            }
            break :blk document.value;
        } else blk: {
            const document = try self.fetchModrinthVersions(scratch, project_value.id, game_version, loader);
            break :blk modrinth.selectVersion(document.value, game_version, loader) orelse
                return error.NoCompatibleContentVersion;
        };

        const file = modrinth.selectFile(version) orelse return error.ContentFileSelectionFailed;
        try resolved.append(scratch, .{
            .project_id = try scratch.dupe(u8, project_value.id),
            .slug = try scratch.dupe(u8, project_value.slug),
            .version_id = try scratch.dupe(u8, version.id),
            .file_name = file.filename,
            .url = file.url,
            .sha1 = file.hashes.sha1,
            .size = file.size,
            .as_dependency = as_dependency,
        });

        if (!with_dependencies) return;
        for (version.dependencies) |dependency| {
            if (dependency.dependency_type != .required) continue;
            if (dependency.project_id) |dependency_project| {
                try self.resolveModrinthContent(
                    scratch,
                    kind,
                    game_version,
                    loader,
                    dependency_project,
                    null,
                    true,
                    with_dependencies,
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
                try self.resolveModrinthContent(
                    scratch,
                    kind,
                    game_version,
                    loader,
                    document.value.project_id,
                    dependency_version,
                    true,
                    with_dependencies,
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
        loader: ?modrinth.Loader,
    ) !std.json.Parsed([]modrinth.Version) {
        const url = try modrinth.projectVersionsUrlAlloc(scratch, project, game_version, loader);
        var body = try self.fetchBody(url, modrinth.max_versions_list_bytes);
        defer body.deinit();
        return modrinth.parseVersions(scratch, body.content);
    }

    fn resolveCurseforgeContent(
        self: Service,
        scratch: std.mem.Allocator,
        kind: content_registry.Kind,
        game_version: []const u8,
        loader: ?modrinth.Loader,
        api_key: []const u8,
        project: []const u8,
        file_id: ?[]const u8,
        as_dependency: bool,
        with_dependencies: bool,
        registry: content_registry.Registry,
        resolved: *std.ArrayList(ResolvedContent),
        visited: *std.StringHashMap(void),
        skipped: *usize,
        depth: u8,
    ) anyerror!void {
        if (depth > 32) return error.ContentDependencyTooDeep;
        if (resolved.items.len >= 256) return error.TooManyContentDependencies;

        const api_header: https_fallback.ExtraHeader = .{ .name = "x-api-key", .value = api_key };
        const project_document = try self.fetchCurseforgeMod(scratch, api_header, project);
        const project_value = project_document.value.data.*;
        const project_kind: content_registry.Kind = switch (project_value.classId) {
            curseforge.class_id_mods => .mod,
            curseforge.class_id_resourcepacks => .resourcepack,
            curseforge.class_id_shaderpacks => .shaderpack,
            else => return error.UnsupportedProjectType,
        };
        if (project_kind != kind) return error.ContentKindMismatch;

        const project_id = try std.fmt.allocPrint(scratch, "{d}", .{project_value.id});
        if (visited.contains(project_id)) return;
        try visited.put(project_id, {});
        if (content_registry.findByProject(registry, .curseforge, project_id) != null) {
            skipped.* += 1;
            return;
        }

        const file = if (file_id) |id| blk: {
            const document = try self.fetchCurseforgeFile(scratch, api_header, project_id, id);
            const value = document.value.data.*;
            if (value.modId != project_value.id) return error.CurseforgeFileProjectMismatch;
            if (!curseforge.fileSupports(value, game_version, loader)) {
                return error.IncompatibleContentVersion;
            }
            if (!value.isAvailable) return error.CurseforgeDownloadUnavailable;
            break :blk value;
        } else blk: {
            const files = try self.fetchCurseforgeFiles(scratch, api_header, project_id, game_version, loader);
            break :blk curseforge.selectFile(files) orelse return error.NoCompatibleContentVersion;
        };

        var download_url = file.downloadUrl;
        if (download_url == null) {
            const document = try self.fetchCurseforgeDownloadUrl(scratch, api_header, project_id, file.id);
            download_url = document.value.data;
        }
        const url = download_url orelse return error.CurseforgeDownloadUnavailable;

        try resolved.append(scratch, .{
            .project_id = project_id,
            .slug = try scratch.dupe(u8, project_value.slug),
            .version_id = try std.fmt.allocPrint(scratch, "{d}", .{file.id}),
            .file_name = file.fileName,
            .url = url,
            .sha1 = curseforge.sha1Of(file),
            .size = file.fileLength,
            .as_dependency = as_dependency,
        });

        if (!with_dependencies) return;
        for (file.dependencies) |dependency| {
            if (dependency.relationType != curseforge.relation_required) continue;
            const dependency_project = try std.fmt.allocPrint(scratch, "{d}", .{dependency.modId});
            try self.resolveCurseforgeContent(
                scratch,
                kind,
                game_version,
                loader,
                api_key,
                dependency_project,
                null,
                true,
                with_dependencies,
                registry,
                resolved,
                visited,
                skipped,
                depth + 1,
            );
        }
    }

    fn fetchCurseforgeMod(self: Service, scratch: std.mem.Allocator, api_header: https_fallback.ExtraHeader, project: []const u8) !std.json.Parsed(curseforge.Envelope(*curseforge.Mod)) {
        const url = try curseforge.projectUrlAlloc(scratch, project);
        var body = try self.fetchBodyWithHeader(url, curseforge.max_project_bytes, api_header);
        defer body.deinit();
        return curseforge.parseMod(scratch, body.content);
    }

    fn fetchCurseforgeFile(self: Service, scratch: std.mem.Allocator, api_header: https_fallback.ExtraHeader, project: []const u8, file_id: []const u8) !std.json.Parsed(curseforge.Envelope(*curseforge.File)) {
        const url = try curseforge.fileUrlAlloc(scratch, project, file_id);
        var body = try self.fetchBodyWithHeader(url, curseforge.max_file_bytes, api_header);
        defer body.deinit();
        return curseforge.parseFile(scratch, body.content);
    }

    fn fetchCurseforgeDownloadUrl(self: Service, scratch: std.mem.Allocator, api_header: https_fallback.ExtraHeader, project: []const u8, file_id: u32) !std.json.Parsed(curseforge.Envelope(?[]const u8)) {
        const file_id_string = try std.fmt.allocPrint(scratch, "{d}", .{file_id});
        const url = try curseforge.downloadUrlAlloc(scratch, project, file_id_string);
        var body = try self.fetchBodyWithHeader(url, curseforge.max_file_bytes, api_header);
        defer body.deinit();
        return curseforge.parseDownloadUrl(scratch, body.content);
    }

    fn fetchCurseforgeFiles(
        self: Service,
        scratch: std.mem.Allocator,
        api_header: https_fallback.ExtraHeader,
        project: []const u8,
        game_version: []const u8,
        loader: ?modrinth.Loader,
    ) ![]const curseforge.File {
        var files: std.ArrayList(curseforge.File) = .empty;
        var index: u32 = 0;
        while (true) {
            const url = try curseforge.filesPageUrlAlloc(scratch, project, game_version, loader, index);
            var body = try self.fetchBodyWithHeader(url, curseforge.max_files_page_bytes, api_header);
            defer body.deinit();
            const page = try curseforge.parseFilesPage(scratch, body.content);
            if (page.value.data.len == 0) break;
            try files.appendSlice(scratch, page.value.data);
            if (files.items.len > curseforge.max_files_per_project) return error.TooManyCurseforgeFiles;
            const pagination = page.value.pagination orelse break;
            if (files.items.len >= pagination.totalCount) break;
            if (page.value.data.len < curseforge.files_page_size) break;
            index += @intCast(page.value.data.len);
        }
        return files.items;
    }

    pub const ContentList = struct {
        arena: *std.heap.ArenaAllocator,
        document: content_registry.Document,
        entries: []const content_registry.Entry,
        unmanaged: []const []const u8,

        pub fn deinit(list: *ContentList) void {
            const allocator = list.arena.child_allocator;
            list.document.deinit();
            list.arena.deinit();
            allocator.destroy(list.arena);
            list.* = undefined;
        }
    };

    /// Lists registered content of the given kind and unmanaged files inside
    /// the kind's directory. Unmanaged files are candidates for
    /// `adoptInstanceContent`.
    pub fn listInstanceContent(self: Service, minecraft_dir: std.Io.Dir, kind: content_registry.Kind) !ContentList {
        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const scratch = arena.allocator();

        var document = try content_registry.load(self.allocator, self.io, minecraft_dir);
        errdefer document.deinit();

        var entries: std.ArrayList(content_registry.Entry) = .empty;
        for (document.value.entries) |entry| {
            if (entry.kind == kind) try entries.append(scratch, entry);
        }
        const unmanaged = try self.scanUnmanagedContent(scratch, minecraft_dir, document.value, kind);
        return .{
            .arena = arena,
            .document = document,
            .entries = entries.items,
            .unmanaged = unmanaged,
        };
    }

    fn scanUnmanagedContent(
        self: Service,
        scratch: std.mem.Allocator,
        minecraft_dir: std.Io.Dir,
        registry: content_registry.Registry,
        kind: content_registry.Kind,
    ) ![]const []const u8 {
        var unmanaged: std.ArrayList([]const u8) = .empty;
        const directory = kind.directory();
        const extension = kind.extension();
        const content_dir = minecraft_dir.openDir(self.io, directory, .{ .iterate = true }) catch |err| switch (err) {
            error.FileNotFound => return unmanaged.toOwnedSlice(scratch),
            else => |other| return other,
        };
        defer content_dir.close(self.io);
        var iterator = content_dir.iterate();
        while (try iterator.next(self.io)) |entry| {
            if (entry.kind != .file) continue;
            if (!std.mem.endsWith(u8, entry.name, extension)) continue;
            const path = try std.fmt.allocPrint(scratch, "{s}/{s}", .{ directory, entry.name });
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

    /// Removes a registered content entry of the given kind by Modrinth
    /// project reference (id or slug) or by registry file path, deleting the
    /// instance hard link. The shared store object remains until garbage
    /// collection reclaims it.
    pub fn removeInstanceContent(
        self: Service,
        minecraft_dir: std.Io.Dir,
        kind: content_registry.Kind,
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
            if (entry.kind != kind) continue;
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

        const disk_path = if (removed.enabled)
            removed.file
        else
            try std.fmt.allocPrint(self.allocator, "{s}{s}", .{ removed.file, disabled_suffix });
        defer if (!removed.enabled) self.allocator.free(disk_path);
        minecraft_dir.deleteFile(self.io, disk_path) catch |err| switch (err) {
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

    /// Adopts unmanaged files of the given kind into the content-addressed
    /// store: each file is hashed, imported into the store, re-linked as a
    /// hard link, and registered with the manual provider.
    pub fn adoptInstanceContent(
        self: Service,
        minecraft_dir: std.Io.Dir,
        store: std.Io.Dir,
        kind: content_registry.Kind,
    ) !ContentAdoption {
        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const scratch = arena.allocator();

        var document = try content_registry.load(self.allocator, self.io, minecraft_dir);
        defer document.deinit();

        const unmanaged = try self.scanUnmanagedContent(scratch, minecraft_dir, document.value, kind);
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
                .kind = kind,
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

    pub const ContentToggle = struct {
        arena: *std.heap.ArenaAllocator,
        changed: bool,
        entry: content_registry.Entry,

        pub fn deinit(toggle: *ContentToggle) void {
            const allocator = toggle.arena.child_allocator;
            toggle.arena.deinit();
            allocator.destroy(toggle.arena);
            toggle.* = undefined;
        }
    };

    pub const disabled_suffix = ".disabled";

    /// Enables or disables a registered content entry. Disabling renames the
    /// instance file to `<file>.disabled` so the game no longer loads it;
    /// enabling renames it back. The registry keeps the canonical path plus
    /// the enabled flag, so project lookups, removal, and version switching
    /// work regardless of state. Renames follow the on-disk reality, making
    /// repeated toggles idempotent even after a partial failure.
    pub fn setInstanceContentEnabled(
        self: Service,
        minecraft_dir: std.Io.Dir,
        kind: content_registry.Kind,
        project: ?[]const u8,
        file: ?[]const u8,
        enabled: bool,
    ) !ContentToggle {
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
            if (entry.kind != kind) continue;
            if (project) |reference| {
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
        const entry = document.value.entries[index];

        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const scratch = arena.allocator();

        const disabled_path = try std.fmt.allocPrint(scratch, "{s}{s}", .{ entry.file, disabled_suffix });
        const canonical_exists = blk: {
            minecraft_dir.access(self.io, entry.file, .{}) catch |err| switch (err) {
                error.FileNotFound => break :blk false,
                else => |other| return other,
            };
            break :blk true;
        };
        const disabled_exists = blk: {
            minecraft_dir.access(self.io, disabled_path, .{}) catch |err| switch (err) {
                error.FileNotFound => break :blk false,
                else => |other| return other,
            };
            break :blk true;
        };
        if (enabled and !canonical_exists and disabled_exists) {
            try minecraft_dir.rename(disabled_path, minecraft_dir, entry.file, self.io);
        } else if (!enabled and canonical_exists) {
            try minecraft_dir.rename(entry.file, minecraft_dir, disabled_path, self.io);
        }

        const changed = entry.enabled != enabled;
        if (changed) {
            const updated = try scratch.alloc(content_registry.Entry, document.value.entries.len);
            @memcpy(updated, document.value.entries);
            updated[index] = .{
                .kind = entry.kind,
                .file = entry.file,
                .sha1 = entry.sha1,
                .size = entry.size,
                .source = entry.source,
                .enabled = enabled,
                .as_dependency = entry.as_dependency,
            };
            try content_registry.save(self.io, minecraft_dir, .{ .entries = updated });
        }

        return .{
            .arena = arena,
            .changed = changed,
            .entry = .{
                .kind = entry.kind,
                .file = try scratch.dupe(u8, entry.file),
                .sha1 = try scratch.dupe(u8, entry.sha1),
                .size = entry.size,
                .source = .{
                    .provider = entry.source.provider,
                    .project_id = if (entry.source.project_id) |id| try scratch.dupe(u8, id) else null,
                    .version_id = if (entry.source.version_id) |id| try scratch.dupe(u8, id) else null,
                    .slug = if (entry.source.slug) |slug| try scratch.dupe(u8, slug) else null,
                },
                .enabled = enabled,
                .as_dependency = entry.as_dependency,
            },
        };
    }

    pub const ContentVersionChange = struct {
        arena: *std.heap.ArenaAllocator,
        changed: bool,
        file: []const u8,
        sha1: []const u8,
        size: u64,
        old_version_id: []const u8,
        new_version_id: []const u8,

        pub fn deinit(change: *ContentVersionChange) void {
            const allocator = change.arena.child_allocator;
            change.arena.deinit();
            allocator.destroy(change.arena);
            change.* = undefined;
        }
    };

    /// Switches a registered content entry to an exact caller-selected
    /// provider version, upgrade or downgrade. The new version must belong
    /// to the same project and support the instance's Minecraft version and
    /// loader. The file downloads through the verified pipeline before the
    /// registry is updated atomically; the old hard link is removed last.
    /// CurseForge entries require a caller-supplied API key. Dependencies
    /// are not touched.
    pub fn setInstanceContentVersion(
        self: Service,
        minecraft_dir: std.Io.Dir,
        store: std.Io.Dir,
        kind: content_registry.Kind,
        game_version: []const u8,
        loader: ?modrinth.Loader,
        project: []const u8,
        version_id: []const u8,
        api_key: ?[]const u8,
        options: ContentInstallOptions,
        reporter: ?install_executor.Reporter,
    ) !ContentVersionChange {
        try modrinth.validateProjectRef(project);
        try modrinth.validateGameVersion(game_version);
        if (api_key) |key| try curseforge.validateApiKey(key);
        const effective_loader: ?modrinth.Loader = switch (kind) {
            .mod => blk: {
                const value = loader orelse return error.ModLoaderRequired;
                if (value == .minecraft) return error.ModLoaderRequired;
                break :blk value;
            },
            .resourcepack => blk: {
                if (loader != null and loader.? != .minecraft) return error.InvalidContentLoader;
                break :blk .minecraft;
            },
            .shaderpack => blk: {
                if (loader != null) return error.InvalidContentLoader;
                break :blk null;
            },
        };

        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const scratch = arena.allocator();

        var registry_document = try content_registry.load(self.allocator, self.io, minecraft_dir);
        defer registry_document.deinit();

        var entry_index: ?usize = null;
        for (registry_document.value.entries, 0..) |entry, index| {
            if (entry.kind != kind) continue;
            if (entry.source.provider == .manual) continue;
            if (entry.source.project_id) |id| {
                if (std.mem.eql(u8, id, project)) {
                    entry_index = index;
                    break;
                }
            }
            if (entry.source.slug) |slug| {
                if (std.mem.eql(u8, slug, project)) {
                    entry_index = index;
                    break;
                }
            }
        }
        const index = entry_index orelse return error.ContentNotFound;
        const entry = registry_document.value.entries[index];
        const project_id = entry.source.project_id.?;
        const old_version_id = entry.source.version_id.?;

        if (std.mem.eql(u8, old_version_id, version_id)) {
            return .{
                .arena = arena,
                .changed = false,
                .file = try scratch.dupe(u8, entry.file),
                .sha1 = try scratch.dupe(u8, entry.sha1),
                .size = entry.size,
                .old_version_id = try scratch.dupe(u8, old_version_id),
                .new_version_id = try scratch.dupe(u8, version_id),
            };
        }

        var new_file_name: []const u8 = undefined;
        var new_url: []const u8 = undefined;
        var new_sha1: []const u8 = undefined;
        var new_size: u64 = undefined;
        switch (entry.source.provider) {
            .modrinth => {
                try modrinth.validateVersionId(version_id);
                const document = try self.fetchModrinthVersion(scratch, version_id);
                const version = document.value;
                if (!std.mem.eql(u8, version.project_id, project_id)) {
                    return error.ModrinthVersionProjectMismatch;
                }
                if (!modrinth.versionSupports(version, game_version, effective_loader)) {
                    return error.IncompatibleContentVersion;
                }
                const file = modrinth.selectFile(version) orelse return error.ContentFileSelectionFailed;
                new_file_name = file.filename;
                new_url = file.url;
                new_sha1 = file.hashes.sha1;
                new_size = file.size;
            },
            .curseforge => {
                try curseforge.validateNumericId(version_id);
                const key = api_key orelse return error.CurseforgeApiKeyRequired;
                const api_header: https_fallback.ExtraHeader = .{ .name = "x-api-key", .value = key };
                const document = try self.fetchCurseforgeFile(scratch, api_header, project_id, version_id);
                const file = document.value.data.*;
                if (!std.mem.eql(u8, try std.fmt.allocPrint(scratch, "{d}", .{file.modId}), project_id)) {
                    return error.CurseforgeFileProjectMismatch;
                }
                if (!curseforge.fileSupports(file, game_version, effective_loader)) {
                    return error.IncompatibleContentVersion;
                }
                if (!file.isAvailable) return error.CurseforgeDownloadUnavailable;
                var download_url = file.downloadUrl;
                if (download_url == null) {
                    const url_document = try self.fetchCurseforgeDownloadUrl(scratch, api_header, project_id, file.id);
                    download_url = url_document.value.data;
                }
                new_file_name = file.fileName;
                new_url = download_url orelse return error.CurseforgeDownloadUnavailable;
                new_sha1 = curseforge.sha1Of(file);
                new_size = file.fileLength;
            },
            .manual => unreachable,
        }

        const new_path = try std.fmt.allocPrint(scratch, "{s}/{s}", .{ kind.directory(), new_file_name });
        for (registry_document.value.entries, 0..) |other, other_index| {
            if (other_index == index) continue;
            if (!std.mem.eql(u8, other.file, new_path)) continue;
            if (!std.ascii.eqlIgnoreCase(other.sha1, new_sha1)) return error.ContentFileConflict;
        }

        const files = try scratch.alloc(install_plan.File, 1);
        files[0] = .{
            .path = new_path,
            .url = new_url,
            .fallback_url = null,
            .sha1 = new_sha1,
            .size = new_size,
            .usage = usageForContentKind(kind),
        };
        try self.executeModpackDownloads(minecraft_dir, store, game_version, files, options, reporter);

        const old_disk_path = if (entry.enabled)
            entry.file
        else
            try std.fmt.allocPrint(scratch, "{s}{s}", .{ entry.file, disabled_suffix });
        if (!entry.enabled) {
            const disabled_new_path = try std.fmt.allocPrint(scratch, "{s}{s}", .{ new_path, disabled_suffix });
            if (std.mem.eql(u8, new_path, entry.file)) {
                minecraft_dir.deleteFile(self.io, disabled_new_path) catch |err| switch (err) {
                    error.FileNotFound => {},
                    else => |other| return other,
                };
            }
            try minecraft_dir.rename(new_path, minecraft_dir, disabled_new_path, self.io);
        }

        const updated = try scratch.alloc(content_registry.Entry, registry_document.value.entries.len);
        @memcpy(updated, registry_document.value.entries);
        updated[index] = .{
            .kind = kind,
            .file = new_path,
            .sha1 = new_sha1,
            .size = new_size,
            .source = .{
                .provider = entry.source.provider,
                .project_id = try scratch.dupe(u8, project_id),
                .version_id = try scratch.dupe(u8, version_id),
                .slug = if (entry.source.slug) |slug| try scratch.dupe(u8, slug) else null,
            },
            .enabled = entry.enabled,
            .as_dependency = entry.as_dependency,
        };
        try content_registry.save(self.io, minecraft_dir, .{ .entries = updated });

        if (!std.mem.eql(u8, entry.file, new_path)) {
            minecraft_dir.deleteFile(self.io, old_disk_path) catch |err| switch (err) {
                error.FileNotFound => {},
                else => |other| return other,
            };
        }

        return .{
            .arena = arena,
            .changed = true,
            .file = new_path,
            .sha1 = try scratch.dupe(u8, new_sha1),
            .size = new_size,
            .old_version_id = try scratch.dupe(u8, old_version_id),
            .new_version_id = try scratch.dupe(u8, version_id),
        };
    }

    pub const ModpackInstallSummary = struct {
        arena: *std.heap.ArenaAllocator,
        id: []const u8,
        name: []const u8,
        version_id: []const u8,
        loader_name: ?[]const u8,
        loader_version: ?[]const u8,
        files_installed: usize,
        files_skipped: usize,
        overrides_written: usize,

        pub fn deinit(summary: *ModpackInstallSummary) void {
            const allocator = summary.arena.child_allocator;
            summary.arena.deinit();
            allocator.destroy(summary.arena);
            summary.* = undefined;
        }
    };

    pub const max_modpack_bytes: usize = 512 * 1024 * 1024;
    const max_override_file_bytes: usize = 64 * 1024 * 1024;
    const max_override_total_bytes: u64 = 512 * 1024 * 1024;
    const max_override_count: usize = 8192;

    /// Creates a new instance from a modpack archive: parses the embedded
    /// index (Modrinth `.mrpack` or CurseForge `manifest.json` format),
    /// creates the instance with the pack's Minecraft version and mod loader,
    /// downloads every client-side content file through the verified
    /// content-addressed pipeline, registers them in the content registry,
    /// and extracts the pack's overrides. CurseForge packs require a
    /// caller-supplied API key. Game files are not installed; run the normal
    /// instance install afterwards.
    pub fn installModpack(
        self: Service,
        instances_root: std.Io.Dir,
        store: std.Io.Dir,
        id: []const u8,
        pack_path: []const u8,
        api_key: ?[]const u8,
        options: ContentInstallOptions,
        reporter: ?install_executor.Reporter,
    ) !ModpackInstallSummary {
        try instance_store.validateId(id);
        if (api_key) |key| try curseforge.validateApiKey(key);

        const arena = try self.allocator.create(std.heap.ArenaAllocator);
        errdefer self.allocator.destroy(arena);
        arena.* = std.heap.ArenaAllocator.init(self.allocator);
        errdefer arena.deinit();
        const scratch = arena.allocator();

        const pack_bytes = std.Io.Dir.cwd().readFileAlloc(self.io, pack_path, scratch, .limited(max_modpack_bytes + 1)) catch |err| switch (err) {
            error.StreamTooLong => return error.ModpackTooLarge,
            else => |other| return other,
        };
        if (pack_bytes.len > max_modpack_bytes) return error.ModpackTooLarge;

        if (instances_root.openDir(self.io, id, .{})) |existing| {
            var dir = existing;
            dir.close(self.io);
            return error.InstanceAlreadyExists;
        } else |err| switch (err) {
            error.FileNotFound => {},
            else => |other| return other,
        }

        if (try zip_reader.findEntry(pack_bytes, mrpack.index_name)) |_| {
            return self.installMrpack(arena, instances_root, store, id, pack_bytes, options, reporter);
        }
        if (try zip_reader.findEntry(pack_bytes, curseforge_pack.manifest_name)) |_| {
            const key = api_key orelse return error.CurseforgeApiKeyRequired;
            return self.installCurseforgePack(arena, instances_root, store, id, pack_bytes, key, options, reporter);
        }
        return error.UnknownModpackFormat;
    }

    fn installMrpack(
        self: Service,
        arena: *std.heap.ArenaAllocator,
        instances_root: std.Io.Dir,
        store: std.Io.Dir,
        id: []const u8,
        pack_bytes: []const u8,
        options: ContentInstallOptions,
        reporter: ?install_executor.Reporter,
    ) !ModpackInstallSummary {
        const scratch = arena.allocator();
        const index_bytes = (try zip_reader.readEntryAlloc(scratch, pack_bytes, mrpack.index_name, mrpack.max_index_bytes)).?;
        const index_document = try mrpack.parse(scratch, index_bytes);
        const index = index_document.value;

        const name = if (index.versionId) |version_id|
            try std.fmt.allocPrint(scratch, "{s} {s}", .{ index.name, version_id })
        else
            index.name;
        var manifest: instance_store.Manifest = .{
            .id = id,
            .name = name,
            .version_id = index.dependencies.minecraft,
        };
        var loader_name: ?[]const u8 = null;
        var loader_version: ?[]const u8 = null;
        if (try mrpack.loaderSelection(index.dependencies)) |selection| {
            loader_name = @tagName(selection.loader);
            loader_version = selection.version;
            switch (selection.loader) {
                .fabric => manifest.fabric_loader = selection.version,
                .forge => manifest.forge_version = selection.version,
                .neoforge => manifest.neoforge_version = selection.version,
            }
        }

        const minecraft = try self.createModpackInstance(instances_root, id, manifest);
        defer minecraft.close(self.io);
        errdefer instances_root.deleteTree(self.io, id) catch {};

        var files: std.ArrayList(install_plan.File) = .empty;
        var entries: std.ArrayList(content_registry.Entry) = .empty;
        var skipped: usize = 0;
        for (index.files) |file| {
            if (!mrpack.shipsToClient(file)) {
                skipped += 1;
                continue;
            }
            const kind = kindForContentPath(file.path);
            try files.append(scratch, .{
                .path = file.path,
                .url = file.downloads[0],
                .fallback_url = if (file.downloads.len > 1) file.downloads[1] else null,
                .sha1 = file.hashes.sha1,
                .size = file.fileSize,
                .usage = usageForContentKind(kind),
            });
            if (kind) |content_kind| {
                const source: content_registry.Source = if (mrpack.modrinthIdsFromUrl(file.downloads[0])) |ids|
                    .{ .provider = .modrinth, .project_id = ids.project_id, .version_id = ids.version_id }
                else
                    .{ .provider = .manual };
                try entries.append(scratch, .{
                    .kind = content_kind,
                    .file = file.path,
                    .sha1 = file.hashes.sha1,
                    .size = file.fileSize,
                    .source = source,
                    .enabled = true,
                    .as_dependency = false,
                });
            }
        }

        try self.executeModpackDownloads(minecraft, store, index.dependencies.minecraft, files.items, options, reporter);
        try content_registry.save(self.io, minecraft, .{ .entries = entries.items });

        var overrides = try self.extractOverrides(scratch, pack_bytes, minecraft, mrpack.overrides_directory);
        overrides += try self.extractOverrides(scratch, pack_bytes, minecraft, mrpack.client_overrides_directory);

        return .{
            .arena = arena,
            .id = id,
            .name = name,
            .version_id = index.dependencies.minecraft,
            .loader_name = loader_name,
            .loader_version = loader_version,
            .files_installed = files.items.len,
            .files_skipped = skipped,
            .overrides_written = overrides,
        };
    }

    fn installCurseforgePack(
        self: Service,
        arena: *std.heap.ArenaAllocator,
        instances_root: std.Io.Dir,
        store: std.Io.Dir,
        id: []const u8,
        pack_bytes: []const u8,
        api_key: []const u8,
        options: ContentInstallOptions,
        reporter: ?install_executor.Reporter,
    ) !ModpackInstallSummary {
        const scratch = arena.allocator();
        const manifest_bytes = (try zip_reader.readEntryAlloc(scratch, pack_bytes, curseforge_pack.manifest_name, curseforge_pack.max_manifest_bytes)).?;
        const manifest_document = try curseforge_pack.parse(scratch, manifest_bytes);
        const pack = manifest_document.value;

        const name = if (pack.version) |version|
            try std.fmt.allocPrint(scratch, "{s} {s}", .{ pack.name, version })
        else
            pack.name;
        var manifest: instance_store.Manifest = .{
            .id = id,
            .name = name,
            .version_id = pack.minecraft.version,
        };
        var loader_name: ?[]const u8 = null;
        var loader_version: ?[]const u8 = null;
        if (try curseforge_pack.loaderSelection(pack.minecraft)) |selection| {
            loader_name = @tagName(selection.loader);
            loader_version = selection.version;
            switch (selection.loader) {
                .fabric => manifest.fabric_loader = selection.version,
                .forge => manifest.forge_version = selection.version,
                .neoforge => manifest.neoforge_version = selection.version,
            }
        }

        const minecraft = try self.createModpackInstance(instances_root, id, manifest);
        defer minecraft.close(self.io);
        errdefer instances_root.deleteTree(self.io, id) catch {};

        const api_header: https_fallback.ExtraHeader = .{ .name = "x-api-key", .value = api_key };
        var project_kinds = std.AutoHashMap(u32, content_registry.Kind).init(scratch);
        var project_slugs = std.AutoHashMap(u32, []const u8).init(scratch);
        var seen_files: std.StringHashMap(void) = .init(scratch);
        var files: std.ArrayList(install_plan.File) = .empty;
        var entries: std.ArrayList(content_registry.Entry) = .empty;
        var skipped: usize = 0;
        for (pack.files) |reference| {
            if (!reference.required) {
                skipped += 1;
                continue;
            }
            const project_id = try std.fmt.allocPrint(scratch, "{d}", .{reference.projectID});
            const kind = project_kinds.get(reference.projectID) orelse blk: {
                const mod_document = try self.fetchCurseforgeMod(scratch, api_header, project_id);
                const mod = mod_document.value.data.*;
                const resolved: content_registry.Kind = switch (mod.classId) {
                    curseforge.class_id_mods => .mod,
                    curseforge.class_id_resourcepacks => .resourcepack,
                    curseforge.class_id_shaderpacks => .shaderpack,
                    else => return error.UnsupportedProjectType,
                };
                try project_kinds.put(reference.projectID, resolved);
                try project_slugs.put(reference.projectID, mod.slug);
                break :blk resolved;
            };
            const file_id = try std.fmt.allocPrint(scratch, "{d}", .{reference.fileID});
            const file_document = try self.fetchCurseforgeFile(scratch, api_header, project_id, file_id);
            const file = file_document.value.data.*;
            if (file.modId != reference.projectID) return error.CurseforgeFileProjectMismatch;

            var download_url = file.downloadUrl;
            if (download_url == null) {
                const url_document = try self.fetchCurseforgeDownloadUrl(scratch, api_header, project_id, file.id);
                download_url = url_document.value.data;
            }
            const url = download_url orelse return error.CurseforgeDownloadUnavailable;

            const path = try std.fmt.allocPrint(scratch, "{s}/{s}", .{ kind.directory(), file.fileName });
            if (seen_files.contains(path)) return error.DuplicateModpackFile;
            try seen_files.put(path, {});
            try files.append(scratch, .{
                .path = path,
                .url = url,
                .fallback_url = null,
                .sha1 = curseforge.sha1Of(file),
                .size = file.fileLength,
                .usage = usageForContentKind(kind),
            });
            try entries.append(scratch, .{
                .kind = kind,
                .file = path,
                .sha1 = curseforge.sha1Of(file),
                .size = file.fileLength,
                .source = .{
                    .provider = .curseforge,
                    .project_id = project_id,
                    .version_id = file_id,
                    .slug = project_slugs.get(reference.projectID),
                },
                .enabled = true,
                .as_dependency = false,
            });
        }

        try self.executeModpackDownloads(minecraft, store, pack.minecraft.version, files.items, options, reporter);
        try content_registry.save(self.io, minecraft, .{ .entries = entries.items });

        const overrides = try self.extractOverrides(scratch, pack_bytes, minecraft, pack.overrides orelse curseforge_pack.default_overrides_directory);

        return .{
            .arena = arena,
            .id = id,
            .name = name,
            .version_id = pack.minecraft.version,
            .loader_name = loader_name,
            .loader_version = loader_version,
            .files_installed = files.items.len,
            .files_skipped = skipped,
            .overrides_written = overrides,
        };
    }

    fn createModpackInstance(
        self: Service,
        instances_root: std.Io.Dir,
        id: []const u8,
        manifest: instance_store.Manifest,
    ) !std.Io.Dir {
        try instance_store.create(self.io, instances_root, manifest);
        const minecraft_path = try std.fmt.allocPrint(self.allocator, "{s}/.minecraft", .{id});
        defer self.allocator.free(minecraft_path);
        return instances_root.openDir(self.io, minecraft_path, .{});
    }

    fn executeModpackDownloads(
        self: Service,
        minecraft: std.Io.Dir,
        store: std.Io.Dir,
        game_version: []const u8,
        files: []const install_plan.File,
        options: ContentInstallOptions,
        reporter: ?install_executor.Reporter,
    ) !void {
        var total_size: u64 = 0;
        for (files) |file| {
            total_size = std.math.add(u64, total_size, file.size) catch return error.DownloadSizeOverflow;
        }
        const host = try install_plan.Target.host();
        _ = try install_executor.execute(
            self.allocator,
            self.io,
            self.environ_map,
            minecraft,
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
    }

    /// Extracts override files from the pack archive into the instance
    /// directory. Entry names are validated as safe relative paths; writes
    /// are atomic with parent creation.
    fn extractOverrides(
        self: Service,
        scratch: std.mem.Allocator,
        pack_bytes: []const u8,
        minecraft: std.Io.Dir,
        directory: []const u8,
    ) !usize {
        const prefix = try std.fmt.allocPrint(scratch, "{s}/", .{directory});
        var written: usize = 0;
        var total_bytes: u64 = 0;
        var iterator = try zip_reader.entries(pack_bytes);
        while (try iterator.next()) |entry| {
            if (!std.mem.startsWith(u8, entry.name, prefix)) continue;
            const relative = entry.name[prefix.len..];
            if (relative.len == 0) continue;
            try install_plan.validateRelativePath(relative);
            if (relative.len > 512) return error.InvalidModpackOverride;
            if (written >= max_override_count) return error.TooManyModpackOverrides;
            const content = try zip_reader.readAlloc(scratch, pack_bytes, entry, max_override_file_bytes);
            total_bytes = std.math.add(u64, total_bytes, content.len) catch return error.ModpackOverridesTooLarge;
            if (total_bytes > max_override_total_bytes) return error.ModpackOverridesTooLarge;
            var atomic = try minecraft.createFileAtomic(self.io, relative, .{ .make_path = true, .replace = true });
            defer atomic.deinit(self.io);
            try std.Io.File.writeStreamingAll(atomic.file, self.io, content);
            try atomic.replace(self.io);
            written += 1;
        }
        return written;
    }

    fn kindForContentPath(path: []const u8) ?content_registry.Kind {
        inline for (.{ content_registry.Kind.mod, content_registry.Kind.resourcepack, content_registry.Kind.shaderpack }) |kind| {
            const prefix = comptime (kind.directory() ++ "/");
            if (std.mem.startsWith(u8, path, prefix) and path.len > prefix.len) return kind;
        }
        return null;
    }

    fn usageForContentKind(kind: ?content_registry.Kind) install_plan.Usage {
        const value = kind orelse return .{};
        return .{
            .mod = value == .mod,
            .resourcepack = value == .resourcepack,
            .shaderpack = value == .shaderpack,
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
                try self.fetchSha1Sidecar(artifact_url, sha1);
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

    fn fetchSha1Sidecar(self: Service, artifact_url: []const u8, output: []u8) !void {
        if (output.len != 40) return error.InvalidSha1Destination;
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
        return self.fetchContentLengthPrimary(url) catch |err| {
            if (err != error.TlsInitializationFailed) return err;
            const head = try https_fallback.fetchHead(self.allocator, self.io, url);
            const status: std.http.Status = @enumFromInt(head.status);
            if (status.class() != .success) return error.UnsuccessfulHttpStatus;
            return head.content_length orelse error.MissingContentLength;
        };
    }

    fn fetchContentLengthPrimary(self: Service, url: []const u8) !u64 {
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
        return self.fetchBodyWithHeader(url, max_bytes, null);
    }

    fn fetchBodyWithHeader(self: Service, url: []const u8, max_bytes: usize, extra_header: ?https_fallback.ExtraHeader) !ResponseBody {
        if (extra_header) |header| try header.validate();
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

        const extra_headers: []const std.http.Header = if (extra_header) |header|
            &.{.{ .name = header.name, .value = header.value }}
        else
            &.{};
        const result = client.fetch(.{
            .location = .{ .url = url },
            .response_writer = &body,
            .redirect_behavior = std.http.Client.Request.RedirectBehavior.init(3),
            .headers = .{
                .user_agent = .{ .override = core.name ++ "/" ++ core.version },
            },
            .extra_headers = extra_headers,
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
                    extra_header,
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

test "executeServerInstall rejects unsupported platforms before touching the host" {
    if (builtin.os.tag == .linux) return;
    const io = std.testing.io;
    var temporary = std.testing.tmpDir(.{});
    defer temporary.cleanup();

    const arena = try std.testing.allocator.create(std.heap.ArenaAllocator);
    defer std.testing.allocator.destroy(arena);
    arena.* = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();

    const plan: install_plan.Plan = .{
        .arena = arena,
        .value = .{
            .version_id = "1.21.4",
            .source = .official,
            .target = .{ .os = .linux, .arch = .x86_64 },
            .files = &.{},
            .classpath = &.{},
            .native_extractions = &.{},
            .asset_projections = &.{},
            .total_download_size = 0,
            .java_major_version = 21,
        },
    };

    const service = Service.init(std.testing.allocator, io, null);
    try std.testing.expectError(error.UnsupportedPlatform, service.executeServerInstall(
        .{ .id = "gate", .name = "Gate", .version_id = "1.21.4" },
        temporary.dir,
        plan,
        temporary.dir,
        .{ .workers = 1 },
        &.{},
        null,
    ));
}
