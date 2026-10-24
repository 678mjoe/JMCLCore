//! Fabric profiles are overlays, never alternate copies of the shared game files.
use crate::metadata::{self, Error};
use serde_json::{Value, json};
const MAX_LIBRARY_SIZE: u64 = 256 * 1024 * 1024;
fn fail(name: &'static str) -> Error {
    Error::Plan(name)
}
pub fn profile_id(s: &str) -> Result<(), Error> {
    if s.is_empty()
        || s.len() > 256
        || s.ends_with(['.', ' '])
        || !s
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_+.".contains(&c))
    {
        return Err(fail("InvalidProfileId"));
    }
    Ok(())
}
fn component(s: &str) -> Result<(), Error> {
    if s.is_empty()
        || s.len() > 256
        || s == "."
        || s == ".."
        || !s
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_+.".contains(&c))
    {
        return Err(fail("InvalidMavenCoordinate"));
    }
    Ok(())
}
pub fn artifact_path(name: &str) -> Result<String, Error> {
    let c: Vec<_> = name.split(':').collect();
    if !(3..=4).contains(&c.len()) {
        return Err(fail("InvalidMavenCoordinate"));
    }
    for part in c[0].split('.') {
        component(part)?;
    }
    for part in &c[1..] {
        component(part)?;
    }
    let classifier = if c.len() == 4 {
        format!("-{}", c[3])
    } else {
        String::new()
    };
    Ok(format!(
        "{}/{}/{}/{}-{}{}.jar",
        c[0].replace('.', "/"),
        c[1],
        c[2],
        c[1],
        c[2],
        classifier
    ))
}
pub fn sha1(s: &str) -> Result<(), Error> {
    if s.len() != 40 || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(fail("InvalidLibrarySha1"));
    }
    Ok(())
}
pub fn size(n: u64) -> Result<(), Error> {
    if n == 0 || n > MAX_LIBRARY_SIZE {
        return Err(fail("InvalidLibrarySize"));
    }
    Ok(())
}
pub fn parse(bytes: &[u8]) -> Result<Value, Error> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(fail("ProfileTooLarge"));
    }
    let p = metadata::fabric_profile(bytes)?;
    profile_id(p["id"].as_str().unwrap())?;
    profile_id(p["inheritsFrom"].as_str().unwrap())?;
    let main = p["mainClass"].as_str().unwrap();
    if main.is_empty()
        || main.len() > 512
        || !main
            .bytes()
            .all(|c| (0x21..=0x7e).contains(&c) && c != b'/' && c != b'\\')
    {
        return Err(fail("InvalidMainClass"));
    }
    let libs = p["libraries"].as_array().unwrap();
    if libs.len() > 1024 {
        return Err(fail("TooManyLibraries"));
    }
    for l in libs {
        artifact_path(l["name"].as_str().unwrap())?;
        let u = l["url"].as_str().unwrap();
        if !u.starts_with("https://") || !u.ends_with('/') {
            return Err(fail("InsecureRepositoryUrl"));
        }
        if u[8..].starts_with('/')
            || u[8..].contains("..")
            || !u
                .bytes()
                .all(|c| (0x21..=0x7e).contains(&c) && !b"\\?#@".contains(&c))
        {
            return Err(fail("InvalidRepositoryUrl"));
        }
        if let Some(s) = l["sha1"].as_str() {
            sha1(s)?;
        }
        if let Some(n) = l["size"].as_u64() {
            size(n)?;
        }
    }
    Ok(p)
}
pub fn merge(parent: &Value, p: &Value, integrities: &[(String, u64)]) -> Result<Value, Error> {
    let libs = p["libraries"].as_array().unwrap();
    if libs.len() != integrities.len() {
        return Err(fail("LibraryIntegrityCountMismatch"));
    }
    let mut out = parent.clone();
    let mut all = Vec::new();
    for (l, (hash, n)) in libs.iter().zip(integrities) {
        sha1(hash)?;
        size(*n)?;
        let path = artifact_path(l["name"].as_str().unwrap())?;
        all.push(json!({"name":l["name"],"downloads":{"artifact":{"path":path,"sha1":hash,"size":n,"url":format!("{}{}",l["url"].as_str().unwrap(),path)},"classifiers":null},"rules":null,"natives":null,"extract":null,"url":l["url"]}));
    }
    all.extend(parent["libraries"].as_array().unwrap().iter().cloned());
    out["libraries"] = all.into();
    for key in ["id", "mainClass"] {
        out[key] = p[key].clone();
    }
    for key in ["type", "releaseTime", "time"] {
        if !p[key].is_null() {
            out[key] = p[key].clone();
        }
    }
    out["inheritsFrom"] = Value::Null;
    if !parent["arguments"].is_null() || !p["arguments"].is_null() {
        let mut args = json!({"game":[],"jvm":[],"default-user-jvm":null});
        for key in ["game", "jvm", "default-user-jvm"] {
            let mut a = Vec::new();
            let mut present = key != "default-user-jvm";
            for v in [&parent["arguments"][key], &p["arguments"][key]] {
                if let Some(items) = v.as_array() {
                    present = true;
                    a.extend(items.iter().cloned());
                }
            }
            if present {
                args[key] = a.into();
            }
        }
        out["arguments"] = args;
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_repository_coordinates_and_sizes() {
        assert_eq!(
            artifact_path("org.example:lib:1.0:native").unwrap(),
            "org/example/lib/1.0/lib-1.0-native.jar"
        );
        for name in ["a:b", "a:b:c:d:e", "a..b:c:d", "a:../x:c", "a:b:"] {
            assert!(artifact_path(name).is_err());
        }
        for url in [
            "http://repo/",
            "https:///",
            "https://host/../",
            "https://user@host/",
            "https://host/?x/",
        ] {
            let p = json!({"id":"fabric-test","inheritsFrom":"1.0","mainClass":"Main","libraries":[{"name":"a:b:c","url":url}]});
            assert!(parse(p.to_string().as_bytes()).is_err());
        }
        assert!(size(0).is_err());
        assert!(size(MAX_LIBRARY_SIZE + 1).is_err());
        assert!(size(MAX_LIBRARY_SIZE).is_ok());
    }
}
