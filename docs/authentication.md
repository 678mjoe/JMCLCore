# Microsoft Authentication Design

## Scope

JMCL Core will support Microsoft device-code authentication for Minecraft: Java
Edition. The core is an OAuth public client: it accepts an application client ID
from its caller and never accepts or embeds a client secret.

Authentication has four layers:

1. Microsoft OAuth device authorization and refresh.
2. Xbox Live user authentication.
3. XSTS authorization for Minecraft Services.
4. Minecraft login, entitlement verification, and profile retrieval.

Third-party account systems and browser password collection are outside this
module. JMCL Core must never ask for, receive, or store a Microsoft password.

## Application Configuration

The caller supplies the Microsoft Entra Application (client) ID on every
device-code or refresh operation. Client IDs are treated as bounded opaque
OAuth strings and are validated only for basic transport safety. This keeps the
LGPL core reusable by multiple launchers without assigning those launchers the
identity of a JMCL-owned application.

The following values are intentionally fixed:

- Tenant: `consumers`.
- Scope: `XboxLive.signin offline_access`.
- Xbox sandbox: `RETAIL`.
- Minecraft relying party: `rp://api.minecraftservices.com/`.
- Production endpoints: Microsoft, Xbox Live, and Minecraft Services HTTPS
  endpoints exported by `src/auth/authentication.zig`.

An optional locale is allowed for Microsoft's device-code instructions. The
RPC API will not expose endpoint overrides. Library-level implementations may
inject endpoints or an HTTP transport for deterministic tests. Endpoint
overrides are still required to use HTTPS with a non-empty host and no URI
fragment.

The application registration must enable public-client/device-code flows. New
applications may also require Microsoft approval to use Minecraft Services;
the launcher distributor, not JMCL Core, is responsible for that registration
and approval.

## Ownership Boundaries

JMCL Core performs exchanges and validates responses, but it does not persist
accounts in ordinary files. The caller owns secure credential storage.

- The Microsoft refresh token is the durable credential.
- Microsoft access tokens, Xbox tokens, XSTS tokens, and device codes are
  transient.
- The Minecraft access token and profile form a launch session.
- A rotated refresh token replaces the previous refresh token atomically in the
  caller's secure store.
- No token, device code, or authorization header may appear in diagnostics,
  progress events, argv diagnostics, or errors.

Library operations return arena-owned result wrappers with explicit `deinit()`
methods. Values such as `Session.launchAuth()` are borrowed views and remain
valid only until their owning authentication result is deinitialized.
Known secret fields, temporary forms, and returned HTTP response buffers are
overwritten before release. This is best-effort hygiene rather than secure
memory: Zig's allocator, TLS implementation, and HTTP client may retain internal
copies that JMCL cannot reliably erase.
Authentication result and parsing arenas use a wiping allocator so every arena
block is cleared even when JSON parsing or a later allocation fails.

The standalone RPC necessarily transports credentials between the core and its
caller. The caller must treat stdin/stdout as a secret-bearing IPC channel and
must not record raw authentication request or result lines.

## Proposed RPC

Protocol version 1 processes one request at a time and has no cancellation. A
single long-running login call would block the session for up to the device-code
expiry period. Authentication is therefore split into short, stateless calls.

### `auth.microsoft.device.begin`

Input:

```json
{"client_id":"00001111-aaaa-2222-bbbb-3333cccc4444","locale":"zh-CN"}
```

Result fields are `device_code`, `user_code`, `verification_uri`, optional
`verification_uri_complete`, `expires_in`, normalized `interval`, and optional
localized `message`. A missing interval is normalized to five seconds as
required by RFC 8628. The caller displays only the user-facing URI, code, and
message. It retains the device code in memory until completion or expiry.

### `auth.microsoft.device.poll`

Input contains the same `client_id` and the short-lived `device_code`. Each RPC
request performs exactly one token request. The result is one of:

- `authorization_pending`: retry no sooner than the current interval.
- `slow_down`: increase the interval by five seconds for this and every later
  poll in the flow.
- `authenticated`: return Microsoft access and refresh tokens, scope, and
  lifetime.

`authorization_declined` and an expired device code are terminal RPC errors,
not pending result states. Network timeouts also require reducing the polling
frequency as specified by RFC 8628.
Only HTTP 400 responses can produce polling states. HTTP 401 is an OAuth
rejection, and both Microsoft's `authorization_declined` and RFC 8628's
`access_denied` map to `AUTHORIZATION_DECLINED`.

Separating OAuth completion from Minecraft exchange ensures an Xbox or
Minecraft service outage does not destroy a newly issued refresh token.

### `auth.microsoft.refresh`

Input contains `client_id` and `refresh_token`. The result has the same
`OAuthCredential` shape as an authenticated device poll and is bound to that
client ID. If Microsoft omits a replacement refresh token, the core copies the
input token into the owned result. If Microsoft rotates it, the caller must
persist the replacement before discarding the old value.

### `auth.minecraft.exchange`

Input is the access-only view returned by `OAuthCredential.access()`, containing
the client ID and Microsoft access token. It never contains the durable refresh
token. Standalone RPC callers are responsible for preserving the client ID and
token pairing returned by OAuth; the core cannot cryptographically prove that
relationship. The core performs Xbox Live authentication, XSTS authorization,
Minecraft login, entitlement checking, and profile retrieval. The result is a
`Session` containing:

