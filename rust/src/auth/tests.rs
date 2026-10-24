use super::*;
use std::collections::VecDeque;
struct Fake {
    responses: VecDeque<Result<Response, Error>>,
    seen: Vec<(Kind, String, Zeroizing<Vec<u8>>)>,
}
impl Fake {
    fn new(responses: &[(u16, &str)]) -> Self {
        Self {
            responses: responses
                .iter()
                .map(|(status, body)| {
                    Ok(Response {
                        status: *status,
                        body: Zeroizing::new(body.as_bytes().to_vec()),
                    })
                })
                .collect(),
            seen: vec![],
        }
    }
}
impl Transport for Fake {
    fn request(&mut self, r: Request<'_>) -> Result<Response, Error> {
        assert_eq!(r.max_response_bytes, 65536);
        assert_eq!(r.timeout_ms, 30000);
        self.seen
            .push((r.kind, r.url.to_owned(), Zeroizing::new(r.payload.to_vec())));
        self.responses.pop_front().expect("unexpected request")
    }
}
const OAUTH: &str = r#"{"token_type":"Bearer","expires_in":3600,"access_token":"MS-secret","refresh_token":"rotated-secret"}"#;
const XBOX_OK: &str =
    r#"{"Token":"xbox-secret","DisplayClaims":{"xui":[{"uhs":"hash","xid":"xbox-id"}]}}"#;
const XSTS_OK: &str =
    r#"{"Token":"xsts-secret","DisplayClaims":{"xui":[{"uhs":"hash","xid":"xsts-id"}]}}"#;
