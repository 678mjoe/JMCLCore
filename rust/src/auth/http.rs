//! Production HTTPS adapter. Unit tests construct requests but never execute them.
use super::{
    DEVICE, ENTITLEMENTS, Error, Kind, LOGIN, MAX_RESPONSE_BYTES, PROFILE, Request, Response,
    TIMEOUT_MS, TOKEN, Transport, XBOX, XSTS,
};
use reqwest::blocking::{Body, Client};
use reqwest::header::{ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use std::io::{Cursor, Read};
use std::time::Duration;
use zeroize::Zeroizing;

/// Stateless adapter: no cookie jar, credential persistence or automatic redirects.
#[derive(Default)]
pub struct HttpsTransport;
fn network_error(error: reqwest::Error) -> Error {
    if error.is_timeout() {
        Error::Timeout
    } else if error.is_decode() {
        Error::InvalidResponse
    } else {
        Error::Network
    }
}
fn allowed(request: &Request<'_>) -> bool {
    match request.kind {
        Kind::FormPost => {
            request.url == TOKEN
                || request.url == DEVICE
                || request
                    .url
                    .strip_prefix(DEVICE)
                    .and_then(|s| s.strip_prefix("?mkt="))
                    .is_some_and(|l| {
                        !l.is_empty()
                            && l.len() <= 35
                            && !l.starts_with('-')
                            && !l.ends_with('-')
                            && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    })
        }
        Kind::JsonPost => matches!(request.url, XBOX | XSTS | LOGIN),
        Kind::BearerGet => matches!(request.url, ENTITLEMENTS | PROFILE),
    }
}
fn prepare(request: &Request<'_>) -> Result<(Client, reqwest::blocking::Request), Error> {
    if !allowed(request)
        || request.max_response_bytes != MAX_RESPONSE_BYTES
        || request.timeout_ms > TIMEOUT_MS
    {
        return Err(Error::InvalidResponse);
    }
    if request.timeout_ms == 0 {
        return Err(Error::Timeout);
    }
    let client = Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(request.timeout_ms))
        .user_agent("jmcl-core/0.1.0-dev")
        .build()
        .map_err(network_error)?;
    let builder = match request.kind {
        Kind::FormPost | Kind::JsonPost => {
            // Body::new owns this wiping reader instead of taking an ordinary Vec.
            // reqwest/TLS internal buffers remain outside our erasure guarantee.
            let bytes = Zeroizing::new(request.payload.to_vec());
            client
                .post(request.url)
                .header(
                    CONTENT_TYPE,
                    match request.kind {
                        Kind::FormPost => "application/x-www-form-urlencoded",
                        _ => "application/json",
                    },
                )
                .body(Body::sized(
                    Cursor::new(bytes),
                    request.payload.len() as u64,
                ))
        }
        Kind::BearerGet => {
            if !request.payload.iter().all(|b| (0x21..=0x7e).contains(b)) {
                return Err(Error::InvalidResponse);
            }
            let mut bearer = Zeroizing::new(Vec::with_capacity(7 + request.payload.len()));
            bearer.extend_from_slice(b"Bearer ");
            bearer.extend_from_slice(request.payload);
            let mut header =
                HeaderValue::from_bytes(&bearer).map_err(|_| Error::InvalidResponse)?;
            header.set_sensitive(true);
            // HeaderValue does not expose zeroization; never Debug/log the request.
            client.get(request.url).header(AUTHORIZATION, header)
        }
    };
    Ok((
        client,
        builder
            .timeout(Duration::from_millis(request.timeout_ms))
            .header(ACCEPT, "application/json")
            .header(ACCEPT_ENCODING, "identity")
            .build()
            .map_err(network_error)?,
    ))
}
fn read_bounded(mut reader: impl Read, limit: usize) -> Result<Zeroizing<Vec<u8>>, Error> {
    let mut body = Zeroizing::new(vec![0; limit + 1]);
    let mut len = 0;
    loop {
        let n = reader.read(&mut body[len..]).map_err(|e| {
            if let Some(inner) = e
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<reqwest::Error>())
            {
                if inner.is_timeout() {
                    return Error::Timeout;
                }
                if inner.is_decode() {
                    return Error::InvalidResponse;
                }
            }
            if e.kind() == std::io::ErrorKind::TimedOut {
                Error::Timeout
            } else {
                Error::Network
            }
        })?;
        if n == 0 {
            break;
        }
        len += n;
        if len > limit {
            return Err(Error::ResponseTooLarge);
        }
    }
    body.truncate(len);
    Ok(body)
}
impl Transport for HttpsTransport {
    fn request(&mut self, request: Request<'_>) -> Result<Response, Error> {
        let (client, wire) = prepare(&request)?;
        let response = client.execute(wire).map_err(network_error)?;
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|n| n > request.max_response_bytes as u64)
        {
            return Err(Error::ResponseTooLarge);
        }
        let body = read_bounded(response, request.max_response_bytes)?;
        Ok(Response { status, body })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request(kind: Kind, url: &str) -> Request<'_> {
        Request {
            kind,
            url,
            payload: b"synthetic-token",
            max_response_bytes: MAX_RESPONSE_BYTES,
            timeout_ms: TIMEOUT_MS,
        }
    }
    #[test]
    fn prepares_official_requests_without_sending() {
        let (_, r) = prepare(&request(Kind::BearerGet, PROFILE)).unwrap();
        assert_eq!(r.method(), reqwest::Method::GET);
        assert_eq!(r.headers()[AUTHORIZATION], "Bearer synthetic-token");
        assert!(r.headers()[AUTHORIZATION].is_sensitive());
        assert!(!format!("{:?}", r.headers()).contains("synthetic-token"));
        let (_, r) = prepare(&request(Kind::FormPost, TOKEN)).unwrap();
        assert_eq!(
            r.headers()[CONTENT_TYPE],
            "application/x-www-form-urlencoded"
        );
        assert_eq!(r.method(), reqwest::Method::POST);
    }
    #[test]
    fn rejects_untrusted_targets_before_network() {
        for url in [
            "http://localhost/token",
            "https://example.com/token",
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/token#fragment",
            "https://user:password@login.microsoftonline.com/token",
        ] {
            assert!(matches!(
                prepare(&request(Kind::FormPost, url)),
                Err(Error::InvalidResponse)
            ));
        }
        let mut r = request(Kind::BearerGet, PROFILE);
        r.timeout_ms = 0;
        assert!(matches!(prepare(&r), Err(Error::Timeout)));
    }
    #[test]
    fn response_limit_is_enforced_while_reading() {
        assert_eq!(&*read_bounded(Cursor::new(b"abcd"), 4).unwrap(), b"abcd");
        assert!(matches!(
            read_bounded(Cursor::new(b"abcde"), 4),
            Err(Error::ResponseTooLarge)
        ));
        assert!(read_bounded(Cursor::new(b""), 4).unwrap().is_empty());
    }
}
