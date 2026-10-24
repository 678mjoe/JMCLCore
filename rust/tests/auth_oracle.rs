//! Fake credentials only; both executables use injected non-network transports.
use jmcl_core_rust::auth::{self, Error, Request, Response, Transport};
use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};
use std::collections::VecDeque;
use std::process::Command;
use zeroize::Zeroizing;
struct Fake {
    steps: VecDeque<Value>,
    captured: Vec<Value>,
}
impl Transport for Fake {
    fn request(&mut self, r: Request<'_>) -> Result<Response, Error> {
        self.captured.push(json!({"kind":format!("{:?}",r.kind),"url":r.url,"payload":std::str::from_utf8(r.payload).unwrap(),"max_response_bytes":r.max_response_bytes,"timeout_ms":r.timeout_ms}));
        let step = self
            .steps
            .pop_front()
            .expect("unexpected synthetic HTTP request");
        if let Some(f) = step.get("failure").and_then(Value::as_str) {
            return Err(match f {
                "Canceled" => Error::Canceled,
                "Timeout" => Error::Timeout,
                "ResponseTooLarge" => Error::ResponseTooLarge,
                "InvalidResponse" => Error::InvalidResponse,
                _ => Error::Network,
            });
        }
        Ok(Response {
            status: step.get("status").and_then(Value::as_u64).unwrap_or(200) as u16,
            body: Zeroizing::new(
                step.get("body")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .as_bytes()
                    .to_vec(),
            ),
        })
    }
}
fn same(method: &str, params: Option<&str>, steps: Vec<Value>) {
    let request = format!(
        r#"{{"protocol":1,"id":"fixture","method":"{method}"{}}}"#,
        params
            .map(|p| format!(",\"params\":{p}"))
            .unwrap_or_default()
    );
    let fixture = json!({"request":request,"responses":steps});
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), serde_json::to_vec(&fixture).unwrap()).unwrap();
    let oracle = std::env::var("JMCL_AUTH_ORACLE")
        .expect("Build rust/compat/build_auth_oracle.py and set JMCL_AUTH_ORACLE");
    let output = Command::new(oracle).arg(file.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    #[derive(Deserialize)]
    struct Input<'a> {
        #[serde(borrow)]
        params: Option<&'a RawValue>,
    }
    let input: Input = serde_json::from_str(&request).unwrap();
    let mut fake = Fake {
        steps: steps.into(),
        captured: vec![],
    };
    let mut out = auth::WipingBuffer::new();
    auth::handle_with_transport(&mut out, "fixture", method, input.params, &mut fake)
        .unwrap()
        .unwrap();
    let actual = json!({"response":serde_json::from_slice::<Value>(out.bytes()).unwrap(),"requests":fake.captured});
    assert_eq!(
        actual, expected,
        "method={method}, params={params:?}, fixture={fixture}"
    );
}
fn step(status: u16, body: &str) -> Value {
    json!({"status":status,"body":body})
}
const CLIENT: &str = r#"{"client_id":"synthetic-client","access_token":"synthetic-ms"}"#;
const REFRESH: &str = r#"{"client_id":"synthetic-client","refresh_token":"synthetic-old"}"#;
const POLL: &str = r#"{"client_id":"synthetic-client","device_code":"synthetic-code"}"#;
const OAUTH: &str = r#"{"token_type":"Bearer","scope":"x","expires_in":3600,"access_token":"synthetic-ms","refresh_token":"synthetic-rotated"}"#;
const XBOX: &str =
    r#"{"Token":"synthetic-xbox","DisplayClaims":{"xui":[{"uhs":"hash","xid":"111"}]}}"#;
const XSTS: &str =
    r#"{"Token":"synthetic-xsts","DisplayClaims":{"xui":[{"uhs":"hash","xid":"222"}]}}"#;
