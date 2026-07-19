const std = @import("std");
const launch_plan = @import("../launch/plan.zig");

pub const microsoft_scope = "XboxLive.signin offline_access";
pub const microsoft_tenant = "consumers";
pub const max_oauth_response_bytes: usize = 64 * 1024;
pub const max_oauth_value_bytes: usize = 64 * 1024;
pub const oauth_request_timeout_ms: u32 = 30_000;

pub const TransportError = error{
    OutOfMemory,
    Canceled,
    NetworkError,
    Timeout,
    ResponseTooLarge,
    InvalidResponse,
};

pub const HttpResponse = struct {
    status: std.http.Status,
    storage: []u8,
    body_len: usize,

    pub fn body(response: HttpResponse) ![]const u8 {
        if (response.body_len > response.storage.len) return error.InvalidResponse;
        return response.storage[0..response.body_len];
    }

    pub fn deinit(response: *HttpResponse, allocator: std.mem.Allocator) void {
        std.crypto.secureZero(u8, response.storage);
        allocator.free(response.storage);
        response.* = undefined;
    }
};

pub const FormPost = struct {
    url: []const u8,
    body: []const u8,
    max_response_bytes: usize = max_oauth_response_bytes,
    timeout_ms: u32 = oauth_request_timeout_ms,
};

pub const JsonPost = struct {
    url: []const u8,
    body: []const u8,
    max_response_bytes: usize = max_oauth_response_bytes,
    timeout_ms: u32 = oauth_request_timeout_ms,
};

pub const BearerGet = struct {
    url: []const u8,
    bearer_token: []const u8,
    max_response_bytes: usize = max_oauth_response_bytes,
    timeout_ms: u32 = oauth_request_timeout_ms,
};

pub const HttpRequest = union(enum) {
    form_post: FormPost,
    json_post: JsonPost,
    bearer_get: BearerGet,
};

pub const Transport = struct {
    context: *anyopaque,
    request_fn: *const fn (*anyopaque, std.mem.Allocator, HttpRequest) TransportError!HttpResponse,

    pub fn request(
        transport: Transport,
        allocator: std.mem.Allocator,
        value: HttpRequest,
    ) TransportError!HttpResponse {
        return transport.request_fn(transport.context, allocator, value);
    }

    pub fn postForm(
        transport: Transport,
        allocator: std.mem.Allocator,
        value: FormPost,
    ) TransportError!HttpResponse {
        return transport.request(allocator, .{ .form_post = value });
    }

    pub fn postJson(
        transport: Transport,
        allocator: std.mem.Allocator,
        value: JsonPost,
    ) TransportError!HttpResponse {
        return transport.request(allocator, .{ .json_post = value });
    }

    pub fn getBearer(
        transport: Transport,
        allocator: std.mem.Allocator,
        value: BearerGet,
    ) TransportError!HttpResponse {
        return transport.request(allocator, .{ .bearer_get = value });
    }
};

pub const Endpoints = struct {
    device_authorization: []const u8,
    oauth_token: []const u8,
    xbox_user_authentication: []const u8,
    xbox_security_token: []const u8,
    minecraft_login: []const u8,
    minecraft_entitlements: []const u8,
    minecraft_profile: []const u8,
};

pub const official_endpoints: Endpoints = .{
    .device_authorization = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode",
    .oauth_token = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token",
    .xbox_user_authentication = "https://user.auth.xboxlive.com/user/authenticate",
    .xbox_security_token = "https://xsts.auth.xboxlive.com/xsts/authorize",
    .minecraft_login = "https://api.minecraftservices.com/authentication/login_with_xbox",
    .minecraft_entitlements = "https://api.minecraftservices.com/entitlements/mcstore",
    .minecraft_profile = "https://api.minecraftservices.com/minecraft/profile",
};

pub const Config = struct {
    client_id: []const u8,
    locale: ?[]const u8 = null,

    pub fn validate(config: Config) !void {
        try validateClientId(config.client_id);
        if (config.locale) |locale| try validateLocale(locale);
    }
};

pub const DeviceAuthorization = struct {
    device_code: []const u8,
    user_code: []const u8,
    verification_uri: []const u8,
    verification_uri_complete: ?[]const u8 = null,
    expires_in: u32,
    interval: u32,
    message: ?[]const u8 = null,
};

pub const OAuthCredential = struct {
    client_id: []const u8,
    token_type: []const u8,
    scope: ?[]const u8 = null,
    expires_in: u32,
    access_token: []const u8,
    refresh_token: []const u8,

    pub fn access(credential: OAuthCredential) MicrosoftAccess {
        return .{
            .client_id = credential.client_id,
            .access_token = credential.access_token,
        };
    }
};

pub const MicrosoftAccess = struct {
    client_id: []const u8,
    access_token: []const u8,
};

pub const DevicePollState = enum {
    authorization_pending,
    slow_down,
};

pub const DevicePollResult = union(enum) {
    pending: DevicePollState,
    authenticated: OAuthCredential,
};

pub const Profile = struct {
    id: []const u8,
    name: []const u8,
};

pub const Session = struct {
    profile: Profile,
    access_token: []const u8,
    expires_in: u32,
    xuid: ?[]const u8 = null,
    client_id: []const u8,

    pub fn launchAuth(session: Session) launch_plan.Auth {
        return .{
            .player_name = session.profile.name,
            .uuid = session.profile.id,
            .access_token = session.access_token,
            .user_type = "msa",
            .xuid = session.xuid orelse "",
            .client_id = session.client_id,
        };
    }
};

pub const ExchangeFailure = enum {
    canceled,
    network_error,
    timeout,
    invalid_response,
    rate_limited,
    xbox_account_banned,
    xbox_account_required,
    xbox_region_unsupported,
    xbox_adult_verification_required,
    xbox_child_account_restricted,
    xbox_auth_failed,
    minecraft_access_denied,
    minecraft_service_error,
    game_not_owned,
    profile_not_found,
    unexpected,
};

pub const RefreshExchange = union(enum) {
    session: struct {
        refresh_token: []const u8,
        session: Session,
    },
    exchange_failed: struct {
        refresh_token: []const u8,
        failure: ExchangeFailure,
    },
};

const WipingAllocator = struct {
    child: std.mem.Allocator,

    fn allocator(wiping: *WipingAllocator) std.mem.Allocator {
        return .{
            .ptr = wiping,
            .vtable = &.{
                .alloc = allocate,
                .resize = resize,
                .remap = remap,
                .free = free,
            },
        };
    }

    fn allocate(
        raw_context: *anyopaque,
        len: usize,
        alignment: std.mem.Alignment,
        return_address: usize,
    ) ?[*]u8 {
        const wiping: *WipingAllocator = @ptrCast(@alignCast(raw_context));
        return wiping.child.rawAlloc(len, alignment, return_address);
    }

    fn resize(
        raw_context: *anyopaque,
        memory: []u8,
        alignment: std.mem.Alignment,
        new_len: usize,
        return_address: usize,
    ) bool {
        _ = raw_context;
        _ = memory;
        _ = alignment;
        _ = new_len;
        _ = return_address;
        return false;
    }

    fn remap(
        raw_context: *anyopaque,
        memory: []u8,
        alignment: std.mem.Alignment,
        new_len: usize,
        return_address: usize,
    ) ?[*]u8 {
        _ = raw_context;
        _ = memory;
        _ = alignment;
        _ = new_len;
        _ = return_address;
        return null;
    }

    fn free(
        raw_context: *anyopaque,
        memory: []u8,
        alignment: std.mem.Alignment,
        return_address: usize,
    ) void {
        const wiping: *WipingAllocator = @ptrCast(@alignCast(raw_context));
        std.crypto.secureZero(u8, memory);
        wiping.child.rawFree(memory, alignment, return_address);
    }
};

const SecureArena = struct {
    backing: std.mem.Allocator,
    wiping: WipingAllocator,
    arena: std.heap.ArenaAllocator,

    fn create(backing: std.mem.Allocator) !*SecureArena {
        const secure = try backing.create(SecureArena);
        secure.backing = backing;
        secure.wiping = .{ .child = backing };
        secure.arena = std.heap.ArenaAllocator.init(secure.wiping.allocator());
        return secure;
    }

    fn allocator(secure: *SecureArena) std.mem.Allocator {
        return secure.arena.allocator();
    }

    fn deinit(secure: *SecureArena) void {
        const backing = secure.backing;
        secure.arena.deinit();
        secure.* = undefined;
        backing.destroy(secure);
    }
};

