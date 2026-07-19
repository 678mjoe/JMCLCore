const std = @import("std");
const core = @import("../root.zig");
const protocol = @import("protocol.zig");

const writeError = protocol.writeError;
const writeEventHeader = protocol.writeEventHeader;
const WipingAllocator = protocol.WipingAllocator;

const MicrosoftDeviceBeginParams = core.authentication.Config;

const MicrosoftDevicePollParams = struct {
    client_id: []const u8,
    device_code: []const u8,
};

const MicrosoftRefreshParams = struct {
    client_id: []const u8,
    refresh_token: []const u8,
};

const MinecraftExchangeParams = core.authentication.MicrosoftAccess;

const AccountDirectoryParams = struct {
    directory: []const u8 = core.account_store.default_root,
};

const AccountSaveParams = struct {
    directory: []const u8 = core.account_store.default_root,
    account: core.account_store.Account,
};

const AccountTargetParams = struct {
    directory: []const u8 = core.account_store.default_root,
    id: []const u8,
};

pub fn writeMicrosoftDeviceBeginResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "auth.microsoft.device.begin requires a Microsoft client ID");
        return;
    };
    var params_allocator: WipingAllocator = .{ .child = service.allocator };
    var params = std.json.parseFromValue(MicrosoftDeviceBeginParams, params_allocator.allocator(), value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "Microsoft device authorization parameters are invalid");
        return;
    };
    defer params.deinit();

    var authorization = service.beginMicrosoftDeviceAuthorization(params.value) catch |err| {
        try writeAuthenticationError(writer, request_id, err);
        return;
    };
    defer authorization.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("device_authorization");
    try json.objectField("device_code");
    try json.write(authorization.value.device_code);
    try json.objectField("user_code");
    try json.write(authorization.value.user_code);
    try json.objectField("verification_uri");
    try json.write(authorization.value.verification_uri);
    try json.objectField("verification_uri_complete");
    try json.write(authorization.value.verification_uri_complete);
    try json.objectField("expires_in");
    try json.write(authorization.value.expires_in);
    try json.objectField("interval");
    try json.write(authorization.value.interval);
    try json.objectField("message");
    try json.write(authorization.value.message);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeMicrosoftDevicePollResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "auth.microsoft.device.poll requires a client ID and device code");
        return;
    };
    var params_allocator: WipingAllocator = .{ .child = service.allocator };
    var params = std.json.parseFromValue(MicrosoftDevicePollParams, params_allocator.allocator(), value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "Microsoft device polling parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.client_id.len == 0 or params.value.device_code.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "Microsoft client ID and device code must not be empty");
        return;
    }

    var result = service.pollMicrosoftDeviceAuthorization(
        params.value.client_id,
        params.value.device_code,
    ) catch |err| {
        try writeAuthenticationError(writer, request_id, err);
        return;
    };
    defer result.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("device_poll");
    switch (result.value) {
        .pending => |state| {
            try json.objectField("state");
            try json.write(@tagName(state));
        },
        .authenticated => |credential| {
            try json.objectField("state");
            try json.write("authenticated");
            try json.objectField("credential");
            try writeOAuthCredential(&json, credential);
        },
    }
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeMicrosoftRefreshResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "auth.microsoft.refresh requires a client ID and refresh token");
        return;
    };
    var params_allocator: WipingAllocator = .{ .child = service.allocator };
    var params = std.json.parseFromValue(MicrosoftRefreshParams, params_allocator.allocator(), value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "Microsoft refresh parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.client_id.len == 0 or params.value.refresh_token.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "Microsoft client ID and refresh token must not be empty");
        return;
    }

    var credential = service.refreshMicrosoftAuthorization(
        params.value.client_id,
        params.value.refresh_token,
    ) catch |err| {
        try writeAuthenticationError(writer, request_id, err);
        return;
    };
    defer credential.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("refresh");
    try json.objectField("credential");
    try writeOAuthCredential(&json, credential.value);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeMinecraftExchangeResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "auth.minecraft.exchange requires a client ID and Microsoft access token");
        return;
    };
    var params_allocator: WipingAllocator = .{ .child = service.allocator };
    var params = std.json.parseFromValue(MinecraftExchangeParams, params_allocator.allocator(), value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "Minecraft exchange parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.client_id.len == 0 or params.value.access_token.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "Microsoft client ID and access token must not be empty");
        return;
    }

    var session = service.exchangeMinecraftSession(params.value) catch |err| {
        try writeAuthenticationError(writer, request_id, err);
        return;
    };
    defer session.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("minecraft_exchange");
    try json.objectField("session");
    try writeMinecraftSession(&json, session.value);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeMinecraftRefreshExchangeResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "auth.minecraft.refresh_exchange requires a client ID and refresh token");
        return;
    };
    var params_allocator: WipingAllocator = .{ .child = service.allocator };
    var params = std.json.parseFromValue(MicrosoftRefreshParams, params_allocator.allocator(), value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "Minecraft refresh-exchange parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.client_id.len == 0 or params.value.refresh_token.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "Microsoft client ID and refresh token must not be empty");
        return;
    }

    var result = service.refreshMinecraftSession(params.value.client_id, params.value.refresh_token) catch |err| {
        try writeAuthenticationError(writer, request_id, err);
        return;
    };
    defer result.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("refresh_exchange");
    switch (result.value) {
        .session => |session_result| {
            try json.objectField("state");
            try json.write("authenticated");
            try json.objectField("refresh_token");
            try json.write(session_result.refresh_token);
            try json.objectField("session");
            try writeMinecraftSession(&json, session_result.session);
        },
        .exchange_failed => |failure_result| {
            try json.objectField("state");
            try json.write("exchange_failed");
            try json.objectField("refresh_token");
            try json.write(failure_result.refresh_token);
            try json.objectField("exchange_error");
            try json.beginObject();
            try json.objectField("code");
            try json.write(exchangeFailureCode(failure_result.failure));
            try json.endObject();
        },
    }
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn exchangeFailureCode(failure: core.authentication.ExchangeFailure) []const u8 {
    return switch (failure) {
        .canceled => "AUTH_CANCELED",
        .network_error => "AUTH_NETWORK_ERROR",
        .timeout => "AUTH_TIMEOUT",
        .invalid_response => "AUTH_INVALID_RESPONSE",
        .rate_limited => "AUTH_RATE_LIMITED",
        .xbox_account_banned => "XBOX_ACCOUNT_BANNED",
        .xbox_account_required => "XBOX_ACCOUNT_REQUIRED",
        .xbox_region_unsupported => "XBOX_REGION_UNSUPPORTED",
        .xbox_adult_verification_required => "XBOX_ADULT_VERIFICATION_REQUIRED",
        .xbox_child_account_restricted => "XBOX_CHILD_ACCOUNT_RESTRICTED",
        .xbox_auth_failed => "XBOX_AUTH_FAILED",
        .minecraft_access_denied => "MINECRAFT_ACCESS_DENIED",
        .minecraft_service_error => "MINECRAFT_SERVICE_ERROR",
        .game_not_owned => "GAME_NOT_OWNED",
        .profile_not_found => "PROFILE_NOT_FOUND",
        .unexpected => "AUTH_ERROR",
    };
}