const MC: &str = r#"{"access_token":"synthetic-mc","token_type":"Bearer","expires_in":10}"#;
const OWNED: &str = r#"{"items":[{"name":"game_minecraft"}]}"#;
const PROFILE: &str = r#"{"id":"0123456789ABCDEF0123456789abcdef","name":"Player_1"}"#;
fn chain() -> Vec<Value> {
    [XBOX, XSTS, MC, OWNED, PROFILE]
        .into_iter()
        .map(|s| step(200, s))
        .collect()
}
#[test]
#[ignore = "requires pinned injected Zig oracle"]
fn all_five_success_and_rotation_recovery() {
    same(
        "auth.microsoft.device.begin",
        Some(r#"{"client_id":"synthetic-client","locale":"zh-CN"}"#),
        vec![step(
            200,
            r#"{"device_code":"synthetic-device","user_code":"CODE","verification_uri":"https://microsoft.com/link","expires_in":900,"interval":null}"#,
        )],
    );
    same(
        "auth.microsoft.device.poll",
        Some(POLL),
        vec![step(200, OAUTH)],
    );
    same(
        "auth.microsoft.refresh",
        Some(REFRESH),
        vec![step(200, OAUTH)],
    );
    same(
        "auth.microsoft.refresh",
        Some(REFRESH),
        vec![step(
            200,
            r#"{"token_type":"bearer","expires_in":"3_600","access_token":"synthetic-ms"}"#,
        )],
    );
    same("auth.minecraft.exchange", Some(CLIENT), chain());
    let mut steps = vec![step(200, OAUTH)];
    steps.extend(chain());
    same("auth.minecraft.refresh_exchange", Some(REFRESH), steps);
    for n in 0..5 {
        let mut steps = vec![step(200, OAUTH)];
        steps.extend(chain().into_iter().take(n));
        steps.push(json!({"failure":"Timeout"}));
        same("auth.minecraft.refresh_exchange", Some(REFRESH), steps);
    }
    same(
        "auth.minecraft.refresh_exchange",
        Some(REFRESH),
        vec![step(400, r#"{"error":"invalid_grant"}"#)],
    );
}
#[test]
#[ignore = "requires pinned injected Zig oracle"]
fn malformed_and_escaped_params() {
    for method in [
        "auth.microsoft.device.begin",
        "auth.microsoft.device.poll",
        "auth.microsoft.refresh",
        "auth.minecraft.exchange",
        "auth.minecraft.refresh_exchange",
    ] {
        for p in [
            None,
            Some("null"),
            Some("[]"),
            Some("{}"),
            Some(r#"{"client_id":4}"#),
            Some(r#"{"client_id":"","access_token":"","device_code":"","refresh_token":""}"#),
            Some(
                r#"{"client_id":"c\u0000","access_token":"x","device_code":"x","refresh_token":"x"}"#,
            ),
        ] {
            same(method, p, vec![]);
        }
    }
    same(
        "auth.microsoft.refresh",
        Some(
            r#"{"client_id":"synthetic\u002dclient","refresh_token":"synthetic\u002dold\n\uD83D\uDE00"}"#,
        ),
        vec![step(200, OAUTH)],
    );
    same(
        "auth.minecraft.exchange",
        Some(r#"{"client_id":"synthetic-client","access_token":"synthetic\"\\\n"}"#),
        chain(),
    );
}
#[test]
#[ignore = "requires pinned injected Zig oracle"]
fn error_chains_and_invalid_wire_shapes() {
    for name in [
        "authorization_pending",
        "slow_down",
        "authorization_declined",
        "access_denied",
        "expired_token",
        "other",
        "",
    ] {
        same(
            "auth.microsoft.device.poll",
            Some(POLL),
            vec![step(400, &json!({"error":name}).to_string())],
        );
    }
    for status in [401, 403, 404, 429, 500] {
        same(
            "auth.microsoft.refresh",
            Some(REFRESH),
            vec![step(
                status,
                r#"{"error":"invalid_grant","private":"synthetic-detail"}"#,
            )],
        );
    }
    for failure in [
        "Canceled",
        "NetworkError",
        "Timeout",
        "ResponseTooLarge",
        "InvalidResponse",
    ] {
        same(
            "auth.microsoft.device.poll",
            Some(POLL),
            vec![json!({"failure":failure})],
        );
    }
    for code in [
        2148916227u64,
        2148916233,
        2148916235,
        2148916236,
        2148916237,
        2148916238,
        1,
    ] {
        same(
            "auth.minecraft.exchange",
            Some(CLIENT),
            vec![
                step(200, XBOX),
                step(
                    401,
                    &json!({"XErr":code,"Message":"synthetic-detail"}).to_string(),
                ),
            ],
        );
    }
    for (index, body) in [
        (0, "{}"),
        (0, r#"{"Token":"x","DisplayClaims":{"xui":[]}}"#),
        (
            0,
            r#"{"Token":"x","DisplayClaims":{"xui":[{"uhs":"h"},{"uhs":4}]}}"#,
        ),
        (
            2,
            r#"{"access_token":"has space","token_type":"Bearer","expires_in":1}"#,
        ),
        (3, r#"{"items":[{"name":""}]}"#),
        (3, r#"{"items":[]}"#),
        (4, r#"{"id":"invalid","name":"bad"}"#),
    ] {
        let mut steps: Vec<_> = chain().into_iter().take(index).collect();
        steps.push(step(200, body));
        same("auth.minecraft.exchange", Some(CLIENT), steps);
    }
    for body in [
        "{}",
        r#"{"access_token":"synthetic","token_type":"Bearer","expires_in":0}"#,
        r#"{"access_token":"synthetic","token_type":"Bearer","expires_in":1,"refresh_token":""}"#,
        r#"{"access_token":"synthetic","token_type":"Bearer","expires_in":1,"refresh_token":42}"#,
    ] {
        same(
            "auth.microsoft.device.poll",
            Some(POLL),
            vec![step(200, body)],
        );
    }
}

#[test]
#[ignore = "requires pinned injected Zig oracle"]
fn escaped_response_shapes_and_bounded_errors() {
    for body in [
        r#"{"token_type":"Bearer","expires_in":1,"access_token":"synthetic\u002dms\uD83D\uDE00","refresh_token":"synthetic\nrotated"}"#,
        r#"{"token_type":"Bearer","expires_in":1,"access_token":"synthetic\uD800","refresh_token":"synthetic"}"#,
        r#"{"token_type":"Bearer","expires_in":1,"access_token":"synthetic","refresh_token":"synthetic","unknown":"\uD800"}"#,
        r#"{"token_type":"Bearer","expires_in":1,"access_token":"synthetic","refresh_token":"synthetic","scope":3}"#,
        r#"{"token_type":"Bearer","expires_in":1,"access_token":"synthetic","access_token":"other","refresh_token":"synthetic"}"#,
    ] {
        same(
            "auth.microsoft.device.poll",
            Some(POLL),
            vec![step(200, body)],
        );
    }
    for len in [128, 129, 256] {
        same(
            "auth.microsoft.device.poll",
            Some(POLL),
            vec![step(400, &json!({"error":"x".repeat(len)}).to_string())],
        );
    }
}

#[test]
#[ignore = "requires pinned injected Zig oracle"]
fn escaped_error_allocation_limit() {
    for len in [128, 129, 256] {
        let body = format!("{{\"error\":\"{}\"}}", "\\u0078".repeat(len));
        same(
            "auth.microsoft.device.poll",
            Some(POLL),
            vec![step(400, &body)],
        );
    }
}