const LOGIN_OK: &str = r#"{"access_token":"mc-secret","token_type":"Bearer","expires_in":86400}"#;
const OWNED: &str = r#"{"items":[{"name":"game_minecraft"}]}"#;
const PLAYER: &str = r#"{"id":"0123456789ABCDEF0123456789abcdef","name":"Player_1"}"#;
fn secret(s: &str) -> Secret {
    join(&[s])
}
#[test]
fn full_exchange() {
    let mut f = Fake::new(&[
        (200, XBOX_OK),
        (200, XSTS_OK),
        (200, LOGIN_OK),
        (200, OWNED),
        (200, PLAYER),
    ]);
    let s = exchange(&mut f, "client", "MS-secret").unwrap();
    assert_eq!(s.access_token.expose(), "mc-secret");
    assert_eq!(s.xuid.expose(), "xsts-id");
    assert_eq!(s.uuid.expose(), "0123456789ABCDEF0123456789abcdef");
    assert_eq!(f.seen[0].1, XBOX);
    assert!(
        std::str::from_utf8(&f.seen[0].2)
            .unwrap()
            .contains(r#""RpsTicket":"d=MS-secret""#)
    );
    assert_eq!(f.seen[3].2.as_slice(), b"mc-secret");
    assert!(f.responses.is_empty());
}
#[test]
fn refresh_rotation_failure_is_a_result() {
    let mut f = Fake::new(&[(200, OAUTH), (401, "secret diagnostics")]);
    let v = refresh_exchange(&mut f, "client", secret("old-secret")).unwrap();
    let output = json_body(&v);
    let s = std::str::from_utf8(output.bytes()).unwrap();
    assert_eq!(
        s,
        r#"{"state":"exchange_failed","refresh_token":"rotated-secret","exchange_error":{"code":"XBOX_AUTH_FAILED"}}"#
    );
    assert!(!s.contains("MS-secret"));
}
#[test]
fn successful_refresh_exchange() {
    let mut f = Fake::new(&[
        (200, OAUTH),
        (200, XBOX_OK),
        (200, XSTS_OK),
        (200, LOGIN_OK),
        (200, OWNED),
        (200, PLAYER),
    ]);
    let v = refresh_exchange(&mut f, "client", secret("old-secret")).unwrap();
    let output = json_body(&v);
    let s = std::str::from_utf8(output.bytes()).unwrap();
    assert!(s.contains("rotated-secret"));
    assert!(s.contains("mc-secret"));
    assert!(!s.contains("MS-secret"));
}
#[test]
fn refresh_carries_old_token() {
    let mut f = Fake::new(&[(
        200,
        r#"{"token_type":"bEaReR","expires_in":"3600","access_token":"a"}"#,
    )]);
    let c = refresh(&mut f, "client", secret("old")).unwrap();
    assert_eq!(c.refresh_token.expose(), "old");
    assert_eq!(c.token_type.expose(), "bEaReR");
    assert_eq!(&*f.seen[0].2,b"client_id=client&grant_type=refresh_token&refresh_token=old&scope=XboxLive.signin+offline_access");
}
#[test]
fn device_begin_and_poll() {
    let mut f = Fake::new(&[
        (
            200,
            r#"{"device_code":"d","user_code":"u","verification_uri":"https://microsoft.com/link","expires_in":900}"#,
        ),
        (400, r#"{"error":"authorization_pending"}"#),
        (400, r#"{"error":"slow_down"}"#),
        (200, OAUTH),
    ]);
    let d = begin(&mut f, "a b", Some("zh-CN")).unwrap();
    assert_eq!(d.interval, 5);
    assert_eq!(f.seen[0].1, format!("{DEVICE}?mkt=zh-CN"));
    assert_eq!(
        &*f.seen[0].2,
        b"client_id=a+b&scope=XboxLive.signin+offline_access"
    );
    assert!(matches!(poll(&mut f, "c", "d").unwrap(), Poll::Pending));
    assert!(matches!(poll(&mut f, "c", "d").unwrap(), Poll::SlowDown));
    assert!(matches!(
        poll(&mut f, "c", "d").unwrap(),
        Poll::Authenticated { .. }
    ));
}
#[test]
fn xsts_restrictions() {
    for (code, expected) in [
        (2148916227u64, Error::XboxAccountBanned),
        (2148916233, Error::XboxAccountRequired),
        (2148916235, Error::XboxRegionUnsupported),
        (2148916236, Error::XboxAdultVerificationRequired),
        (2148916237, Error::XboxAdultVerificationRequired),
        (2148916238, Error::XboxChildAccountRestricted),
        (1, Error::XboxAuthFailed),
    ] {
        let response = format!(r#"{{"XErr":{code},"Message":"sensitive"}}"#);
        let mut f = Fake::new(&[(200, XBOX_OK), (401, &response)]);
        assert!(matches!(exchange(&mut f,"c","a"),Err(e) if e==expected));
    }
}
#[test]
fn rpc_escaped_secrets_and_errors() {
    let p = serde_json::from_str(r#"{"client_id":"c","refresh_token":"old\u002dsecret"}"#).unwrap();
    let mut f = Fake::new(&[(200, OAUTH), (401, "sensitive diagnostic")]);
    let mut out = WipingBuffer::new();
    handle_with_transport(
        &mut out,
        "id",
        "auth.minecraft.refresh_exchange",
        Some(p),
        &mut f,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        std::str::from_utf8(out.bytes()).unwrap(),
        "{\"protocol\":1,\"id\":\"id\",\"event\":\"result\",\"result\":{\"stage\":\"refresh_exchange\",\"state\":\"exchange_failed\",\"refresh_token\":\"rotated-secret\",\"exchange_error\":{\"code\":\"XBOX_AUTH_FAILED\"}}}\n"
    );
    assert!(
        std::str::from_utf8(&f.seen[0].2)
            .unwrap()
            .contains("refresh_token=old-secret")
    );
}
#[test]
fn status_ownership_profile_and_transport_failures() {
    for (status, body, expected) in [
        (429, "", Error::RateLimited),
        (401, "", Error::MinecraftAccessDenied),
        (500, "", Error::MinecraftService),
        (200, r#"{"items":[]}"#, Error::GameNotOwned),
    ] {
        let mut f = Fake::new(&[
            (200, XBOX_OK),
            (200, XSTS_OK),
            (200, LOGIN_OK),
            (status, body),
        ]);
        assert!(matches!(exchange(&mut f,"c","a"),Err(e) if e==expected));
    }
    let mut f = Fake::new(&[
        (200, XBOX_OK),
        (200, XSTS_OK),
        (200, LOGIN_OK),
        (200, OWNED),
        (404, ""),
    ]);
    assert!(matches!(
        exchange(&mut f, "c", "a"),
        Err(Error::ProfileNotFound)
    ));
    for e in [
        Error::Canceled,
        Error::Network,
        Error::Timeout,
        Error::ResponseTooLarge,
    ] {
        let mut f = Fake::new(&[]);
        f.responses.push_back(Err(e));
        assert!(matches!(refresh(&mut f,"c",secret("r")),Err(actual) if actual==e));
    }
}
#[test]
fn malformed_secret_drops_and_limits() {
    let mut f = Fake::new(&[(
        200,
        r#"{"token_type":"Bearer","access_token":"secret\uD800","expires_in":1,"refresh_token":"r"}"#,
    )]);
    assert!(matches!(
        refresh(&mut f, "c", secret("r")),
        Err(Error::InvalidResponse)
    ));
    let mut f = Fake::new(&[]);
    f.responses.push_back(Ok(Response {
        status: 200,
        body: Zeroizing::new(vec![b'x'; 65537]),
    }));
    assert!(matches!(
        begin(&mut f, "c", None),
        Err(Error::ResponseTooLarge)
    ));
    assert!(matches!(
        begin(&mut f, "c", Some("bad_locale")),
        Err(Error::InvalidParams)
    ));
}

#[test]
fn device_error_classification_and_nullable_interval() {
    for (status, body, expected) in [
        (
            400,
            r#"{"error":"authorization_declined"}"#,
            Error::AuthorizationDeclined,
        ),
        (
            400,
            r#"{"error":"access_denied"}"#,
            Error::AuthorizationDeclined,
        ),
        (
            400,
            r#"{"error":"expired_token"}"#,
            Error::DeviceCodeExpired,
        ),
        (
            400,
            r#"{"error":"other","error_description":"private"}"#,
            Error::OAuthRejected,
        ),
        (400, r#"{"error":""}"#, Error::InvalidResponse),
        (
            401,
            r#"{"error":"authorization_pending"}"#,
            Error::OAuthRejected,
        ),
        (429, "private", Error::RateLimited),
        (503, "private", Error::OAuthService),
    ] {
        let mut f = Fake::new(&[(status, body)]);
        assert!(matches!(poll(&mut f, "client", "code"), Err(e) if e==expected));
    }
    let mut f = Fake::new(&[(
        200,
        r#"{"device_code":"d","user_code":"u","verification_uri":"https://example.com./p#f","expires_in":1,"interval":null}"#,
    )]);
    assert_eq!(begin(&mut f, "c", None).unwrap().interval, 5);
    for uri in [
        "http://example.com",
        "HTTPS://example.com",
        "https://user@example.com",
        "https://-bad.example",
        "https://a..example",
        "https://[::1]",
        "https://example.com:65536",
    ] {
        assert!(!https(uri), "{uri}");
    }
    for uri in [
        "https://example.com",
        "https://example.com:443/p?q#f",
        "https://%65xample.com/",
    ] {
        assert!(https(uri), "{uri}");
    }
}
#[test]
fn invalid_rpc_params_never_touch_transport() {
    let methods = [
        "auth.microsoft.device.begin",
        "auth.microsoft.device.poll",
        "auth.microsoft.refresh",
        "auth.minecraft.exchange",
        "auth.minecraft.refresh_exchange",
    ];
    for method in methods {
        for raw in ["null", "{}", r#"{"client_id":2}"#, r#"{"client_id":""}"#] {
            let p = serde_json::from_str(raw).unwrap();
            let mut f = Fake::new(&[]);
            let mut output = WipingBuffer::new();
            handle_with_transport(&mut output, "request", method, Some(p), &mut f)
                .unwrap()
                .unwrap();
            let line = std::str::from_utf8(output.bytes()).unwrap();
            assert!(line.contains("INVALID_PARAMS"));
            assert!(!line.contains("NOT_IMPLEMENTED"));
            assert!(f.seen.is_empty());
        }
    }
}
#[test]
fn missing_refresh_token_and_malformed_later_claim_are_rejected() {
    let mut f = Fake::new(&[(
        200,
        r#"{"token_type":"Bearer","expires_in":1,"access_token":"a"}"#,
    )]);
    assert!(matches!(
        poll(&mut f, "c", "d"),
        Err(Error::InvalidResponse)
    ));
    let mut f = Fake::new(&[(
        200,
        r#"{"Token":"x","DisplayClaims":{"xui":[{"uhs":"hash"},{"uhs":7}]}}"#,
    )]);
    assert!(matches!(
        exchange(&mut f, "c", "a"),
        Err(Error::InvalidResponse)
    ));
    let mut f = Fake::new(&[
        (200, XBOX_OK),
        (200, XSTS_OK),
        (200, LOGIN_OK),
        (200, r#"{"items":[{"name":""}]}"#),
    ]);
    assert!(matches!(
        exchange(&mut f, "c", "a"),
        Err(Error::GameNotOwned)
    ));
}
