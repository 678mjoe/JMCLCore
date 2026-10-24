//! Byte-bounded HTTPS provider metadata, separated from credential-free artifact I/O.
//! Cross-origin redirects on keyed requests are deliberately not followed: native
//! transport error/redirect parity remains a documented acceptance gate.
//! No new time deadline is imposed: the pinned content metadata fetch has none.
//! reqwest may copy header bytes internally; no all-allocation wiping claim.
use super::{curseforge, modrinth as mr, remote};
use crate::install;
use std::io::{Read, Write};
pub struct Native;
impl Native {
    fn body(&self, url: &str, key: Option<&str>, max: usize) -> mr::Result<Vec<u8>> {
        if !url.starts_with("https://") {
            return Err("InsecureContentUrl".into());
        }
        if key.is_some() && !url.starts_with("https://api.curseforge.com/") {
            return Err("InvalidContentApiOrigin".into());
        }
        let keyed = key.is_some();
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .user_agent("jmcl-core/0.1.0-dev")
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() > 3 {
                    return attempt.error("redirect limit");
                }
                if keyed
                    && (attempt.url().scheme() != "https"
                        || attempt.url().host_str() != Some("api.curseforge.com")
                        || attempt.url().port_or_known_default() != Some(443))
                {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|_| mr::Error::from("TlsInitializationFailed"))?;
        let mut request = client.get(url);
        if let Some(key) = key {
            request = request.header("x-api-key", key)
        }
        let response = request
            .send()
            .map_err(|_| mr::Error::from("HttpRequestFailed"))?;
        let status = response.status();
        let mut body = Vec::with_capacity(max.min(65536));
        response
            .take(max as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|_| mr::Error::from("ReadFailed"))?;
        if body.len() > max {
            return Err("ResponseBodyTooLarge".into());
        }
        if !status.is_success() {
            return Err("UnsuccessfulHttpStatus".into());
        }
        Ok(body)
    }
}
impl remote::Metadata for Native {
    fn get(&self, url: &str, max: usize) -> mr::Result<Vec<u8>> {
        self.body(url, None, max)
    }
}
impl curseforge::Metadata for Native {
    fn get(&self, url: &str, key: &str, max: usize) -> mr::Result<Vec<u8>> {
        self.body(url, Some(key), max)
    }
}
impl install::Transport for Native {
    fn fetch(&self, url: &str, sink: &mut dyn Write) -> Result<u16, install::Error> {
        install::Transport::fetch(&install::HttpTransport::new()?, url, sink)
    }
}
