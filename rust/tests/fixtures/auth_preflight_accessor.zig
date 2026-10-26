
// Pure fixture accessor: only original input validators; no OAuth method,
// transport, endpoint construction, allocator or credential operation.
pub fn __fixtureValidateRequest(mode: u8, client: []const u8, secondary: ?[]const u8) !void {
    try validateClientId(client);
    switch (mode) {
        0 => { if (secondary) |locale| try validateLocale(locale); },
        1 => try validateOpaqueInput(secondary.?, error.InvalidDeviceCode),
        3 => try validateOpaqueInput(secondary.?, error.InvalidAccessToken),
        2, 4 => try validateOpaqueInput(secondary.?, error.InvalidRefreshToken),
        else => unreachable,
    }
}