pub fn Owned(comptime T: type) type {
    return struct {
        arena: *SecureArena,
        value: T,

        pub fn deinit(result: *@This()) void {
            wipeOwnedValue(T, &result.value);
            result.arena.deinit();
            result.* = undefined;
        }
    };
}

pub const DeviceAuthorizationResult = Owned(DeviceAuthorization);
pub const OAuthCredentialResult = Owned(OAuthCredential);
pub const DevicePollResultOwned = Owned(DevicePollResult);
pub const SessionResult = Owned(Session);
pub const RefreshExchangeResult = Owned(RefreshExchange);

pub const MicrosoftOAuth = struct {
    transport: Transport,
    endpoints: Endpoints = official_endpoints,

    pub fn deviceBegin(
        oauth: MicrosoftOAuth,
        allocator: std.mem.Allocator,
        config: Config,
    ) !DeviceAuthorizationResult {
        try config.validate();
        try validateEndpoint(oauth.endpoints.device_authorization);
        const form = try encodeForm(allocator, &.{
            .{ .name = "client_id", .value = config.client_id },
            .{ .name = "scope", .value = microsoft_scope },
        });
        defer freeSecret(allocator, form);
        const url = try endpointWithLocale(allocator, oauth.endpoints.device_authorization, config.locale);
        defer allocator.free(url);

        var response = try oauth.transport.postForm(allocator, .{ .url = url, .body = form });
        defer response.deinit(allocator);
        if (response.status != .ok) return classifyGeneralFailure(allocator, response);

        const arena = try createArena(allocator);
        errdefer destroyArena(arena);
        const wire = parseWire(DeviceAuthorizationWire, arena.allocator(), try response.body()) catch |err| return err;
        errdefer wipeDeviceAuthorizationWire(wire);
        try validateDeviceAuthorization(wire);
        return .{
            .arena = arena,
            .value = .{
                .device_code = wire.device_code,
                .user_code = wire.user_code,
                .verification_uri = wire.verification_uri,
                .verification_uri_complete = wire.verification_uri_complete,
                .expires_in = wire.expires_in,
                .interval = wire.interval orelse 5,
                .message = wire.message,
            },
        };
    }

    pub fn devicePoll(
        oauth: MicrosoftOAuth,
        allocator: std.mem.Allocator,
        client_id: []const u8,
        device_code: []const u8,
    ) !DevicePollResultOwned {
        try validateClientId(client_id);
        try validateOpaqueInput(device_code, error.InvalidDeviceCode);
        try validateEndpoint(oauth.endpoints.oauth_token);
        const form = try encodeForm(allocator, &.{
            .{ .name = "grant_type", .value = "urn:ietf:params:oauth:grant-type:device_code" },
            .{ .name = "client_id", .value = client_id },
            .{ .name = "device_code", .value = device_code },
        });
        defer freeSecret(allocator, form);

        var response = try oauth.transport.postForm(allocator, .{ .url = oauth.endpoints.oauth_token, .body = form });
        defer response.deinit(allocator);
        if (response.status == .ok) {
            const arena = try createArena(allocator);
            errdefer destroyArena(arena);
            const wire = parseWire(TokenWire, arena.allocator(), try response.body()) catch |err| return err;
            errdefer wipeTokenWire(wire);
            const credential = try credentialFromWire(arena.allocator(), client_id, wire, null);
            return .{ .arena = arena, .value = .{ .authenticated = credential } };
        }
        if (response.status == .too_many_requests) return error.RateLimited;
        if (response.status == .bad_request) {
            const state = try classifyPollFailure(allocator, try response.body());
            const arena = try createArena(allocator);
            return .{ .arena = arena, .value = .{ .pending = state } };
        }
        if (response.status == .unauthorized) return classifyGeneralFailure(allocator, response);
        return classifyServiceFailure(response.status);
    }

    pub fn refresh(
        oauth: MicrosoftOAuth,
        allocator: std.mem.Allocator,
        client_id: []const u8,
        refresh_token: []const u8,
    ) !OAuthCredentialResult {
        try validateClientId(client_id);
        try validateOpaqueInput(refresh_token, error.InvalidRefreshToken);
        try validateEndpoint(oauth.endpoints.oauth_token);
        const form = try encodeForm(allocator, &.{
            .{ .name = "client_id", .value = client_id },
            .{ .name = "grant_type", .value = "refresh_token" },
            .{ .name = "refresh_token", .value = refresh_token },
            .{ .name = "scope", .value = microsoft_scope },
        });
        defer freeSecret(allocator, form);

        var response = try oauth.transport.postForm(allocator, .{ .url = oauth.endpoints.oauth_token, .body = form });
        defer response.deinit(allocator);
        if (response.status != .ok) return classifyGeneralFailure(allocator, response);

        const arena = try createArena(allocator);
        errdefer destroyArena(arena);
        const wire = parseWire(TokenWire, arena.allocator(), try response.body()) catch |err| return err;
        errdefer wipeTokenWire(wire);
        return .{
            .arena = arena,
            .value = try credentialFromWire(arena.allocator(), client_id, wire, refresh_token),
        };
    }

    pub fn minecraftExchange(
        oauth: MicrosoftOAuth,
        allocator: std.mem.Allocator,
        microsoft: MicrosoftAccess,
    ) !SessionResult {
        try validateClientId(microsoft.client_id);
        try validateOpaqueInput(microsoft.access_token, error.InvalidAccessToken);
        try validateEndpoint(oauth.endpoints.xbox_user_authentication);
        try validateEndpoint(oauth.endpoints.xbox_security_token);
        try validateEndpoint(oauth.endpoints.minecraft_login);
        try validateEndpoint(oauth.endpoints.minecraft_entitlements);
        try validateEndpoint(oauth.endpoints.minecraft_profile);

        const XstsStage = struct {
            identity: XboxIdentity,
            xuid: ?[]u8,
        };
        var xsts_stage: XstsStage = xsts_stage: {
            var xbox = xbox: {
                const rps_ticket = try std.fmt.allocPrint(allocator, "d={s}", .{microsoft.access_token});
                defer freeSecret(allocator, rps_ticket);
                const payload = try std.json.Stringify.valueAlloc(allocator, XboxUserAuthRequest{
                    .Properties = .{ .RpsTicket = rps_ticket },
                }, .{});
                defer freeSecret(allocator, payload);
                var response = try oauth.transport.postJson(allocator, .{
                    .url = oauth.endpoints.xbox_user_authentication,
                    .body = payload,
                });
                defer response.deinit(allocator);
                if (response.status == .too_many_requests) return error.RateLimited;
                if (response.status != .ok) return error.XboxAuthFailed;
                break :xbox try parseXboxIdentity(allocator, try response.body());
            };
            defer xbox.deinit();

            const user_tokens = [_][]const u8{xbox.token};
            const payload = try std.json.Stringify.valueAlloc(allocator, XstsRequest{
                .Properties = .{ .UserTokens = &user_tokens },
            }, .{});
            defer freeSecret(allocator, payload);
            var xsts = xsts: {
                var response = try oauth.transport.postJson(allocator, .{
                    .url = oauth.endpoints.xbox_security_token,
                    .body = payload,
                });
                defer response.deinit(allocator);
                if (response.status == .too_many_requests) return error.RateLimited;
                if (response.status == .bad_request or response.status == .unauthorized) {
                    return classifyXstsResponse(allocator, try response.body());
                }
                if (response.status != .ok) return error.XboxAuthFailed;
                break :xsts try parseXboxIdentity(allocator, try response.body());
            };
            errdefer xsts.deinit();
            const selected_xuid = xsts.xuid orelse xbox.xuid;
            const xuid = if (selected_xuid) |value| try allocator.dupe(u8, value) else null;
            errdefer if (xuid) |value| freeSecret(allocator, value);
            break :xsts_stage .{ .identity = xsts, .xuid = xuid };
        };
        defer if (xsts_stage.xuid) |value| freeSecret(allocator, value);

        var minecraft = minecraft: {
            defer xsts_stage.identity.deinit();
            const identity_token = try std.fmt.allocPrint(allocator, "XBL3.0 x={s};{s}", .{
                xsts_stage.identity.user_hash,
                xsts_stage.identity.token,
            });
            defer freeSecret(allocator, identity_token);
            const payload = try std.json.Stringify.valueAlloc(allocator, MinecraftLoginRequest{
                .identityToken = identity_token,
            }, .{});
            defer freeSecret(allocator, payload);
            var response = try oauth.transport.postJson(allocator, .{
                .url = oauth.endpoints.minecraft_login,
                .body = payload,
            });
            defer response.deinit(allocator);
            try requireMinecraftSuccess(response.status, false);
            break :minecraft try parseMinecraftAccess(allocator, try response.body());
        };
        defer minecraft.deinit();

        const owns_java = owns: {
            var response = try oauth.transport.getBearer(allocator, .{
                .url = oauth.endpoints.minecraft_entitlements,
                .bearer_token = minecraft.access_token,
            });
            defer response.deinit(allocator);
            try requireMinecraftSuccess(response.status, false);
            break :owns try hasJavaEntitlement(allocator, try response.body());
        };
        if (!owns_java) return error.GameNotOwned;

        const arena = try createArena(allocator);
        errdefer destroyArena(arena);
        const profile = profile: {
            var response = try oauth.transport.getBearer(allocator, .{
                .url = oauth.endpoints.minecraft_profile,
                .bearer_token = minecraft.access_token,
            });
            defer response.deinit(allocator);
            try requireMinecraftSuccess(response.status, true);
            break :profile parseWire(MinecraftProfileWire, arena.allocator(), try response.body()) catch |err| return err;
        };
        errdefer {
            wipeString(profile.id);
            wipeString(profile.name);
        }
        try validateProfile(profile);
        const session_access_token = try arena.allocator().dupe(u8, minecraft.access_token);
        errdefer wipeString(session_access_token);
        const session_xuid = if (xsts_stage.xuid) |value| try arena.allocator().dupe(u8, value) else null;
        errdefer if (session_xuid) |value| wipeString(value);
        const session_client_id = try arena.allocator().dupe(u8, microsoft.client_id);
        errdefer wipeString(session_client_id);
        return .{
            .arena = arena,
            .value = .{
                .profile = .{ .id = profile.id, .name = profile.name },
                .access_token = session_access_token,
                .expires_in = minecraft.expires_in,
                .xuid = session_xuid,
                .client_id = session_client_id,
            },
        };
    }

    /// Refreshes a durable Microsoft credential and produces a current Minecraft
    /// launch session without returning the intermediate Microsoft access token.
    pub fn refreshMinecraftSession(
        oauth: MicrosoftOAuth,
        allocator: std.mem.Allocator,
        client_id: []const u8,
        refresh_token: []const u8,
    ) !RefreshExchangeResult {
        var credential = try oauth.refresh(allocator, client_id, refresh_token);
        var credential_active = true;
        defer if (credential_active) credential.deinit();
        var session = oauth.minecraftExchange(allocator, credential.value.access()) catch |err| {
            return preserveRotatedRefresh(&credential, &credential_active, classifyExchangeFailure(err));
        };
        defer session.deinit();

        const arena = createArena(allocator) catch {
            return preserveRotatedRefresh(&credential, &credential_active, .unexpected);
        };
        const result_allocator = arena.allocator();
        const result_refresh_token = result_allocator.dupe(u8, credential.value.refresh_token) catch {
            destroyArena(arena);
            return preserveRotatedRefresh(&credential, &credential_active, .unexpected);
        };
        const result_session = copySession(result_allocator, session.value) catch {
            destroyArena(arena);
            return preserveRotatedRefresh(&credential, &credential_active, .unexpected);
        };
        return .{
            .arena = arena,
            .value = .{ .session = .{
                .refresh_token = result_refresh_token,
                .session = result_session,
            } },
        };
    }
};