- Minecraft profile ID and player name.
- Minecraft access token and lifetime.
- XUID when supplied by Xbox claims.
- The caller's client ID for `${clientid}` launch substitution.

`Session.launchAuth()` converts this directly to the existing launch-plan
authentication structure.

The exchange performs exactly five requests. Xbox user authentication receives
the Microsoft token as an RPS ticket prefixed with `d=`. XSTS uses sandbox
`RETAIL` and the Minecraft Services relying party. Minecraft login receives an
identity token built from the XSTS token and user hash. Entitlement and profile
requests use the resulting Minecraft access token as a Bearer credential.
Intermediate Xbox and XSTS tokens are cleared immediately after their dependent
request completes and are never included in the returned session.

## Errors

Errors must distinguish user action from transport and server failures. Planned
stable categories are:

| Category | Meaning |
| --- | --- |
| `INVALID_PARAMS` | Authentication input, including client ID, is missing or malformed. |
| `AUTH_NETWORK_ERROR` | A request could not reach the selected official service. |
| `AUTH_CANCELED` | The caller canceled an in-flight authentication request. |
| `AUTH_RATE_LIMITED` | A service requested backoff. |
| `AUTH_TIMEOUT` | An authentication service did not respond before the deadline. |
| `AUTH_INVALID_RESPONSE` | A response was malformed or exceeded its size limit. |
| `AUTH_SERVICE_ERROR` | Microsoft OAuth returned an unexpected status. |
| `AUTHORIZATION_DECLINED` | The user declined device authorization. |
| `DEVICE_CODE_EXPIRED` | The device flow expired. |
| `OAUTH_REJECTED` | Microsoft rejected a token request for another reason. |
| `XBOX_ACCOUNT_REQUIRED` | The Microsoft account has no Xbox profile. |
| `XBOX_ACCOUNT_BANNED` | Xbox enforcement blocked the account. |
| `XBOX_REGION_UNSUPPORTED` | Xbox Live is unavailable for the account region. |
| `XBOX_ADULT_VERIFICATION_REQUIRED` | Xbox requires adult verification. |
| `XBOX_CHILD_ACCOUNT_RESTRICTED` | Xbox child account requires family approval. |
| `XBOX_AUTH_FAILED` | Xbox rejected authentication for an unknown reason. |
| `MINECRAFT_ACCESS_DENIED` | Minecraft Services rejected the Xbox identity. |
| `MINECRAFT_SERVICE_ERROR` | Minecraft Services returned an unexpected status. |
| `GAME_NOT_OWNED` | No Java Edition entitlement was returned. |
| `PROFILE_NOT_FOUND` | Java Edition ownership exists but no profile is available. |
| `AUTH_ERROR` | An uncategorized authentication failure occurred. |

Known XSTS numeric errors are converted to semantic internal values by
`classifyXstsFailure`; unknown values map to `XBOX_AUTH_FAILED` rather than
being mislabeled as a user restriction or leaking token-bearing response
bodies.

## Response Validation

Authentication uses a dedicated request primitive rather than the metadata
download helper. It returns both HTTP status and bounded response body because
OAuth polling states use HTTP 400 and XSTS details use HTTP 401. Requests have
explicit timeouts, do not follow redirects, and support an injected transport
for tests. Required strings must be non-empty and token lengths bounded. Tokens
are opaque strings and are never decoded as JWTs.

The standard transport races each request against a 30-second Zig I/O timer and
cancels and drains the losing task. Cancellation from a parent operation remains
distinct from timeout and network failure.

Entitlement verification accepts `product_minecraft`, `game_minecraft`,
`product_game_pass_pc`, or `product_game_pass_ultimate`, then requires a valid
Java profile. Bedrock-only entries are not sufficient. Profile IDs must be 32
hexadecimal characters and names must be valid Minecraft-style ASCII names. The
implementation initially trusts the authenticated HTTPS response and recognized
item names; signature verification can be added independently without changing
the public session model.

## Implementation Stages

1. Add shared models, configuration validation, endpoint definitions, XSTS
   error classification, and launch-session conversion.
2. Implement URL-encoded OAuth requests with an injectable HTTP transport.
3. Implement Xbox, XSTS, and Minecraft JSON exchanges with bounded parsing.
4. Add the authentication RPC methods and secret-aware protocol tests.
5. Add interactive CLI login diagnostics without credential persistence.
6. Verify a real approved client ID on Windows, Linux, and macOS.

The protocol method names and result JSON remain proposed until stages 2 and 3
have executable tests. Adding them earlier would expose an API that cannot yet
complete a login.

Stages 1 through 4 are implemented. Device authorization, one-shot polling,
token refresh, refresh-and-exchange, Xbox/XSTS/Minecraft exchange, form and
JSON encoding, bounded non-2xx responses, timeout handling, token rotation,
entitlement/profile validation, account registry operations, allocation cleanup,
and secret-aware RPC methods have deterministic tests. Secure credential storage
and real approved-client validation remain caller and release tasks.

## References

- Microsoft OAuth 2.0 device authorization grant:
  <https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code>
- OAuth 2.0 Device Authorization Grant (RFC 8628):
  <https://www.rfc-editor.org/rfc/rfc8628>
- Minecraft Services authentication sequence overview:
  <https://minecraft.wiki/w/Microsoft_authentication>
