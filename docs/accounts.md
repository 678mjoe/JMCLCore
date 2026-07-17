# Account Registry

JMCL Core stores public Microsoft account metadata under `./accounts` by
default. The registry is for account selection and display only. It never
stores a Microsoft access token, refresh token, Minecraft access token, device
code, or token expiry.

```text
accounts/
  0123456789abcdef0123456789abcdef.json
```

Each JSON file is atomically replaced and contains a schema version, Minecraft
profile UUID, player name, public Microsoft client ID, and optional XUID. The
profile UUID is a lowercase 32-hex-character filename and account ID. Invalid
or corrupted files are ignored by listing and cannot be deleted through the
managed API unless their ID and manifest agree.

Persisted client IDs use the 36-character UUID format assigned by Microsoft
Entra app registrations. This intentionally prevents a token or device code
from being accidentally saved in the public `client_id` field.

The default `accounts/` directory is ignored by JMCL Core's own Git checkout.

## Credential Ownership

The GUI or another RPC caller owns durable credentials. Store each refresh token
in the platform secret store, keyed at least by the public client ID and
Minecraft profile UUID. Suitable backends include Windows Credential Manager,
macOS Keychain, and a Linux Secret Service implementation.

The core's expected flow is:

1. Complete device authorization and persist the returned refresh token in the
   caller's secret store.
2. Exchange the Microsoft access token for a Minecraft session.
3. Save the public session identity with `account.microsoft.save`, mapping the
   session's `uuid` to account `id` and `player_name` directly.
4. Before a launch, load the refresh token from the caller's secret store and
   call `auth.minecraft.refresh_exchange`. Atomically save its returned rotated
   refresh token even when its exchange state is `exchange_failed`; only pass a
   session to the launch RPC when the state is `authenticated`.
5. On account removal, delete the refresh token from the caller's secret store
   and call `account.microsoft.delete` to remove public metadata.

The caller must atomically replace a rotated refresh token before discarding
its previous value. JMCL Core does not read ordinary configuration files for
credentials and does not provide a plaintext token-store fallback. If an IPC
process or pipe fails before the caller receives a complete terminal result,
refresh-token rotation state is unknowable; recover the account with device
authorization if the previously stored token no longer works.

## RPC

All account registry methods accept optional `directory`, which defaults to
`accounts`. It must be a single lowercase/uppercase alphanumeric, `_`, or `-`
directory name beneath the current working directory; absolute paths, path
separators, and parent traversal are rejected. Their results contain only public
metadata.

`account.microsoft.save` creates or atomically updates one account:

```json
{"protocol":1,"id":"account-1","method":"account.microsoft.save","params":{"account":{"id":"0123456789abcdef0123456789abcdef","player_name":"Player","client_id":"00001111-aaaa-2222-bbbb-3333cccc4444","xuid":"123"}}}
```

`account.microsoft.list` returns valid account metadata sorted by UUID:

```json
{"protocol":1,"id":"account-2","method":"account.microsoft.list"}
```

`account.microsoft.get` and `account.microsoft.delete` require an account ID:

```json
{"protocol":1,"id":"account-3","method":"account.microsoft.get","params":{"id":"0123456789abcdef0123456789abcdef"}}
{"protocol":1,"id":"account-4","method":"account.microsoft.delete","params":{"id":"0123456789abcdef0123456789abcdef"}}
```

Errors include `ACCOUNT_NOT_FOUND`, `ACCOUNT_LIMIT_REACHED`, and
`ACCOUNT_STORE_ERROR`. Invalid public metadata returns `INVALID_PARAMS`.