const FormPair = struct {
    name: []const u8,
    value: []const u8,
};

const DeviceAuthorizationWire = struct {
    device_code: []const u8,
    user_code: []const u8,
    verification_uri: []const u8,
    verification_uri_complete: ?[]const u8 = null,
    expires_in: u32,
    interval: ?u32 = null,
    message: ?[]const u8 = null,
};

const TokenWire = struct {
    token_type: []const u8,
    scope: ?[]const u8 = null,
    expires_in: u32,
    access_token: []const u8,
    refresh_token: ?[]const u8 = null,
};

const OAuthErrorWire = struct {
    @"error": []const u8,
};

const XboxUserAuthRequest = struct {
    Properties: struct {
        AuthMethod: []const u8 = "RPS",
        SiteName: []const u8 = "user.auth.xboxlive.com",
        RpsTicket: []const u8,
    },
    RelyingParty: []const u8 = "http://auth.xboxlive.com",
    TokenType: []const u8 = "JWT",
};

const XstsRequest = struct {
    Properties: struct {
        SandboxId: []const u8 = "RETAIL",
        UserTokens: []const []const u8,
    },
    RelyingParty: []const u8 = "rp://api.minecraftservices.com/",
    TokenType: []const u8 = "JWT",
};

const MinecraftLoginRequest = struct {
    identityToken: []const u8,
};

const XboxClaimWire = struct {
    uhs: []const u8,
    xid: ?[]const u8 = null,
};

const XboxTokenWire = struct {
    Token: []const u8,
    DisplayClaims: struct {
        xui: []const XboxClaimWire,
    },
};

const XstsErrorWire = struct {
    XErr: ?u64 = null,
};

const MinecraftLoginWire = struct {
    access_token: []const u8,
    token_type: []const u8,
    expires_in: u32,
};

const EntitlementWire = struct {
    name: []const u8,
};

const EntitlementsWire = struct {
    items: []const EntitlementWire,
};

const MinecraftProfileWire = struct {
    id: []const u8,
    name: []const u8,
};

const XboxIdentity = struct {
    arena: *SecureArena,
    token: []const u8,
    user_hash: []const u8,
    xuid: ?[]const u8,

    fn deinit(identity: *XboxIdentity) void {
        wipeString(identity.token);
        wipeString(identity.user_hash);
        if (identity.xuid) |value| wipeString(value);
        destroyArena(identity.arena);
        identity.* = undefined;
    }
};

const MinecraftAccess = struct {
    arena: *SecureArena,
    access_token: []const u8,
    expires_in: u32,

    fn deinit(access: *MinecraftAccess) void {
        wipeString(access.access_token);
        destroyArena(access.arena);
        access.* = undefined;
    }
};

fn parseXboxIdentity(allocator: std.mem.Allocator, body: []const u8) !XboxIdentity {
    const arena = try createArena(allocator);
    errdefer destroyArena(arena);
    const wire = try parseWire(XboxTokenWire, arena.allocator(), body);
    errdefer {
        wipeString(wire.Token);
        for (wire.DisplayClaims.xui) |claim| {
            wipeString(claim.uhs);
            if (claim.xid) |value| wipeString(value);
        }
    }
    try validateOpaqueResponse(wire.Token);
    if (wire.DisplayClaims.xui.len == 0) return error.InvalidResponse;
    const claim = wire.DisplayClaims.xui[0];
    try validateOpaqueResponse(claim.uhs);
    if (claim.xid) |xuid| try validateOpaqueResponse(xuid);
    return .{
        .arena = arena,
        .token = wire.Token,
        .user_hash = claim.uhs,
        .xuid = claim.xid,
    };
}

fn parseMinecraftAccess(allocator: std.mem.Allocator, body: []const u8) !MinecraftAccess {
    const arena = try createArena(allocator);
    errdefer destroyArena(arena);
    const wire = try parseWire(MinecraftLoginWire, arena.allocator(), body);
    errdefer wipeString(wire.access_token);
    if (!std.ascii.eqlIgnoreCase(wire.token_type, "Bearer") or wire.expires_in == 0) {
        return error.InvalidResponse;
    }
    try validateBearerToken(wire.access_token);
    return .{
        .arena = arena,
        .access_token = wire.access_token,
        .expires_in = wire.expires_in,
    };
}

fn classifyXstsResponse(allocator: std.mem.Allocator, body: []const u8) anyerror {
    var parsed = std.json.parseFromSlice(XstsErrorWire, allocator, body, .{
        .ignore_unknown_fields = true,
    }) catch |err| return switch (err) {
        error.OutOfMemory => error.OutOfMemory,
        else => error.XboxAuthFailed,
    };
    defer parsed.deinit();
    const code = parsed.value.XErr orelse return error.XboxAuthFailed;
    return switch (classifyXstsFailure(code)) {
        .account_banned => error.XboxAccountBanned,
        .xbox_account_missing => error.XboxAccountRequired,
        .region_unsupported => error.XboxRegionUnsupported,
        .adult_verification_required => error.XboxAdultVerificationRequired,
        .child_account_restricted => error.XboxChildAccountRestricted,
        .unknown => error.XboxAuthFailed,
    };
}

