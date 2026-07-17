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