fn writeOAuthCredential(
    json: *std.json.Stringify,
    credential: core.authentication.OAuthCredential,
) std.Io.Writer.Error!void {
    try json.beginObject();
    try json.objectField("client_id");
    try json.write(credential.client_id);
    try json.objectField("token_type");
    try json.write(credential.token_type);
    try json.objectField("scope");
    try json.write(credential.scope);
    try json.objectField("expires_in");
    try json.write(credential.expires_in);
    try json.objectField("access_token");
    try json.write(credential.access_token);
    try json.objectField("refresh_token");
    try json.write(credential.refresh_token);
    try json.endObject();
}

fn writeMinecraftSession(
    json: *std.json.Stringify,
    session: core.authentication.Session,
) std.Io.Writer.Error!void {
    const auth = session.launchAuth();
    try json.beginObject();
    try json.objectField("player_name");
    try json.write(auth.player_name);
    try json.objectField("uuid");
    try json.write(auth.uuid);
    try json.objectField("access_token");
    try json.write(auth.access_token);
    try json.objectField("user_type");
    try json.write(auth.user_type);
    try json.objectField("xuid");
    try json.write(auth.xuid);
    try json.objectField("client_id");
    try json.write(auth.client_id);
    try json.objectField("user_properties");
    try json.write(auth.user_properties);
    try json.objectField("expires_in");
    try json.write(session.expires_in);
    try json.endObject();
}