fn requireMinecraftSuccess(status: std.http.Status, profile: bool) !void {
    if (status == .ok) return;
    if (status == .too_many_requests) return error.RateLimited;
    if (profile and status == .not_found) return error.ProfileNotFound;
    if (status == .unauthorized or status == .forbidden) return error.MinecraftAccessDenied;
    return error.MinecraftServiceError;
}

fn hasJavaEntitlement(allocator: std.mem.Allocator, body: []const u8) !bool {
    var parsed = std.json.parseFromSlice(EntitlementsWire, allocator, body, .{
        .ignore_unknown_fields = true,
        .max_value_len = 256,
    }) catch |err| return switch (err) {
        error.OutOfMemory => error.OutOfMemory,
        else => error.InvalidResponse,
    };
    defer parsed.deinit();
    for (parsed.value.items) |item| {
        if (std.mem.eql(u8, item.name, "product_minecraft") or
            std.mem.eql(u8, item.name, "game_minecraft") or
            std.mem.eql(u8, item.name, "product_game_pass_pc") or
            std.mem.eql(u8, item.name, "product_game_pass_ultimate"))
        {
            return true;
        }
    }
    return false;
}

fn validateProfile(profile: MinecraftProfileWire) !void {
    if (profile.id.len != 32) return error.InvalidResponse;
    for (profile.id) |byte| if (!std.ascii.isHex(byte)) return error.InvalidResponse;
    if (profile.name.len == 0 or profile.name.len > 16) return error.InvalidResponse;
    for (profile.name) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '_') return error.InvalidResponse;
    }
}

fn copySession(allocator: std.mem.Allocator, session: Session) !Session {
    return .{
        .profile = .{
            .id = try allocator.dupe(u8, session.profile.id),
            .name = try allocator.dupe(u8, session.profile.name),
        },
        .access_token = try allocator.dupe(u8, session.access_token),
        .expires_in = session.expires_in,
        .xuid = if (session.xuid) |value| try allocator.dupe(u8, value) else null,
        .client_id = try allocator.dupe(u8, session.client_id),
    };
}

fn classifyExchangeFailure(failure: anyerror) ExchangeFailure {
    return switch (failure) {
        error.Canceled => .canceled,
        error.NetworkError => .network_error,
        error.Timeout => .timeout,
        error.InvalidResponse, error.InvalidEndpoint, error.ResponseTooLarge => .invalid_response,
        error.RateLimited => .rate_limited,
        error.XboxAccountBanned => .xbox_account_banned,
        error.XboxAccountRequired => .xbox_account_required,
        error.XboxRegionUnsupported => .xbox_region_unsupported,
        error.XboxAdultVerificationRequired => .xbox_adult_verification_required,
        error.XboxChildAccountRestricted => .xbox_child_account_restricted,
        error.XboxAuthFailed => .xbox_auth_failed,
        error.MinecraftAccessDenied => .minecraft_access_denied,
        error.MinecraftServiceError => .minecraft_service_error,
        error.GameNotOwned => .game_not_owned,
        error.ProfileNotFound => .profile_not_found,
        else => .unexpected,
    };
}

fn preserveRotatedRefresh(
    credential: *OAuthCredentialResult,
    credential_active: *bool,
    failure: ExchangeFailure,
) RefreshExchangeResult {
    wipeCredentialExceptRefresh(&credential.value);
    const arena = credential.arena;
    const refresh_token = credential.value.refresh_token;
    credential_active.* = false;
    credential.* = undefined;
    return .{
        .arena = arena,
        .value = .{ .exchange_failed = .{
            .refresh_token = refresh_token,
            .failure = failure,
        } },
    };
}

fn encodeForm(allocator: std.mem.Allocator, pairs: []const FormPair) ![]u8 {
    var total: usize = 0;
    for (pairs, 0..) |pair, index| {
        if (index != 0) total = std.math.add(usize, total, 1) catch return error.OutOfMemory;
        total = std.math.add(usize, total, formComponentLength(pair.name)) catch return error.OutOfMemory;
        total = std.math.add(usize, total, 1) catch return error.OutOfMemory;
        total = std.math.add(usize, total, formComponentLength(pair.value)) catch return error.OutOfMemory;
    }
    const output = try allocator.alloc(u8, total);
    var output_index: usize = 0;
    for (pairs, 0..) |pair, pair_index| {
        if (pair_index != 0) {
            output[output_index] = '&';
            output_index += 1;
        }
        writeFormComponent(output, &output_index, pair.name);
        output[output_index] = '=';
        output_index += 1;
        writeFormComponent(output, &output_index, pair.value);
    }
    std.debug.assert(output_index == output.len);
    return output;
}

fn formComponentLength(value: []const u8) usize {
    var total: usize = 0;
    for (value) |byte| {
        total += if (formByteUnescaped(byte) or byte == ' ') 1 else 3;
    }
    return total;
}

fn writeFormComponent(output: []u8, output_index: *usize, value: []const u8) void {
    const hex = "0123456789ABCDEF";
    for (value) |byte| {
        if (formByteUnescaped(byte)) {
            output[output_index.*] = byte;
            output_index.* += 1;
        } else if (byte == ' ') {
            output[output_index.*] = '+';
            output_index.* += 1;
        } else {
            output[output_index.*] = '%';
            output[output_index.* + 1] = hex[byte >> 4];
            output[output_index.* + 2] = hex[byte & 0x0f];
            output_index.* += 3;
        }
    }
}

fn formByteUnescaped(byte: u8) bool {
    return std.ascii.isAlphanumeric(byte) or byte == '-' or byte == '.' or byte == '_' or byte == '~';
}

fn endpointWithLocale(allocator: std.mem.Allocator, endpoint: []const u8, locale: ?[]const u8) ![]u8 {
    if (std.mem.indexOfScalar(u8, endpoint, '#') != null) return error.InvalidEndpoint;
    const selected = locale orelse return allocator.dupe(u8, endpoint);
    const separator: u8 = if (std.mem.indexOfScalar(u8, endpoint, '?') == null) '?' else '&';
    return std.fmt.allocPrint(allocator, "{s}{c}mkt={s}", .{ endpoint, separator, selected });
}

fn parseWire(comptime T: type, allocator: std.mem.Allocator, body: []const u8) !T {
    return std.json.parseFromSliceLeaky(T, allocator, body, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
        .max_value_len = max_oauth_value_bytes,
    }) catch |err| switch (err) {
        error.OutOfMemory => error.OutOfMemory,
        else => error.InvalidResponse,
    };
}

fn validateDeviceAuthorization(wire: DeviceAuthorizationWire) !void {
    try validateOpaqueResponse(wire.device_code);
    try validateOpaqueResponse(wire.user_code);
    if (!validHttpsUri(wire.verification_uri)) return error.InvalidResponse;
    if (wire.verification_uri_complete) |uri| {
        if (!validHttpsUri(uri)) return error.InvalidResponse;
    }
    if (wire.expires_in == 0 or (wire.interval != null and wire.interval.? == 0)) return error.InvalidResponse;
}

fn credentialFromWire(
    allocator: std.mem.Allocator,
    client_id: []const u8,
    wire: TokenWire,
    fallback_refresh_token: ?[]const u8,
) !OAuthCredential {
    if (!std.ascii.eqlIgnoreCase(wire.token_type, "Bearer") or wire.expires_in == 0) return error.InvalidResponse;
    try validateOpaqueResponse(wire.access_token);
    const owned_client_id = try allocator.dupe(u8, client_id);
    errdefer wipeString(owned_client_id);
    const refresh_token = if (wire.refresh_token) |replacement| refresh: {
        try validateOpaqueResponse(replacement);
        break :refresh replacement;
    } else if (fallback_refresh_token) |fallback|
        try allocator.dupe(u8, fallback)
    else
        return error.InvalidResponse;
    return .{
        .client_id = owned_client_id,
        .token_type = wire.token_type,
        .scope = wire.scope,
        .expires_in = wire.expires_in,
        .access_token = wire.access_token,
        .refresh_token = refresh_token,
    };
}

