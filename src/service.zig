const std = @import("std");
const authentication = @import("authentication.zig");
const authentication_http = @import("authentication_http.zig");
const account_store = @import("account_store.zig");
const asset_index = @import("asset_index.zig");
const core = @import("root.zig");
const install_plan = @import("install_plan.zig");
const version_manifest = @import("version_manifest.zig");
const version_metadata = @import("version_metadata.zig");

const max_manifest_bytes = 2 * 1024 * 1024;
const max_metadata_bytes = 4 * 1024 * 1024;
const max_asset_index_bytes = 16 * 1024 * 1024;

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

    pub fn createInstallPlan(
        self: Service,
        version_id: []const u8,
        source: core.DownloadSource,
        target: install_plan.Target,
    ) !install_plan.Plan {
        return self.createInstallPlanFromSource(version_id, source, source, target) catch |primary_error| {
            if (source != .bmclapi) return primary_error;
            return self.createInstallPlanFromSource(version_id, .official, source, target);
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
        version_id: []const u8,
        metadata_source: core.DownloadSource,
        download_source: core.DownloadSource,
        target: install_plan.Target,
    ) !install_plan.Plan {
        var metadata_document = try self.fetchVersionMetadata(version_id, metadata_source);
        defer metadata_document.deinit();

        const asset_document: ?std.json.Parsed(asset_index.Index) = if (metadata_document.value.assetIndex) |descriptor|
            try self.fetchAssetIndex(descriptor, metadata_source)
        else
            null;
        defer if (asset_document) |document| document.deinit();

        return install_plan.build(
            self.allocator,
            metadata_document.value,
            if (asset_document) |document| document.value else null,
            download_source,
            target,
        );
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