fn writeAuthenticationError(
    writer: *std.Io.Writer,
    request_id: []const u8,
    failure: anyerror,
) std.Io.Writer.Error!void {
    switch (failure) {
        error.InvalidClientId, error.InvalidLocale, error.InvalidDeviceCode, error.InvalidRefreshToken, error.InvalidAccessToken => try writeError(writer, request_id, "INVALID_PARAMS", "Authentication parameters are invalid"),
        error.Canceled => try writeError(writer, request_id, "AUTH_CANCELED", "Authentication was canceled"),
        error.NetworkError => try writeError(writer, request_id, "AUTH_NETWORK_ERROR", "Authentication service could not be reached"),
        error.Timeout => try writeError(writer, request_id, "AUTH_TIMEOUT", "Authentication service did not respond in time"),
        error.ResponseTooLarge, error.InvalidResponse, error.InvalidEndpoint => try writeError(writer, request_id, "AUTH_INVALID_RESPONSE", "Authentication service returned an invalid response"),
        error.RateLimited => try writeError(writer, request_id, "AUTH_RATE_LIMITED", "Authentication service requested backoff"),
        error.AuthorizationDeclined => try writeError(writer, request_id, "AUTHORIZATION_DECLINED", "Device authorization was declined"),
        error.DeviceCodeExpired => try writeError(writer, request_id, "DEVICE_CODE_EXPIRED", "Device authorization code expired"),
        error.OAuthRejected => try writeError(writer, request_id, "OAUTH_REJECTED", "Microsoft rejected the authentication request"),
        error.OAuthServiceError => try writeError(writer, request_id, "AUTH_SERVICE_ERROR", "Microsoft authentication service returned an unexpected status"),
        error.XboxAccountBanned => try writeError(writer, request_id, "XBOX_ACCOUNT_BANNED", "Xbox account is unavailable"),
        error.XboxAccountRequired => try writeError(writer, request_id, "XBOX_ACCOUNT_REQUIRED", "Microsoft account requires an Xbox profile"),
        error.XboxRegionUnsupported => try writeError(writer, request_id, "XBOX_REGION_UNSUPPORTED", "Xbox Live is unavailable for this account region"),
        error.XboxAdultVerificationRequired => try writeError(writer, request_id, "XBOX_ADULT_VERIFICATION_REQUIRED", "Xbox account requires adult verification"),
        error.XboxChildAccountRestricted => try writeError(writer, request_id, "XBOX_CHILD_ACCOUNT_RESTRICTED", "Xbox child account requires family approval"),
        error.XboxAuthFailed => try writeError(writer, request_id, "XBOX_AUTH_FAILED", "Xbox authentication failed"),
        error.MinecraftAccessDenied => try writeError(writer, request_id, "MINECRAFT_ACCESS_DENIED", "Minecraft Services rejected the Xbox identity"),
        error.MinecraftServiceError => try writeError(writer, request_id, "MINECRAFT_SERVICE_ERROR", "Minecraft Services returned an unexpected status"),
        error.GameNotOwned => try writeError(writer, request_id, "GAME_NOT_OWNED", "Minecraft Java Edition ownership was not found"),
        error.ProfileNotFound => try writeError(writer, request_id, "PROFILE_NOT_FOUND", "Minecraft Java Edition profile was not found"),
        else => try writeError(writer, request_id, "AUTH_ERROR", "Authentication failed"),
    }
}