fn classifyGeneralFailure(allocator: std.mem.Allocator, response: HttpResponse) anyerror {
    if (response.status == .too_many_requests) return error.RateLimited;
    if (response.status != .bad_request and response.status != .unauthorized) {
        return classifyServiceFailure(response.status);
    }
    const body = response.body() catch return error.InvalidResponse;
    var parsed = std.json.parseFromSlice(OAuthErrorWire, allocator, body, .{
        .ignore_unknown_fields = true,
        .max_value_len = 128,
    }) catch |err| return switch (err) {
        error.OutOfMemory => error.OutOfMemory,
        else => error.InvalidResponse,
    };
    defer parsed.deinit();
    if (parsed.value.@"error".len == 0) return error.InvalidResponse;
    return error.OAuthRejected;
}

fn classifyPollFailure(allocator: std.mem.Allocator, body: []const u8) !DevicePollState {
    var parsed = std.json.parseFromSlice(OAuthErrorWire, allocator, body, .{
        .ignore_unknown_fields = true,
        .max_value_len = 128,
    }) catch |err| return switch (err) {
        error.OutOfMemory => error.OutOfMemory,
        else => error.InvalidResponse,
    };
    defer parsed.deinit();
    const code = parsed.value.@"error";
    if (std.mem.eql(u8, code, "authorization_pending")) return .authorization_pending;
    if (std.mem.eql(u8, code, "slow_down")) return .slow_down;
    if (std.mem.eql(u8, code, "authorization_declined")) return error.AuthorizationDeclined;
    if (std.mem.eql(u8, code, "access_denied")) return error.AuthorizationDeclined;
    if (std.mem.eql(u8, code, "expired_token")) return error.DeviceCodeExpired;
    if (code.len == 0) return error.InvalidResponse;
    return error.OAuthRejected;
}

fn classifyServiceFailure(status: std.http.Status) anyerror {
    _ = status;
    return error.OAuthServiceError;
}

fn validateOpaqueInput(value: []const u8, invalid_error: anyerror) !void {
    if (value.len == 0 or value.len > max_oauth_value_bytes or !std.unicode.utf8ValidateSlice(value)) {
        return invalid_error;
    }
}

fn validateOpaqueResponse(value: []const u8) !void {
    if (value.len == 0 or value.len > max_oauth_value_bytes) return error.InvalidResponse;
}

fn validateBearerToken(value: []const u8) !void {
    try validateOpaqueResponse(value);
    for (value) |byte| {
        if (byte < 0x21 or byte > 0x7e) return error.InvalidResponse;
    }
}

fn validHttpsUri(value: []const u8) bool {
    if (value.len > 4096) return false;
    const uri = std.Uri.parse(value) catch return false;
    return validHttpsUriParts(uri, true);
}

fn validateEndpoint(value: []const u8) !void {
    if (value.len == 0 or value.len > 4096) return error.InvalidEndpoint;
    const uri = std.Uri.parse(value) catch return error.InvalidEndpoint;
    if (!validHttpsUriParts(uri, false)) return error.InvalidEndpoint;
}

fn validHttpsUriParts(uri: std.Uri, allow_fragment: bool) bool {
    if (!std.mem.eql(u8, uri.scheme, "https") or uri.user != null or uri.password != null) return false;
    if (!allow_fragment and uri.fragment != null) return false;
    const component = uri.host orelse return false;
    var host_buffer: [std.Io.net.HostName.max_len]u8 = undefined;
    const host = component.toRaw(&host_buffer) catch return false;
    std.Io.net.HostName.validate(host) catch return false;
    return true;
}

fn createArena(allocator: std.mem.Allocator) !*SecureArena {
    return SecureArena.create(allocator);
}

fn destroyArena(arena: *SecureArena) void {
    arena.deinit();
}

fn freeSecret(allocator: std.mem.Allocator, value: []u8) void {
    std.crypto.secureZero(u8, value);
    allocator.free(value);
}

fn wipeOwnedValue(comptime T: type, value: *T) void {
    if (T == DeviceAuthorization) {
        wipeString(value.device_code);
        wipeString(value.user_code);
        wipeString(value.verification_uri);
        if (value.verification_uri_complete) |text| wipeString(text);
        if (value.message) |text| wipeString(text);
    } else if (T == OAuthCredential) {
        wipeCredential(value);
    } else if (T == DevicePollResult) {
        switch (value.*) {
            .pending => {},
            .authenticated => |*credential| wipeCredential(credential),
        }
    } else if (T == Session) {
        wipeSession(value);
    } else if (T == RefreshExchange) {
        switch (value.*) {
            .session => |*result| {
                wipeString(result.refresh_token);
                wipeSession(&result.session);
            },
            .exchange_failed => |*result| wipeString(result.refresh_token),
        }
    }
}

fn wipeSession(session: *Session) void {
    wipeString(session.profile.id);
    wipeString(session.profile.name);
    wipeString(session.access_token);
    if (session.xuid) |text| wipeString(text);
    wipeString(session.client_id);
}

fn wipeCredential(credential: *OAuthCredential) void {
    wipeString(credential.client_id);
    wipeString(credential.token_type);
    if (credential.scope) |text| wipeString(text);
    wipeString(credential.access_token);
    wipeString(credential.refresh_token);
}

fn wipeCredentialExceptRefresh(credential: *OAuthCredential) void {
    wipeString(credential.client_id);
    wipeString(credential.token_type);
    if (credential.scope) |text| wipeString(text);
    wipeString(credential.access_token);
}

fn wipeDeviceAuthorizationWire(wire: DeviceAuthorizationWire) void {
    wipeString(wire.device_code);
    wipeString(wire.user_code);
    wipeString(wire.verification_uri);
    if (wire.verification_uri_complete) |value| wipeString(value);
    if (wire.message) |value| wipeString(value);
}

fn wipeTokenWire(wire: TokenWire) void {
    wipeString(wire.token_type);
    if (wire.scope) |value| wipeString(value);
    wipeString(wire.access_token);
    if (wire.refresh_token) |value| wipeString(value);
}

fn wipeString(value: []const u8) void {
    std.crypto.secureZero(u8, @constCast(value));
}

const FakeTransport = struct {
    status: std.http.Status = .ok,
    response_body: []const u8 = "{}",
    failure: ?TransportError = null,
    calls: usize = 0,
    url_buffer: [1024]u8 = undefined,
    url_len: usize = 0,
    body_buffer: [2048]u8 = undefined,
    body_len: usize = 0,
    response_limit: usize = 0,
    timeout_ms: u32 = 0,
    reported_body_len: ?usize = null,

    fn transport(fake: *FakeTransport) Transport {
        return .{ .context = fake, .request_fn = request };
    }

    fn request(
        raw_context: *anyopaque,
        allocator: std.mem.Allocator,
        http_request: HttpRequest,
    ) TransportError!HttpResponse {
        const fake: *FakeTransport = @ptrCast(@alignCast(raw_context));
        const form = switch (http_request) {
            .form_post => |value| value,
            else => return error.InvalidResponse,
        };
        fake.calls += 1;
        if (fake.failure) |failure| return failure;
        if (form.url.len > fake.url_buffer.len or form.body.len > fake.body_buffer.len) {
            return error.ResponseTooLarge;
        }
        @memcpy(fake.url_buffer[0..form.url.len], form.url);
        fake.url_len = form.url.len;
        @memcpy(fake.body_buffer[0..form.body.len], form.body);
        fake.body_len = form.body.len;
        fake.response_limit = form.max_response_bytes;
        fake.timeout_ms = form.timeout_ms;
        if (fake.response_body.len > form.max_response_bytes) return error.ResponseTooLarge;
        return .{
            .status = fake.status,
            .storage = try allocator.dupe(u8, fake.response_body),
            .body_len = fake.reported_body_len orelse fake.response_body.len,
        };
    }

    fn url(fake: *const FakeTransport) []const u8 {
        return fake.url_buffer[0..fake.url_len];
    }

    fn body(fake: *const FakeTransport) []const u8 {
        return fake.body_buffer[0..fake.body_len];
    }
};

const ExchangeStep = struct {
    status: std.http.Status = .ok,
    body: []const u8,
    failure: ?TransportError = null,
};

const CapturedRequest = struct {
    kind: enum { json_post, bearer_get },
    url_buffer: [256]u8 = undefined,
    url_len: usize = 0,
    body_buffer: [2048]u8 = undefined,
    body_len: usize = 0,
    bearer_buffer: [256]u8 = undefined,
    bearer_len: usize = 0,

    fn url(captured: *const CapturedRequest) []const u8 {
        return captured.url_buffer[0..captured.url_len];
    }

    fn body(captured: *const CapturedRequest) []const u8 {
        return captured.body_buffer[0..captured.body_len];
    }

    fn bearer(captured: *const CapturedRequest) []const u8 {
        return captured.bearer_buffer[0..captured.bearer_len];
    }
};

