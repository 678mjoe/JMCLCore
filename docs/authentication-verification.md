# Authentication Verification

Real Microsoft authentication requires a distributor-owned Microsoft Entra
public-client registration that is approved for Minecraft Services. JMCL Core
does not ship a client ID or secret.

## Preconditions

- Register a public client with Microsoft Entra and enable device-code flow.
- Obtain Minecraft Services approval for the application where required.
- Use a Microsoft account that owns Minecraft: Java Edition or has an eligible
  PC/Ultimate Game Pass entitlement.
- Run tests from an IPC client that keeps raw JSON Lines requests and results
  out of terminal history, log files, telemetry, and crash reports.

## Flow

1. Send `auth.microsoft.device.begin` with the public client ID.
2. Display only `user_code`, `verification_uri`, and optional `message` to the
   user. Keep `device_code` private and in memory.
3. Poll with `auth.microsoft.device.poll` at the returned interval, applying
   the RFC 8628 slow-down adjustment when requested.
4. Store the returned refresh token in the OS secret store under the client ID
   and returned Minecraft profile UUID after the initial exchange.
5. Save the public profile with `account.microsoft.save`.
6. For every later launch, read the refresh token from the secret store and
   call `auth.minecraft.refresh_exchange`.
7. Atomically replace the secret-store token with the returned refresh token
   before launching the returned session through `launch.execute`.

## Platform Matrix

Perform the complete flow and a subsequent token-refresh launch on each target:

| Platform | Credential store expected in GUI | Required checks |
| --- | --- | --- |
| Windows | Credential Manager | Device login, restart, refresh, authenticated multiplayer launch. |
| macOS | Keychain | Device login, restart, refresh, authenticated multiplayer launch. |
| Linux | Secret Service | Device login, restart, refresh, authenticated multiplayer launch. |

Verify these negative cases on at least one platform:

- Declined or expired device authorization.
- Account without Xbox profile.
- Java entitlement absent.
- Missing Java profile.
- Rotated refresh token survives a process restart.

Record only fixed RPC error codes and high-level outcome. Never record device
codes, refresh tokens, Microsoft access tokens, Minecraft access tokens, or
raw service responses.

If the IPC process or result pipe fails after a refresh request starts, do not
assume the locally stored refresh token remains valid. Retry it only through the
normal flow; if rejected, recover with device authorization. A core without an
OS-backed secret store cannot make token rotation atomic across a caller crash.

## Account-free HTTP adapter regression checks

`zig build test` includes production `StdHttpTransport` tests using only a
synthetic HTTP fixture on loopback. They cover numeric loopback addressing,
`localhost` resolution, successful form POST response handling and timer
cancellation, and timeout/cancellation of a stalled response. The fixture uses
cancellable I/O so a failing client does not leave an accept task running.
It supplies no proxy environment and contacts no Microsoft or Minecraft service.

These checks do not exercise TLS, the operating system trust store, external
DNS/proxies, a packaged GUI, or real device authorization. They cannot establish
that a particular user's Windows login failure is fixed. A frontend revision
alone also does not identify a manually supplied Core sidecar.

Initial verification on Linux x86_64 with Zig 0.16.0, based on Core
`5a4310debbcee26b3d2391bab2339f79cd6ec52e`: `zig build test --summary all`
reported 319 passing tests and one skipped test (320 total). The focused adapter
tests also compile for `x86_64-windows`; compilation is not a Windows execution
result. Native Windows execution remains necessary.
