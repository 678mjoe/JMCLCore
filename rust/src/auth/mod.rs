//! Caller-owned, stateless Microsoft/Minecraft authentication.
//! Tests use scripted transports only. Credentials never enter a generic JSON
//! tree or Debug/error text; owned decode, request and response buffers wipe.
pub mod http;
pub(crate) mod json;
use json::Json;
use serde::Serialize;
use std::io::{self, Write};
use zeroize::Zeroizing;
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;
pub const TIMEOUT_MS: u64 = 30_000;
const DEVICE: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBOX: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const LOGIN: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const ENTITLEMENTS: &str = "https://api.minecraftservices.com/entitlements/mcstore";
const PROFILE: &str = "https://api.minecraftservices.com/minecraft/profile";
const SCOPE: &str = "XboxLive.signin offline_access";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidParams,
    Canceled,
    Network,
    Timeout,
    ResponseTooLarge,
    InvalidResponse,
    RateLimited,
    AuthorizationDeclined,
    DeviceCodeExpired,
    OAuthRejected,
    OAuthService,
    XboxAccountBanned,
    XboxAccountRequired,
    XboxRegionUnsupported,
    XboxAdultVerificationRequired,
    XboxChildAccountRestricted,
    XboxAuthFailed,
    MinecraftAccessDenied,
    MinecraftService,
    GameNotOwned,
    ProfileNotFound,
}
impl From<()> for Error {
    fn from(_: ()) -> Self {
        Self::InvalidResponse
    }
}
impl Error {
    pub fn wire(self) -> (&'static str, &'static str) {
        match self {
            Self::InvalidParams => ("INVALID_PARAMS", "Authentication parameters are invalid"),
            Self::Canceled => ("AUTH_CANCELED", "Authentication was canceled"),
            Self::Network => (
                "AUTH_NETWORK_ERROR",
                "Authentication service could not be reached",
            ),
            Self::Timeout => (
                "AUTH_TIMEOUT",
                "Authentication service did not respond in time",
            ),
            Self::ResponseTooLarge | Self::InvalidResponse => (
                "AUTH_INVALID_RESPONSE",
                "Authentication service returned an invalid response",
            ),
            Self::RateLimited => (
                "AUTH_RATE_LIMITED",
                "Authentication service requested backoff",
            ),
            Self::AuthorizationDeclined => (
                "AUTHORIZATION_DECLINED",
                "Device authorization was declined",
            ),
            Self::DeviceCodeExpired => ("DEVICE_CODE_EXPIRED", "Device authorization code expired"),
            Self::OAuthRejected => (
                "OAUTH_REJECTED",
                "Microsoft rejected the authentication request",
            ),
            Self::OAuthService => (
                "AUTH_SERVICE_ERROR",
                "Microsoft authentication service returned an unexpected status",
            ),
            Self::XboxAccountBanned => ("XBOX_ACCOUNT_BANNED", "Xbox account is unavailable"),
            Self::XboxAccountRequired => (
                "XBOX_ACCOUNT_REQUIRED",
                "Microsoft account requires an Xbox profile",
            ),
            Self::XboxRegionUnsupported => (
                "XBOX_REGION_UNSUPPORTED",
                "Xbox Live is unavailable for this account region",
            ),
            Self::XboxAdultVerificationRequired => (
                "XBOX_ADULT_VERIFICATION_REQUIRED",
                "Xbox account requires adult verification",
            ),
            Self::XboxChildAccountRestricted => (
                "XBOX_CHILD_ACCOUNT_RESTRICTED",
                "Xbox child account requires family approval",
            ),
            Self::XboxAuthFailed => ("XBOX_AUTH_FAILED", "Xbox authentication failed"),
            Self::MinecraftAccessDenied => (
                "MINECRAFT_ACCESS_DENIED",
                "Minecraft Services rejected the Xbox identity",
            ),
            Self::MinecraftService => (
                "MINECRAFT_SERVICE_ERROR",
                "Minecraft Services returned an unexpected status",
            ),
            Self::GameNotOwned => (
                "GAME_NOT_OWNED",
                "Minecraft Java Edition ownership was not found",
            ),
            Self::ProfileNotFound => (
                "PROFILE_NOT_FOUND",
                "Minecraft Java Edition profile was not found",
            ),
        }
    }
}
/// Intentionally has neither Debug nor Clone.
pub struct Secret(Zeroizing<String>);
impl Secret {
    fn new(s: Zeroizing<String>) -> Self {
        Self(s)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}
/// Growth replaces and wipes the old allocation instead of reallocating secrets.
pub struct WipingBuffer(Zeroizing<Vec<u8>>);
impl WipingBuffer {
    pub fn new() -> Self {
        Self(Zeroizing::new(Vec::new()))
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}
impl Default for WipingBuffer {
    fn default() -> Self {
        Self::new()
    }
}
impl Write for WipingBuffer {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.0.capacity() - self.0.len() < b.len() {
            let mut next = Zeroizing::new(Vec::with_capacity(
                (self.0.len() + b.len()).max(self.0.capacity().saturating_mul(2)),
            ));
            next.extend_from_slice(&self.0);
            self.0 = next;
        }
        self.0.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    FormPost,
    JsonPost,
    BearerGet,
}
/// No Debug: body and bearer may contain credentials.
pub struct Request<'a> {
    pub kind: Kind,
    pub url: &'a str,
    pub payload: &'a [u8],
    pub max_response_bytes: usize,
    pub timeout_ms: u64,
}
pub struct Response {
    pub status: u16,
    pub body: Zeroizing<Vec<u8>>,
}
pub trait Transport {
    fn request(&mut self, request: Request<'_>) -> Result<Response, Error>;
}
fn request(
    t: &mut impl Transport,
    kind: Kind,
    url: &str,
    payload: &[u8],
) -> Result<Response, Error> {
    let r = t.request(Request {
        kind,
        url,
        payload,
        max_response_bytes: MAX_RESPONSE_BYTES,
        timeout_ms: TIMEOUT_MS,
    })?;
    if r.body.len() > MAX_RESPONSE_BYTES {
        return Err(Error::ResponseTooLarge);
    }
    Ok(r)
}
fn parsed(r: &Response) -> Result<Json<'_>, Error> {
    Ok(Json::parse(
        std::str::from_utf8(&r.body).map_err(|_| Error::InvalidResponse)?,
    )?)
}
fn text(j: Json<'_>, key: &str) -> Result<Secret, Error> {
    let s = j.required(key)?.string()?;
    if s.is_empty() || s.len() > MAX_RESPONSE_BYTES {
        return Err(Error::InvalidResponse);
    }
    Ok(Secret::new(s))
}
fn optional(j: Json<'_>, key: &str) -> Result<Option<Secret>, Error> {
    j.field(key)?
        .filter(|v| !v.null())
        .map(|v| v.string().map(Secret::new).map_err(Error::from))
        .transpose()
}
fn positive(j: Json<'_>, key: &str) -> Result<u32, Error> {
    let n = j.required(key)?.u32()?;
    if n == 0 {
        Err(Error::InvalidResponse)
    } else {
        Ok(n)
    }
}
fn validate_client(s: &str) -> Result<(), Error> {
    if s.is_empty() || s.len() > 256 || s.bytes().any(|c| c < 32 || c == 127) {
        Err(Error::InvalidParams)
    } else {
        Ok(())
    }
}
fn validate_opaque(s: &str) -> Result<(), Error> {
    if s.is_empty() || s.len() > MAX_RESPONSE_BYTES {
        Err(Error::InvalidParams)
    } else {
        Ok(())
    }
}
fn form(fields: &[(&str, &str)]) -> WipingBuffer {
    let mut out = WipingBuffer::new();
    for (i, (k, v)) in fields.iter().enumerate() {
        if i > 0 {
            out.write_all(b"&").unwrap();
        }
        out.write_all(k.as_bytes()).unwrap();
        out.write_all(b"=").unwrap();
        for c in v.bytes() {
            match c {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.write_all(&[c]).unwrap()
                }
                b' ' => out.write_all(b"+").unwrap(),
                _ => {
                    const HEX: &[u8] = b"0123456789ABCDEF";
                    out.write_all(&[b'%', HEX[(c >> 4) as usize], HEX[(c & 15) as usize]])
                        .unwrap();
                }
            }
        }
    }
    out
}
fn json_body(value: &impl Serialize) -> WipingBuffer {
    let mut out = WipingBuffer::new();
    serde_json::to_writer(&mut out, value).expect("in-memory serialization");
    out
}
// Zig alloc_if_needed checks max_value_len only when an escape requires an
// allocated decoding buffer; unescaped source slices bypass that limit.
fn limited_string(j: Json<'_>, key: &str, limit: usize) -> Result<Secret, Error> {
    let raw = j.required(key)?;
    let s = raw.string()?;
    if raw.0.as_bytes().contains(&b'\\') && s.len() > limit {
        return Err(Error::InvalidResponse);
    }
    Ok(Secret::new(s))
}
fn general_failure(r: &Response) -> Error {
    match r.status {
        429 => Error::RateLimited,
        400 | 401 => match parsed(r).and_then(|j| limited_string(j, "error", 128)) {
            Ok(e) if !e.expose().is_empty() => Error::OAuthRejected,
            _ => Error::InvalidResponse,
        },
        _ => Error::OAuthService,
    }
}
#[derive(Serialize)]
pub struct Credential {
    client_id: Secret,
    token_type: Secret,
    scope: Option<Secret>,
    expires_in: u32,
    access_token: Secret,
    refresh_token: Secret,
}
fn credential(r: &Response, client: &str, fallback: Option<Secret>) -> Result<Credential, Error> {
    let j = parsed(r)?;
    let token_type = text(j, "token_type")?;
    if !token_type.expose().eq_ignore_ascii_case("Bearer") {
        return Err(Error::InvalidResponse);
    }
    let refresh_token = match optional(j, "refresh_token")? {
        Some(s) => {
            if s.expose().is_empty() {
                return Err(Error::InvalidResponse);
            }
            s
        }
        None => fallback.ok_or(Error::InvalidResponse)?,
    };
    Ok(Credential {
        client_id: Secret::new(Zeroizing::new(client.to_owned())),
        token_type,
        scope: optional(j, "scope")?,
        expires_in: positive(j, "expires_in")?,
        access_token: text(j, "access_token")?,
        refresh_token,
    })
}
#[derive(Serialize)]
pub struct Device {
    device_code: Secret,
    user_code: Secret,
    verification_uri: Secret,
    verification_uri_complete: Option<Secret>,
    expires_in: u32,
    interval: u32,
    message: Option<Secret>,
}
fn https(s: &str) -> bool {
    if s.len() > 4096 {
        return false;
    }
    let Some(rest) = s.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return false;
    }
    let host = if let Some((host, port)) = authority.rsplit_once(':') {
        if port.parse::<u16>().is_err() {
            return false;
        }
        host
    } else {
        authority
    };
    let mut decoded = Zeroizing::new(Vec::with_capacity(host.len()));
    let bytes = host.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let a = (bytes[i + 1] as char).to_digit(16);
            let b = (bytes[i + 2] as char).to_digit(16);
            if let (Some(a), Some(b)) = (a, b) {
                decoded.push((a * 16 + b) as u8);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    let host = decoded.strip_suffix(b".").unwrap_or(&decoded);
    if host.is_empty() || host.len() > 255 {
        return false;
    }
    host.split(|b| *b == b'.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label[0].is_ascii_alphanumeric()
            && label[label.len() - 1].is_ascii_alphanumeric()
            && label
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
    })
}
pub fn begin(t: &mut impl Transport, client: &str, locale: Option<&str>) -> Result<Device, Error> {
    validate_client(client)?;
    if let Some(l) = locale
        && (l.is_empty()
            || l.len() > 35
            || l.starts_with('-')
            || l.ends_with('-')
            || !l.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'))
    {
        return Err(Error::InvalidParams);
    }
    let url = locale.map(|l| format!("{DEVICE}?mkt={l}"));
    let body = form(&[("client_id", client), ("scope", SCOPE)]);
    let r = request(
        t,
        Kind::FormPost,
        url.as_deref().unwrap_or(DEVICE),
        body.bytes(),
    )?;
    if r.status != 200 {
        return Err(general_failure(&r));
    }
    let j = parsed(&r)?;
    let uri = text(j, "verification_uri")?;
    let complete = optional(j, "verification_uri_complete")?;
    if !https(uri.expose()) || complete.as_ref().is_some_and(|s| !https(s.expose())) {
        return Err(Error::InvalidResponse);
    }
    let interval = match j.field("interval")? {
        None => 5,
        Some(v) if v.null() => 5,
        Some(v) => {
            let n = v.u32()?;
            if n == 0 {
                return Err(Error::InvalidResponse);
            }
            n
        }
    };
    Ok(Device {
        device_code: text(j, "device_code")?,
        user_code: text(j, "user_code")?,
        verification_uri: uri,
        verification_uri_complete: complete,
        expires_in: positive(j, "expires_in")?,
        interval,
        message: optional(j, "message")?,
    })
}
#[derive(Serialize)]
#[serde(tag = "state")]
pub enum Poll {
    #[serde(rename = "authorization_pending")]
    Pending,
    #[serde(rename = "slow_down")]
    SlowDown,
    #[serde(rename = "authenticated")]
    Authenticated { credential: Credential },
}
pub fn poll(t: &mut impl Transport, client: &str, code: &str) -> Result<Poll, Error> {
    validate_client(client)?;
    validate_opaque(code)?;
    let body = form(&[
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("client_id", client),
        ("device_code", code),
    ]);
    let r = request(t, Kind::FormPost, TOKEN, body.bytes())?;
    match r.status {
        200 => Ok(Poll::Authenticated {
            credential: credential(&r, client, None)?,
        }),
        429 => Err(Error::RateLimited),
        400 => {
            let e = limited_string(parsed(&r)?, "error", 128)?;
            if e.expose().is_empty() {
                return Err(Error::InvalidResponse);
            }
            match e.expose() {
                "authorization_pending" => Ok(Poll::Pending),
                "slow_down" => Ok(Poll::SlowDown),
                "authorization_declined" | "access_denied" => Err(Error::AuthorizationDeclined),
                "expired_token" => Err(Error::DeviceCodeExpired),
                _ => Err(Error::OAuthRejected),
            }
        }
        401 => Err(general_failure(&r)),
        _ => Err(Error::OAuthService),
    }
}
pub fn refresh(t: &mut impl Transport, client: &str, token: Secret) -> Result<Credential, Error> {
    validate_client(client)?;
    validate_opaque(token.expose())?;
    let body = form(&[
        ("client_id", client),
        ("grant_type", "refresh_token"),
        ("refresh_token", token.expose()),
        ("scope", SCOPE),
    ]);
    let r = request(t, Kind::FormPost, TOKEN, body.bytes())?;
    if r.status != 200 {
        return Err(general_failure(&r));
    }
    credential(&r, client, Some(token))
}
struct Identity {
    token: Secret,
    uhs: Secret,
    xid: Option<Secret>,
}
fn identity(r: &Response) -> Result<Identity, Error> {
    let j = parsed(r)?;
    let claims = j.required("DisplayClaims")?.required("xui")?.array()?;
    let claim = *claims.first().ok_or(Error::InvalidResponse)?;
    // Typed Zig parsing also validates the shape of unused claims.
    for extra in claims.iter().skip(1) {
        let _ = extra.required("uhs")?.string()?;
        let _ = optional(*extra, "xid")?;
    }
    let xid = optional(claim, "xid")?;
    if xid.as_ref().is_some_and(|s| s.expose().is_empty()) {
        return Err(Error::InvalidResponse);
    }
    Ok(Identity {
        token: text(j, "Token")?,
        uhs: text(claim, "uhs")?,
        xid,
    })
}
fn xsts_error(r: &Response) -> Error {
    match parsed(r).and_then(|j| Ok(j.required("XErr")?.u64()?)) {
        Ok(2148916227) => Error::XboxAccountBanned,
        Ok(2148916233) => Error::XboxAccountRequired,
        Ok(2148916235) => Error::XboxRegionUnsupported,
        Ok(2148916236 | 2148916237) => Error::XboxAdultVerificationRequired,
        Ok(2148916238) => Error::XboxChildAccountRestricted,
        _ => Error::XboxAuthFailed,
    }
}
fn mc_status(r: &Response, profile: bool) -> Result<(), Error> {
    match r.status {
        200 => Ok(()),
        429 => Err(Error::RateLimited),
        404 if profile => Err(Error::ProfileNotFound),
        401 | 403 => Err(Error::MinecraftAccessDenied),
        _ => Err(Error::MinecraftService),
    }
}
fn join(parts: &[&str]) -> Secret {
    let mut s = Zeroizing::new(String::with_capacity(parts.iter().map(|s| s.len()).sum()));
    for part in parts {
        s.push_str(part);
    }
    Secret::new(s)
}
#[derive(Serialize)]
pub struct Session {
    player_name: Secret,
    uuid: Secret,
    access_token: Secret,
    user_type: &'static str,
    xuid: Secret,
    client_id: Secret,
    user_properties: &'static str,
    expires_in: u32,
}
pub fn exchange(t: &mut impl Transport, client: &str, access: &str) -> Result<Session, Error> {
    validate_client(client)?;
    validate_opaque(access)?;
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct XboxProperties<'a> {
        auth_method: &'a str,
        site_name: &'a str,
        rps_ticket: &'a Secret,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct XboxRequest<P> {
        properties: P,
        relying_party: &'static str,
        token_type: &'static str,
    }
    let ticket = join(&["d=", access]);
    let body = json_body(&XboxRequest {
        properties: XboxProperties {
            auth_method: "RPS",
            site_name: "user.auth.xboxlive.com",
            rps_ticket: &ticket,
        },
        relying_party: "http://auth.xboxlive.com",
        token_type: "JWT",
    });
    let r = request(t, Kind::JsonPost, XBOX, body.bytes())?;
    if r.status == 429 {
        return Err(Error::RateLimited);
    }
    if r.status != 200 {
        return Err(Error::XboxAuthFailed);
    }
    let xbox = identity(&r)?;
    drop(r);
    drop(body);
    drop(ticket);
    #[derive(Serialize)]
    #[serde(rename_all = "PascalCase")]
    struct XstsProperties<'a> {
        sandbox_id: &'static str,
        user_tokens: [&'a Secret; 1],
    }
    let body = json_body(&XboxRequest {
        properties: XstsProperties {
            sandbox_id: "RETAIL",
            user_tokens: [&xbox.token],
        },
        relying_party: "rp://api.minecraftservices.com/",
        token_type: "JWT",
    });
    let r = request(t, Kind::JsonPost, XSTS, body.bytes())?;
    if r.status == 429 {
        return Err(Error::RateLimited);
    }
    if r.status == 400 || r.status == 401 {
        return Err(xsts_error(&r));
    }
    if r.status != 200 {
        return Err(Error::XboxAuthFailed);
    }
    let xsts = identity(&r)?;
    drop(r);
    drop(body);
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Login<'a> {
        identity_token: &'a Secret,
    }
    let identity_token = join(&["XBL3.0 x=", xsts.uhs.expose(), ";", xsts.token.expose()]);
    let body = json_body(&Login {
        identity_token: &identity_token,
    });
    let r = request(t, Kind::JsonPost, LOGIN, body.bytes())?;
    mc_status(&r, false)?;
    let j = parsed(&r)?;
    let access_token = text(j, "access_token")?;
    if !text(j, "token_type")?
        .expose()
        .eq_ignore_ascii_case("Bearer")
        || !access_token
            .expose()
            .bytes()
            .all(|b| (0x21..=0x7e).contains(&b))
    {
        return Err(Error::InvalidResponse);
    }
    let expires_in = positive(j, "expires_in")?;
    drop(r);
    drop(body);
    drop(identity_token);
    let r = request(
        t,
        Kind::BearerGet,
        ENTITLEMENTS,
        access_token.expose().as_bytes(),
    )?;
    mc_status(&r, false)?;
    let items = parsed(&r)?.required("items")?.array()?;
    let mut owned = false;
    for item in items {
        let name = limited_string(item, "name", 256)?;
        owned |= matches!(
            name.expose(),
            "product_minecraft"
                | "game_minecraft"
                | "product_game_pass_pc"
                | "product_game_pass_ultimate"
        );
    }
    if !owned {
        return Err(Error::GameNotOwned);
    }
    drop(r);
    let r = request(
        t,
        Kind::BearerGet,
        PROFILE,
        access_token.expose().as_bytes(),
    )?;
    mc_status(&r, true)?;
    let j = parsed(&r)?;
    let uuid = text(j, "id")?;
    let player_name = text(j, "name")?;
    if uuid.expose().len() != 32
        || !uuid.expose().bytes().all(|b| b.is_ascii_hexdigit())
        || player_name.expose().len() > 16
        || !player_name
            .expose()
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::InvalidResponse);
    }
    Ok(Session {
        player_name,
        uuid,
        access_token,
        user_type: "msa",
        xuid: xsts.xid.or(xbox.xid).unwrap_or_else(|| join(&[""])),
        client_id: join(&[client]),
        user_properties: "{}",
        expires_in,
    })
}
#[derive(Serialize)]
pub struct ExchangeError {
    code: &'static str,
}
#[derive(Serialize)]
#[serde(tag = "state")]
pub enum RefreshExchange {
    #[serde(rename = "authenticated")]
    Authenticated {
        refresh_token: Secret,
        session: Session,
    },
    #[serde(rename = "exchange_failed")]
    Failed {
        refresh_token: Secret,
        exchange_error: ExchangeError,
    },
}
pub fn refresh_exchange(
    t: &mut impl Transport,
    client: &str,
    token: Secret,
) -> Result<RefreshExchange, Error> {
    let credential = refresh(t, client, token)?;
    let session = exchange(t, client, credential.access_token.expose());
    let refresh_token = credential.refresh_token;
    Ok(match session {
        Ok(session) => RefreshExchange::Authenticated {
            refresh_token,
            session,
        },
        Err(e) => RefreshExchange::Failed {
            refresh_token,
            exchange_error: ExchangeError { code: e.wire().0 },
        },
    })
}
/// Serialize a terminal response directly; no generic Value containing tokens.
fn result(out: &mut impl Write, id: &str, stage: &str, data: &impl Serialize) -> io::Result<()> {
    #[derive(Serialize)]
    struct Envelope<'a, T> {
        protocol: u32,
        id: &'a str,
        event: &'static str,
        result: Staged<'a, T>,
    }
    #[derive(Serialize)]
    struct Staged<'a, T> {
        stage: &'a str,
        #[serde(flatten)]
        data: T,
    }
    serde_json::to_writer(
        &mut *out,
        &Envelope {
            protocol: 1,
            id,
            event: "result",
            result: Staged { stage, data },
        },
    )?;
    out.write_all(b"\n")
}
// Request strings use dynamic JSON byte-element coercion. Response schemas
// keep their existing decoders. Allocate once before writing decoded bytes so
// secret-bearing data is never stranded in a superseded Vec allocation.
fn request_text(j: Json<'_>) -> Result<Secret, Error> {
    if j.0.starts_with('[') {
        let items = j.array()?;
        let mut bytes = Zeroizing::new(Vec::with_capacity(items.len()));
        for item in items {
            bytes.push(
                crate::json_number::dynamic_raw(item.0, 8).map_err(|_| Error::InvalidResponse)?
                    as u8,
            );
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| Error::InvalidResponse)?;
        Ok(Secret::new(Zeroizing::new(text.to_owned())))
    } else {
        j.string().map(Secret::new).map_err(Error::from)
    }
}
type DecodedRpcParams = (Secret, Option<Secret>, Option<Secret>);
fn parse_rpc_params(
    params: &serde_json::value::RawValue,
    secret_field: Option<&str>,
    stage: &str,
) -> Result<DecodedRpcParams, Error> {
    let j = Json::parse(params.get())?;
    let client = request_text(j.required("client_id")?)?;
    let secret = secret_field
        .map(|key| request_text(j.required(key)?))
        .transpose()?;
    let locale = if stage == "device_authorization" {
        j.field("locale")?
            .filter(|v| !v.null())
            .map(request_text)
            .transpose()?
    } else {
        None
    };
    Ok((client, secret, locale))
}