const ExchangeFakeTransport = struct {
    steps: []const ExchangeStep,
    calls: usize = 0,
    captured: [5]CapturedRequest = undefined,

    fn transport(fake: *ExchangeFakeTransport) Transport {
        return .{ .context = fake, .request_fn = request };
    }

    fn request(
        raw_context: *anyopaque,
        allocator: std.mem.Allocator,
        http_request: HttpRequest,
    ) TransportError!HttpResponse {
        const fake: *ExchangeFakeTransport = @ptrCast(@alignCast(raw_context));
        if (fake.calls >= fake.steps.len or fake.calls >= fake.captured.len) return error.InvalidResponse;
        const index = fake.calls;
        fake.calls += 1;
        const max_response_bytes = switch (http_request) {
            .form_post => return error.InvalidResponse,
            .json_post => |request_value| capture: {
                var captured: CapturedRequest = .{ .kind = .json_post };
                try captureBytes(&captured.url_buffer, &captured.url_len, request_value.url);
                try captureBytes(&captured.body_buffer, &captured.body_len, request_value.body);
                fake.captured[index] = captured;
                break :capture request_value.max_response_bytes;
            },
            .bearer_get => |request_value| capture: {
                var captured: CapturedRequest = .{ .kind = .bearer_get };
                try captureBytes(&captured.url_buffer, &captured.url_len, request_value.url);
                try captureBytes(&captured.bearer_buffer, &captured.bearer_len, request_value.bearer_token);
                fake.captured[index] = captured;
                break :capture request_value.max_response_bytes;
            },
        };
        const step = fake.steps[index];
        if (step.failure) |failure| return failure;
        if (step.body.len > max_response_bytes) return error.ResponseTooLarge;
        return .{
            .status = step.status,
            .storage = try allocator.dupe(u8, step.body),
            .body_len = step.body.len,
        };
    }

    fn captureBytes(buffer: anytype, length: *usize, value: []const u8) TransportError!void {
        if (value.len > buffer.len) return error.ResponseTooLarge;
        @memcpy(buffer[0..value.len], value);
        length.* = value.len;
    }
};

const RefreshExchangeFakeTransport = struct {
    steps: []const ExchangeStep,
    calls: usize = 0,
    refresh_form_buffer: [2048]u8 = undefined,
    refresh_form_len: usize = 0,

    fn transport(fake: *RefreshExchangeFakeTransport) Transport {
        return .{ .context = fake, .request_fn = request };
    }

    fn request(
        raw_context: *anyopaque,
        allocator: std.mem.Allocator,
        http_request: HttpRequest,
    ) TransportError!HttpResponse {
        const fake: *RefreshExchangeFakeTransport = @ptrCast(@alignCast(raw_context));
        if (fake.calls >= fake.steps.len) return error.InvalidResponse;
        const index = fake.calls;
        fake.calls += 1;
        const maximum = switch (http_request) {
            .form_post => |request_value| form: {
                if (index != 0) return error.InvalidResponse;
                if (request_value.body.len > fake.refresh_form_buffer.len) return error.ResponseTooLarge;
                @memcpy(fake.refresh_form_buffer[0..request_value.body.len], request_value.body);
                fake.refresh_form_len = request_value.body.len;
                break :form request_value.max_response_bytes;
            },
            .json_post => |request_value| request_value.max_response_bytes,
            .bearer_get => |request_value| request_value.max_response_bytes,
        };
        const step = fake.steps[index];
        if (step.failure) |failure| return failure;
        if (step.body.len > maximum) return error.ResponseTooLarge;
        return .{
            .status = step.status,
            .storage = try allocator.dupe(u8, step.body),
            .body_len = step.body.len,
        };
    }
};

const successful_exchange_steps = [_]ExchangeStep{
    .{ .body =
    \\{"Token":"xbox-token","DisplayClaims":{"xui":[{"uhs":"user-hash","xid":"111"}]}}
    },
    .{ .body =
    \\{"Token":"xsts-token","DisplayClaims":{"xui":[{"uhs":"user-hash","xid":"222"}]}}
    },
    .{ .body =
    \\{"access_token":"minecraft-token","token_type":"Bearer","expires_in":86400}
    },
    .{ .body =
    \\{"items":[{"name":"product_game_pass_pc"}]}
    },
    .{ .body =
    \\{"id":"0123456789abcdef0123456789abcdef","name":"Player_1"}
    },
};

test "exchanges Microsoft access for an owned Minecraft session" {
    var fake: ExchangeFakeTransport = .{ .steps = &successful_exchange_steps };
    const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
    var result = try oauth.minecraftExchange(std.testing.allocator, .{
        .client_id = "client-id",
        .access_token = "ms-access",
    });
    defer result.deinit();

    try std.testing.expectEqual(@as(usize, 5), fake.calls);
    try std.testing.expectEqualStrings("0123456789abcdef0123456789abcdef", result.value.profile.id);
    try std.testing.expectEqualStrings("Player_1", result.value.profile.name);
    try std.testing.expectEqualStrings("minecraft-token", result.value.access_token);
    try std.testing.expectEqualStrings("222", result.value.xuid.?);
    try std.testing.expectEqualStrings("client-id", result.value.client_id);

    try std.testing.expectEqualStrings(official_endpoints.xbox_user_authentication, fake.captured[0].url());
    try std.testing.expectEqualStrings(
        "{\"Properties\":{\"AuthMethod\":\"RPS\",\"SiteName\":\"user.auth.xboxlive.com\",\"RpsTicket\":\"d=ms-access\"},\"RelyingParty\":\"http://auth.xboxlive.com\",\"TokenType\":\"JWT\"}",
        fake.captured[0].body(),
    );
    try std.testing.expectEqualStrings(
        "{\"Properties\":{\"SandboxId\":\"RETAIL\",\"UserTokens\":[\"xbox-token\"]},\"RelyingParty\":\"rp://api.minecraftservices.com/\",\"TokenType\":\"JWT\"}",
        fake.captured[1].body(),
    );
    try std.testing.expectEqualStrings(
        "{\"identityToken\":\"XBL3.0 x=user-hash;xsts-token\"}",
        fake.captured[2].body(),
    );
    try std.testing.expectEqualStrings("minecraft-token", fake.captured[3].bearer());
    try std.testing.expectEqualStrings("minecraft-token", fake.captured[4].bearer());
}

test "refreshes and exchanges without returning Microsoft access credentials" {
    const steps = [_]ExchangeStep{
        .{ .body =
        \\{"token_type":"Bearer","expires_in":3600,"access_token":"microsoft-access","refresh_token":"rotated-refresh"}
        },
        successful_exchange_steps[0],
        successful_exchange_steps[1],
        successful_exchange_steps[2],
        successful_exchange_steps[3],
        successful_exchange_steps[4],
    };
    var fake: RefreshExchangeFakeTransport = .{ .steps = &steps };
    const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
    var result = try oauth.refreshMinecraftSession(std.testing.allocator, "client-id", "old-refresh");
    defer result.deinit();

    try std.testing.expectEqual(@as(usize, 6), fake.calls);
    const session_result = result.value.session;
    try std.testing.expectEqualStrings("rotated-refresh", session_result.refresh_token);
    try std.testing.expectEqualStrings("minecraft-token", session_result.session.access_token);
    try std.testing.expectEqualStrings("Player_1", session_result.session.profile.name);
    try std.testing.expect(std.mem.indexOf(u8, fake.refresh_form_buffer[0..fake.refresh_form_len], "old-refresh") != null);
}

test "preserves a rotated refresh token when Minecraft exchange fails" {
    const steps = [_]ExchangeStep{
        .{ .body =
        \\{"token_type":"Bearer","expires_in":3600,"access_token":"microsoft-access","refresh_token":"rotated-refresh"}
        },
        .{ .status = .internal_server_error, .body = "{}" },
    };
    var fake: RefreshExchangeFakeTransport = .{ .steps = &steps };
    const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
    var result = try oauth.refreshMinecraftSession(std.testing.allocator, "client-id", "old-refresh");
    defer result.deinit();

    switch (result.value) {
        .session => unreachable,
        .exchange_failed => |failure| {
            try std.testing.expectEqualStrings("rotated-refresh", failure.refresh_token);
            try std.testing.expectEqual(ExchangeFailure.xbox_auth_failed, failure.failure);
        },
    }
    try std.testing.expectEqual(@as(usize, 2), fake.calls);
}