pub fn writeAccountSaveResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "account.microsoft.save requires public account metadata");
        return;
    };
    var params = std.json.parseFromValue(AccountSaveParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "Account metadata parameters are invalid");
        return;
    };
    defer params.deinit();
    if (params.value.directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "Account directory must not be empty");
        return;
    }
    service.saveAccount(params.value.directory, params.value.account) catch |err| {
        try writeAccountStoreError(writer, request_id, err);
        return;
    };
    try writeAccountResult(writer, request_id, "save", params.value.account);
}

pub fn writeAccountListResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var params: AccountDirectoryParams = .{};
    var parsed_params: ?std.json.Parsed(AccountDirectoryParams) = null;
    defer if (parsed_params) |parsed| parsed.deinit();
    if (params_value) |value| {
        parsed_params = std.json.parseFromValue(AccountDirectoryParams, service.allocator, value, .{
            .ignore_unknown_fields = true,
        }) catch {
            try writeError(writer, request_id, "INVALID_PARAMS", "Account list parameters are invalid");
            return;
        };
        params = parsed_params.?.value;
    }
    if (params.directory.len == 0) {
        try writeError(writer, request_id, "INVALID_PARAMS", "Account directory must not be empty");
        return;
    }
    var accounts = service.listAccounts(params.directory) catch |err| {
        try writeAccountStoreError(writer, request_id, err);
        return;
    };
    defer accounts.deinit();

    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write("account_list");
    try json.objectField("accounts");
    try json.beginArray();
    for (accounts.items) |document| try json.write(document.value);
    try json.endArray();
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

pub fn writeAccountGetResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var parsed_params = (try parseAccountTargetParams(service, writer, request_id, params_value)) orelse return;
    defer parsed_params.deinit();
    const params = parsed_params.value;
    var account = service.loadAccount(params.directory, params.id) catch |err| {
        try writeAccountStoreError(writer, request_id, err);
        return;
    };
    defer account.deinit();
    try writeAccountResult(writer, request_id, "account_get", account.value);
}

pub fn writeAccountDeleteResult(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!void {
    var parsed_params = (try parseAccountTargetParams(service, writer, request_id, params_value)) orelse return;
    defer parsed_params.deinit();
    const params = parsed_params.value;
    service.deleteAccount(params.directory, params.id) catch |err| {
        try writeAccountStoreError(writer, request_id, err);
        return;
    };
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.write(.{ .stage = "account_delete", .id = params.id, .deleted = true });
    try json.endObject();
    try writer.writeByte('\n');
}

fn parseAccountTargetParams(
    service: *core.Service,
    writer: *std.Io.Writer,
    request_id: []const u8,
    params_value: ?std.json.Value,
) std.Io.Writer.Error!?std.json.Parsed(AccountTargetParams) {
    const value = params_value orelse {
        try writeError(writer, request_id, "INVALID_PARAMS", "Account operation requires an ID");
        return null;
    };
    var params = std.json.parseFromValue(AccountTargetParams, service.allocator, value, .{
        .ignore_unknown_fields = true,
    }) catch {
        try writeError(writer, request_id, "INVALID_PARAMS", "Account target parameters are invalid");
        return null;
    };
    if (params.value.directory.len == 0 or params.value.id.len == 0) {
        params.deinit();
        try writeError(writer, request_id, "INVALID_PARAMS", "Account directory and ID must not be empty");
        return null;
    }
    return params;
}

fn writeAccountResult(
    writer: *std.Io.Writer,
    request_id: []const u8,
    stage: []const u8,
    account: core.account_store.Account,
) std.Io.Writer.Error!void {
    var json: std.json.Stringify = .{ .writer = writer };
    try writeEventHeader(&json, request_id, "result");
    try json.objectField("result");
    try json.beginObject();
    try json.objectField("stage");
    try json.write(stage);
    try json.objectField("account");
    try json.write(account);
    try json.endObject();
    try json.endObject();
    try writer.writeByte('\n');
}

fn writeAccountStoreError(
    writer: *std.Io.Writer,
    request_id: []const u8,
    failure: anyerror,
) std.Io.Writer.Error!void {
    switch (failure) {
        error.EmptyAccountsRoot, error.InvalidAccountsRoot, error.InvalidAccountId, error.InvalidPlayerName, error.InvalidClientId, error.InvalidXuid, error.UnsupportedAccountSchema => try writeError(writer, request_id, "INVALID_PARAMS", "Account metadata is invalid"),
        error.FileNotFound => try writeError(writer, request_id, "ACCOUNT_NOT_FOUND", "Account metadata was not found"),
        error.AccountManifestTooLarge, error.AccountManifestInvalid, error.AccountIdMismatch => try writeError(writer, request_id, "ACCOUNT_STORE_ERROR", "Stored account metadata is invalid"),
        error.TooManyAccounts => try writeError(writer, request_id, "ACCOUNT_LIMIT_REACHED", "Account registry has too many entries"),
        else => try writeError(writer, request_id, "ACCOUNT_STORE_ERROR", "Account registry operation failed"),
    }
}

test "authentication serializers contain only their explicit sensitive result fields" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var json: std.json.Stringify = .{ .writer = &output.writer };
    try writeOAuthCredential(&json, .{
        .client_id = "client",
        .token_type = "Bearer",
        .scope = "scope",
        .expires_in = 3600,
        .access_token = "access-secret",
        .refresh_token = "refresh-secret",
    });
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "access-secret") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "refresh-secret") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "device-secret") == null);
}