/// Recognized methods return Some, allowing centralized dispatch to continue for others.
/// The caller supplies a transport; this module never initiates a live login itself.
pub fn handle_with_transport(
    out: &mut impl Write,
    id: &str,
    method: &str,
    params: Option<&serde_json::value::RawValue>,
    t: &mut impl Transport,
) -> Option<io::Result<()>> {
    let (stage, missing, invalid, secret_field) = match method {
        "auth.microsoft.device.begin" => (
            "device_authorization",
            "auth.microsoft.device.begin requires a Microsoft client ID",
            "Microsoft device authorization parameters are invalid",
            None,
        ),
        "auth.microsoft.device.poll" => (
            "device_poll",
            "auth.microsoft.device.poll requires a client ID and device code",
            "Microsoft device polling parameters are invalid",
            Some("device_code"),
        ),
        "auth.microsoft.refresh" => (
            "refresh",
            "auth.microsoft.refresh requires a client ID and refresh token",
            "Microsoft refresh parameters are invalid",
            Some("refresh_token"),
        ),
        "auth.minecraft.exchange" => (
            "minecraft_exchange",
            "auth.minecraft.exchange requires a client ID and Microsoft access token",
            "Minecraft exchange parameters are invalid",
            Some("access_token"),
        ),
        "auth.minecraft.refresh_exchange" => (
            "refresh_exchange",
            "auth.minecraft.refresh_exchange requires a client ID and refresh token",
            "Minecraft refresh-exchange parameters are invalid",
            Some("refresh_token"),
        ),
        _ => return None,
    };
    Some((|| {
        let Some(params) = params else {
            return crate::protocol::error(out, Some(id), "INVALID_PARAMS", missing);
        };
        let parsed_params = parse_rpc_params(params, secret_field, stage);
        let (client, secret, locale) = match parsed_params {
            Ok(v) => v,
            Err(_) => return crate::protocol::error(out, Some(id), "INVALID_PARAMS", invalid),
        };
        if let Some(s) = &secret
            && (client.expose().is_empty() || s.expose().is_empty())
        {
            let message = match secret_field.unwrap() {
                "device_code" => "Microsoft client ID and device code must not be empty",
                "access_token" => "Microsoft client ID and access token must not be empty",
                _ => "Microsoft client ID and refresh token must not be empty",
            };
            return crate::protocol::error(out, Some(id), "INVALID_PARAMS", message);
        }
        #[derive(Serialize)]
        struct CredentialResult {
            credential: Credential,
        }
        #[derive(Serialize)]
        struct SessionResult {
            session: Session,
        }
        let outcome = match stage {
            "device_authorization" => {
                begin(t, client.expose(), locale.as_ref().map(Secret::expose))
                    .map(|v| result(out, id, stage, &v))
            }
            "device_poll" => poll(t, client.expose(), secret.as_ref().unwrap().expose())
                .map(|v| result(out, id, stage, &v)),
            "refresh" => refresh(t, client.expose(), secret.unwrap())
                .map(|credential| result(out, id, stage, &CredentialResult { credential })),
            "minecraft_exchange" => exchange(t, client.expose(), secret.as_ref().unwrap().expose())
                .map(|session| result(out, id, stage, &SessionResult { session })),
            _ => refresh_exchange(t, client.expose(), secret.unwrap())
                .map(|v| result(out, id, stage, &v)),
        };
        match outcome {
            Ok(v) => v,
            Err(e) => {
                let (code, message) = e.wire();
                crate::protocol::error(out, Some(id), code, message)
            }
        }
    })())
}
#[cfg(test)]
mod tests;

/// Production binding. Constructing the adapter does not start authentication;
/// only a recognized valid RPC method can invoke its official HTTPS endpoints.
pub fn handle(
    out: &mut impl Write,
    id: &str,
    method: &str,
    params: Option<&serde_json::value::RawValue>,
) -> Option<io::Result<()>> {
    handle_with_transport(out, id, method, params, &mut http::HttpsTransport)
}

#[cfg(test)]
#[path = "../../tests/support/auth_rpc_params.rs"]
mod request_param_tests;