test "maps XSTS account restrictions without exposing response details" {
    const cases = [_]struct {
        code: u64,
        expected: anyerror,
    }{
        .{ .code = 2148916227, .expected = error.XboxAccountBanned },
        .{ .code = 2148916233, .expected = error.XboxAccountRequired },
        .{ .code = 2148916235, .expected = error.XboxRegionUnsupported },
        .{ .code = 2148916236, .expected = error.XboxAdultVerificationRequired },
        .{ .code = 2148916238, .expected = error.XboxChildAccountRestricted },
        .{ .code = 1, .expected = error.XboxAuthFailed },
    };
    for (cases) |case| {
        var error_body_buffer: [64]u8 = undefined;
        const error_body = try std.fmt.bufPrint(&error_body_buffer, "{{\"XErr\":{d}}}", .{case.code});
        const steps = [_]ExchangeStep{
            successful_exchange_steps[0],
            .{ .status = .unauthorized, .body = error_body },
        };
        var fake: ExchangeFakeTransport = .{ .steps = &steps };
        const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
        try std.testing.expectError(case.expected, oauth.minecraftExchange(std.testing.allocator, .{
            .client_id = "client-id",
            .access_token = "ms-access",
        }));
        try std.testing.expectEqual(@as(usize, 2), fake.calls);
    }
}

test "requires Java entitlement and a valid Minecraft profile" {
    var no_entitlement_steps = successful_exchange_steps;
    no_entitlement_steps[3] = .{ .body = "{\"items\":[{\"name\":\"product_minecraft_bedrock\"}]}" };
    var no_entitlement_fake: ExchangeFakeTransport = .{ .steps = &no_entitlement_steps };
    var oauth: MicrosoftOAuth = .{ .transport = no_entitlement_fake.transport() };
    try std.testing.expectError(error.GameNotOwned, oauth.minecraftExchange(std.testing.allocator, .{
        .client_id = "client-id",
        .access_token = "ms-access",
    }));
    try std.testing.expectEqual(@as(usize, 4), no_entitlement_fake.calls);

    var missing_profile_steps = successful_exchange_steps;
    missing_profile_steps[4] = .{ .status = .not_found, .body = "{}" };
    var missing_profile_fake: ExchangeFakeTransport = .{ .steps = &missing_profile_steps };
    oauth = .{ .transport = missing_profile_fake.transport() };
    try std.testing.expectError(error.ProfileNotFound, oauth.minecraftExchange(std.testing.allocator, .{
        .client_id = "client-id",
        .access_token = "ms-access",
    }));

    var invalid_profile_steps = successful_exchange_steps;
    invalid_profile_steps[4] = .{ .body = "{\"id\":\"not-a-uuid\",\"name\":\"Player\"}" };
    var invalid_profile_fake: ExchangeFakeTransport = .{ .steps = &invalid_profile_steps };
    oauth = .{ .transport = invalid_profile_fake.transport() };
    try std.testing.expectError(error.InvalidResponse, oauth.minecraftExchange(std.testing.allocator, .{
        .client_id = "client-id",
        .access_token = "ms-access",
    }));
}

test "maps Minecraft authentication and service statuses" {
    const cases = [_]struct {
        status: std.http.Status,
        expected: anyerror,
    }{
        .{ .status = .forbidden, .expected = error.MinecraftAccessDenied },
        .{ .status = .too_many_requests, .expected = error.RateLimited },
        .{ .status = .internal_server_error, .expected = error.MinecraftServiceError },
    };
    for (cases) |case| {
        const steps = [_]ExchangeStep{
            successful_exchange_steps[0],
            successful_exchange_steps[1],
            .{ .status = case.status, .body = "{}" },
        };
        var fake: ExchangeFakeTransport = .{ .steps = &steps };
        const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
        try std.testing.expectError(case.expected, oauth.minecraftExchange(std.testing.allocator, .{
            .client_id = "client-id",
            .access_token = "ms-access",
        }));
    }
}

fn minecraftExchangeAllocationTest(allocator: std.mem.Allocator) !void {
    var fake: ExchangeFakeTransport = .{ .steps = &successful_exchange_steps };
    const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
    var result = try oauth.minecraftExchange(allocator, .{
        .client_id = "client-id",
        .access_token = "ms-access",
    });
    defer result.deinit();
}

test "Minecraft exchange cleans up every allocation failure" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, minecraftExchangeAllocationTest, .{});
}

test "begins device authorization with encoded form and locale" {
    var fake: FakeTransport = .{
        .response_body =
        \\{"device_code":"device-secret","user_code":"ABCD-EFGH","verification_uri":"https://microsoft.com/link","expires_in":900,"message":"Open the page"}
        ,
    };
    const oauth: MicrosoftOAuth = .{
        .transport = fake.transport(),
        .endpoints = .{
            .device_authorization = "https://login.example/device?test=1",
            .oauth_token = "https://login.example/token",
            .xbox_user_authentication = "https://xbox.example/user",
            .xbox_security_token = "https://xbox.example/xsts",
            .minecraft_login = "https://minecraft.example/login",
            .minecraft_entitlements = "https://minecraft.example/entitlements",
            .minecraft_profile = "https://minecraft.example/profile",
        },
    };
    var result = try oauth.deviceBegin(std.testing.allocator, .{
        .client_id = "client +&=id",
        .locale = "zh-CN",
    });
    defer result.deinit();

    try std.testing.expectEqual(@as(usize, 1), fake.calls);
    try std.testing.expectEqualStrings("https://login.example/device?test=1&mkt=zh-CN", fake.url());
    try std.testing.expectEqualStrings(
        "client_id=client+%2B%26%3Did&scope=XboxLive.signin+offline_access",
        fake.body(),
    );
    try std.testing.expectEqual(max_oauth_response_bytes, fake.response_limit);
    try std.testing.expectEqual(oauth_request_timeout_ms, fake.timeout_ms);
    try std.testing.expectEqual(@as(u32, 5), result.value.interval);
    try std.testing.expectEqualStrings("device-secret", result.value.device_code);
}

test "maps one-shot device polling states and terminal failures" {
    const cases = [_]struct {
        status: std.http.Status,
        body: []const u8,
        state: ?DevicePollState = null,
        failure: ?anyerror = null,
    }{
        .{ .status = .bad_request, .body = "{\"error\":\"authorization_pending\"}", .state = .authorization_pending },
        .{ .status = .bad_request, .body = "{\"error\":\"slow_down\"}", .state = .slow_down },
        .{ .status = .bad_request, .body = "{\"error\":\"authorization_declined\"}", .failure = error.AuthorizationDeclined },
        .{ .status = .bad_request, .body = "{\"error\":\"expired_token\"}", .failure = error.DeviceCodeExpired },
        .{ .status = .too_many_requests, .body = "{}", .failure = error.RateLimited },
        .{ .status = .internal_server_error, .body = "{\"error\":\"authorization_pending\"}", .failure = error.OAuthServiceError },
    };
    for (cases) |case| {
        var fake: FakeTransport = .{ .status = case.status, .response_body = case.body };
        const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
        if (case.failure) |expected| {
            try std.testing.expectError(expected, oauth.devicePoll(std.testing.allocator, "client", "device"));
        } else {
            var result = try oauth.devicePoll(std.testing.allocator, "client", "device");
            defer result.deinit();
            try std.testing.expectEqual(case.state.?, result.value.pending);
        }
        try std.testing.expectEqual(@as(usize, 1), fake.calls);
    }
}

test "device poll returns an owned OAuth credential" {
    var fake: FakeTransport = .{
        .response_body =
        \\{"token_type":"Bearer","scope":"XboxLive.signin offline_access","expires_in":3600,"access_token":"access-value","refresh_token":"refresh-value"}
        ,
    };
    const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
    var result = try oauth.devicePoll(std.testing.allocator, "client", "device +&");
    defer result.deinit();

    const credential = result.value.authenticated;
    try std.testing.expectEqualStrings("access-value", credential.access_token);
    try std.testing.expectEqualStrings("refresh-value", credential.refresh_token);
    try std.testing.expectEqualStrings(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&client_id=client&device_code=device+%2B%26",
        fake.body(),
    );
}