test "Minecraft session serializer is launch auth compatible" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    var json: std.json.Stringify = .{ .writer = &output.writer };
    try writeMinecraftSession(&json, .{
        .profile = .{ .id = "0123456789abcdef0123456789abcdef", .name = "Player" },
        .access_token = "minecraft-secret",
        .expires_in = 86400,
        .xuid = "123",
        .client_id = "client",
    });
    var parsed = try std.json.parseFromSlice(core.launch_plan.Auth, std.testing.allocator, output.written(), .{
        .ignore_unknown_fields = true,
    });
    defer parsed.deinit();
    try std.testing.expectEqualStrings("Player", parsed.value.player_name);
    try std.testing.expectEqualStrings("0123456789abcdef0123456789abcdef", parsed.value.uuid);
    try std.testing.expectEqualStrings("minecraft-secret", parsed.value.access_token);
    try std.testing.expectEqualStrings("msa", parsed.value.user_type);
    try std.testing.expectEqualStrings("123", parsed.value.xuid);
    try std.testing.expectEqualStrings("client", parsed.value.client_id);
}

test "authentication error codes match the documented categories" {
    const cases = [_]struct {
        failure: anyerror,
        code: []const u8,
    }{
        .{ .failure = error.InvalidClientId, .code = "INVALID_PARAMS" },
        .{ .failure = error.XboxAccountBanned, .code = "XBOX_ACCOUNT_BANNED" },
        .{ .failure = error.XboxRegionUnsupported, .code = "XBOX_REGION_UNSUPPORTED" },
        .{ .failure = error.XboxAdultVerificationRequired, .code = "XBOX_ADULT_VERIFICATION_REQUIRED" },
        .{ .failure = error.XboxChildAccountRestricted, .code = "XBOX_CHILD_ACCOUNT_RESTRICTED" },
    };
    for (cases) |case| {
        var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
        defer output.deinit();
        try writeAuthenticationError(&output.writer, "auth-test", case.failure);
        try std.testing.expect(std.mem.indexOf(u8, output.written(), case.code) != null);
    }
}

test "account serializer contains only public metadata" {
    var output: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer output.deinit();
    try writeAccountResult(&output.writer, "account-2", "save", .{
        .id = "0123456789abcdef0123456789abcdef",
        .player_name = "Player",
        .client_id = "client",
        .xuid = "123",
    });
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "Player") != null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "access_token") == null);
    try std.testing.expect(std.mem.indexOf(u8, output.written(), "refresh_token") == null);
}