test "refresh carries forward or replaces the refresh token" {
    const cases = [_]struct {
        body: []const u8,
        expected: []const u8,
    }{
        .{
            .body = "{\"token_type\":\"Bearer\",\"expires_in\":3600,\"access_token\":\"new-access\"}",
            .expected = "old-refresh",
        },
        .{
            .body = "{\"token_type\":\"Bearer\",\"expires_in\":3600,\"access_token\":\"new-access\",\"refresh_token\":\"new-refresh\"}",
            .expected = "new-refresh",
        },
    };
    for (cases) |case| {
        var fake: FakeTransport = .{ .response_body = case.body };
        const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
        var input = [_]u8{ 'o', 'l', 'd', '-', 'r', 'e', 'f', 'r', 'e', 's', 'h' };
        var result = try oauth.refresh(std.testing.allocator, "client", &input);
        defer result.deinit();
        @memset(&input, 'x');
        try std.testing.expectEqualStrings(case.expected, result.value.refresh_token);
    }
}

test "rejects malformed OAuth responses and propagates transport errors" {
    const cases = [_]struct {
        status: std.http.Status = .ok,
        body: []const u8,
        failure: ?TransportError = null,
        expected: anyerror,
    }{
        .{ .body = "{}", .expected = error.InvalidResponse },
        .{
            .body = "{\"device_code\":\"d\",\"user_code\":\"u\",\"verification_uri\":\"http://insecure.example\",\"expires_in\":900}",
            .expected = error.InvalidResponse,
        },
        .{
            .body = "{\"device_code\":\"d\",\"user_code\":\"u\",\"verification_uri\":\"https://example.com\",\"expires_in\":900,\"interval\":0}",
            .expected = error.InvalidResponse,
        },
        .{ .status = .found, .body = "{}", .expected = error.OAuthServiceError },
        .{ .body = "{}", .failure = error.Timeout, .expected = error.Timeout },
        .{ .body = "{}", .failure = error.ResponseTooLarge, .expected = error.ResponseTooLarge },
    };
    for (cases) |case| {
        var fake: FakeTransport = .{
            .status = case.status,
            .response_body = case.body,
            .failure = case.failure,
        };
        const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
        try std.testing.expectError(case.expected, oauth.deviceBegin(std.testing.allocator, .{ .client_id = "client" }));
    }
}

test "rejects insecure endpoints and inconsistent transport responses" {
    var insecure_fake: FakeTransport = .{};
    const insecure: MicrosoftOAuth = .{
        .transport = insecure_fake.transport(),
        .endpoints = .{
            .device_authorization = "http://login.example/device",
            .oauth_token = "http://login.example/token",
            .xbox_user_authentication = "https://xbox.example/user",
            .xbox_security_token = "https://xbox.example/xsts",
            .minecraft_login = "https://minecraft.example/login",
            .minecraft_entitlements = "https://minecraft.example/entitlements",
            .minecraft_profile = "https://minecraft.example/profile",
        },
    };
    try std.testing.expectError(
        error.InvalidEndpoint,
        insecure.devicePoll(std.testing.allocator, "client", "device"),
    );
    try std.testing.expectEqual(@as(usize, 0), insecure_fake.calls);

    var malformed_fake: FakeTransport = .{
        .response_body = "{}",
        .reported_body_len = 100,
    };
    const malformed: MicrosoftOAuth = .{ .transport = malformed_fake.transport() };
    try std.testing.expectError(
        error.InvalidResponse,
        malformed.devicePoll(std.testing.allocator, "client", "device"),
    );
}

fn deviceBeginAllocationTest(allocator: std.mem.Allocator) !void {
    var fake: FakeTransport = .{
        .response_body =
        \\{"device_code":"device","user_code":"code","verification_uri":"https://microsoft.com/link","expires_in":900,"interval":5}
        ,
    };
    const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
    var result = try oauth.deviceBegin(allocator, .{ .client_id = "client" });
    defer result.deinit();
}

test "device authorization cleans up every allocation failure" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, deviceBeginAllocationTest, .{});
}

fn devicePollAllocationTest(allocator: std.mem.Allocator) !void {
    var fake: FakeTransport = .{
        .response_body =
        \\{"token_type":"Bearer","expires_in":3600,"access_token":"access","refresh_token":"refresh"}
        ,
    };
    const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
    var result = try oauth.devicePoll(allocator, "client", "device");
    defer result.deinit();
}

fn refreshAllocationTest(allocator: std.mem.Allocator) !void {
    var fake: FakeTransport = .{
        .response_body =
        \\{"token_type":"Bearer","expires_in":3600,"access_token":"access"}
        ,
    };
    const oauth: MicrosoftOAuth = .{ .transport = fake.transport() };
    var result = try oauth.refresh(allocator, "client", "refresh");
    defer result.deinit();
}

test "OAuth credentials clean up every allocation failure" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, devicePollAllocationTest, .{});
    try std.testing.checkAllAllocationFailures(std.testing.allocator, refreshAllocationTest, .{});
}

pub const XstsFailure = enum {
    account_banned,
    xbox_account_missing,
    region_unsupported,
    adult_verification_required,
    child_account_restricted,
    unknown,
};

pub fn classifyXstsFailure(code: u64) XstsFailure {
    return switch (code) {
        2148916227 => .account_banned,
        2148916233 => .xbox_account_missing,
        2148916235 => .region_unsupported,
        2148916236, 2148916237 => .adult_verification_required,
        2148916238 => .child_account_restricted,
        else => .unknown,
    };
}

fn validateClientId(client_id: []const u8) !void {
    if (client_id.len == 0 or client_id.len > 256 or !std.unicode.utf8ValidateSlice(client_id)) {
        return error.InvalidClientId;
    }
    for (client_id) |byte| {
        if (byte < 0x20 or byte == 0x7f) return error.InvalidClientId;
    }
}

fn validateLocale(locale: []const u8) !void {
    if (locale.len == 0 or locale.len > 35 or locale[0] == '-' or locale[locale.len - 1] == '-') {
        return error.InvalidLocale;
    }
    for (locale) |byte| {
        if (!std.ascii.isAlphanumeric(byte) and byte != '-') return error.InvalidLocale;
    }
}

test "validates configurable opaque Microsoft application IDs" {
    try (Config{
        .client_id = "00001111-aaaa-2222-bbbb-3333cccc4444",
        .locale = "zh-CN",
    }).validate();
    try (Config{ .client_id = "legacy-public-client" }).validate();
    try std.testing.expectError(error.InvalidClientId, (Config{ .client_id = "" }).validate());
    try std.testing.expectError(error.InvalidClientId, (Config{ .client_id = "bad\nclient" }).validate());
    try std.testing.expectError(error.InvalidLocale, (Config{
        .client_id = "00001111-aaaa-2222-bbbb-3333cccc4444",
        .locale = "zh_CN",
    }).validate());
}

test "requires canonical HTTPS authentication endpoints" {
    try validateEndpoint("https://login.microsoftonline.com/consumers/oauth2/v2.0/token");
    try std.testing.expectError(error.InvalidEndpoint, validateEndpoint("HTTPS://login.microsoftonline.com/token"));
    try std.testing.expectError(error.InvalidEndpoint, validateEndpoint("https://user:password@login.example/token"));
    try std.testing.expectError(error.InvalidEndpoint, validateEndpoint("https://login.example/token#fragment"));
    try std.testing.expectError(
        error.InvalidEndpoint,
        validateEndpoint("https://" ++ ("a" ** 256) ++ ".example/token"),
    );
}

test "converts a Minecraft session into launch authentication" {
    const auth = (Session{
        .profile = .{ .id = "0123456789abcdef0123456789abcdef", .name = "Player" },
        .access_token = "minecraft-token",
        .expires_in = 86400,
        .xuid = "1234",
        .client_id = "00001111-aaaa-2222-bbbb-3333cccc4444",
    }).launchAuth();

    try std.testing.expectEqualStrings("Player", auth.player_name);
    try std.testing.expectEqualStrings("0123456789abcdef0123456789abcdef", auth.uuid);
    try std.testing.expectEqualStrings("minecraft-token", auth.access_token);
    try std.testing.expectEqualStrings("1234", auth.xuid);
    try std.testing.expectEqualStrings("00001111-aaaa-2222-bbbb-3333cccc4444", auth.client_id);
    try std.testing.expectEqualStrings("msa", auth.user_type);
}

test "classifies actionable XSTS failures" {
    try std.testing.expectEqual(XstsFailure.xbox_account_missing, classifyXstsFailure(2148916233));
    try std.testing.expectEqual(XstsFailure.child_account_restricted, classifyXstsFailure(2148916238));
    try std.testing.expectEqual(XstsFailure.unknown, classifyXstsFailure(1));
}
