//! Pure NeoForge installer profile policy. No processors are executed here.
use super::fabric;
use crate::metadata::{self, Error};
use serde_json::{Value, json};
fn fail(name: &'static str) -> Error {
    Error::Plan(name)
}
pub fn version_id(value: &str) -> Result<(), Error> {
    fabric::profile_id(value).map_err(|_| fail("InvalidNeoForgeVersion"))
}
pub fn artifact_path(value: &str) -> Result<String, Error> {
    if value.is_empty() || value.len() > 512 {
        return Err(fail("InvalidMavenCoordinate"));
    }
    let (coordinate, extension) = value.rsplit_once('@').unwrap_or((value, "jar"));
    if extension.is_empty()
        || extension.len() > 16
        || !extension.bytes().all(|c| c.is_ascii_alphanumeric())
    {
        return Err(fail("InvalidMavenCoordinate"));
    }
    let path = fabric::artifact_path(coordinate)?;
    Ok(format!(
        "{}.{extension}",
        path.strip_suffix(".jar").unwrap()
    ))
}
fn placeholder(value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(fail("InvalidPlaceholderName"));
    }
    Ok(())
}
#[derive(Debug, PartialEq, Eq)]
pub enum Data<'a> {
    Coordinate(&'a str),
    Embedded(&'a str),
    Literal(&'a str),
}
pub fn classify(value: &str) -> Data<'_> {
    if value.len() >= 2 {
        if value.starts_with('[') && value.ends_with(']') {
            return Data::Coordinate(&value[1..value.len() - 1]);
        }
        if value.starts_with('\'') && value.ends_with('\'') {
            return Data::Literal(&value[1..value.len() - 1]);
        }
    }
    if let Some(path) = value.strip_prefix('/') {
        Data::Embedded(path)
    } else {
        Data::Literal(value)
    }
}
fn data_value(value: &str) -> Result<(), Error> {
    if value.is_empty() || value.len() > 4096 {
        return Err(fail("InvalidDataValue"));
    }
    match classify(value) {
        Data::Coordinate(c) => {
            artifact_path(c)?;
        }
        Data::Embedded(p) => {
            if p.is_empty() || p.contains("..") || p.contains('\\') {
                return Err(fail("InvalidDataValue"));
            }
        }
        Data::Literal(_) => {}
    }
    Ok(())
}
fn library(l: &Value, allow_data: bool) -> Result<(), Error> {
    let name = l["name"].as_str().unwrap();
    artifact_path(name)?;
    let ext = name.rsplit_once('@').map(|x| x.1).unwrap_or("jar");
    if ext != "jar" && !(allow_data && ["zip", "txt", "lzma"].contains(&ext)) {
        return Err(fail("UnsupportedLibraryExtension"));
    }
    if l["downloads"].is_null() {
        return Err(fail("MissingLibraryDownloads"));
    }
    let a = &l["downloads"]["artifact"];
    if a.is_null() {
        return Err(fail("MissingLibraryArtifact"));
    }
    let u = a["url"].as_str().unwrap();
    if !u.starts_with("https://") {
        return Err(fail("InsecureArtifactUrl"));
    }
    if u.len() == 8
        || u[8..].starts_with('/')
        || u[8..].contains("..")
        || !u
            .bytes()
            .all(|c| (0x21..=0x7e).contains(&c) && !b"\\?#@".contains(&c))
    {
        return Err(fail("InvalidArtifactUrl"));
    }
    fabric::sha1(a["sha1"].as_str().unwrap())?;
    fabric::size(a["size"].as_u64().unwrap())
}
pub fn install(bytes: &[u8]) -> Result<Value, Error> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(fail("InstallProfileTooLarge"));
    }
    let p = metadata::neoforge_install(bytes)?;
    if p["spec"].as_u64().unwrap() > 1 {
        return Err(fail("UnsupportedInstallProfileSpec"));
    }
    version_id(p["version"].as_str().unwrap())?;
    version_id(p["minecraft"].as_str().unwrap())?;
    let data = p["data"].as_object().unwrap();
    if data.len() > 64 {
        return Err(fail("TooManyDataEntries"));
    }
    for (k, v) in data {
        placeholder(k)?;
        data_value(v["client"].as_str().unwrap())?;
        if let Some(s) = v["server"].as_str() {
            data_value(s)?;
        }
    }
    let processors = p["processors"].as_array().unwrap();
    if processors.len() > 16 {
        return Err(fail("TooManyProcessors"));
    }
    for processor in processors {
        artifact_path(processor["jar"].as_str().unwrap())?;
        let cp = processor["classpath"].as_array().unwrap();
        if cp.len() > 64 {
            return Err(fail("TooManyProcessorClasspaths"));
        }
        for c in cp {
            artifact_path(c.as_str().unwrap())?;
        }
        let args = processor["args"].as_array().unwrap();
        if args.len() > 64 {
            return Err(fail("TooManyProcessorArguments"));
        }
        for a in args {
            let a = a.as_str().unwrap();
            if a.is_empty() || a.len() > 4096 {
                return Err(fail("InvalidProcessorArgument"));
            }
        }
        if let Some(sides) = processor["sides"].as_array() {
            for s in sides {
                if s != "client" && s != "server" {
                    return Err(fail("InvalidProcessorSide"));
                }
            }
        }
    }
    let libs = p["libraries"].as_array().unwrap();
    if libs.len() > 1024 {
        return Err(fail("TooManyLibraries"));
    }
    for l in libs {
        library(l, true)?;
    }
    Ok(p)
}
pub fn version(bytes: &[u8]) -> Result<Value, Error> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(fail("VersionProfileTooLarge"));
    }
    let p = metadata::neoforge_version(bytes)?;
    version_id(p["id"].as_str().unwrap())?;
    version_id(p["inheritsFrom"].as_str().unwrap())?;
    let main = p["mainClass"].as_str().unwrap();
    if main.is_empty()
        || main.len() > 512
        || !main
            .bytes()
            .all(|c| (0x21..=0x7e).contains(&c) && !b"/\\".contains(&c))
    {
        return Err(fail("InvalidMainClass"));
    }
    if let Some(a) = p["minecraftArguments"].as_str()
        && (a.is_empty() || a.len() > 16 * 1024 || !a.bytes().all(|c| (0x20..0x7f).contains(&c)))
    {
        return Err(fail("InvalidMinecraftArguments"));
    }
    let libs = p["libraries"].as_array().unwrap();
    if libs.len() > 1024 {
        return Err(fail("TooManyLibraries"));
    }
    for l in libs {
        library(l, false)?;
    }
    Ok(p)
}
pub fn patched_path(profile: &Value, server: bool) -> Result<String, Error> {
    let entry = &profile["data"]["PATCHED"];
    let value = if server {
        entry["server"].as_str().or(entry["client"].as_str())
    } else {
        entry["client"].as_str()
    }
    .ok_or(fail("MissingPatchedData"))?;
    if let Data::Coordinate(c) = classify(value) {
        Ok(format!("libraries/{}", artifact_path(c)?))
    } else {
        Err(fail("MissingPatchedData"))
    }
}
pub fn processor_applies(processor: &Value, server: bool) -> bool {
    let Some(sides) = processor["sides"].as_array() else {
        return true;
    };
    sides.is_empty()
        || sides
            .iter()
            .any(|s| s == if server { "server" } else { "client" })
}
pub fn substitute(
    template: &str,
    resolve: impl Fn(&str) -> Option<String>,
    artifact: impl Fn(&str) -> Result<String, Error>,
) -> Result<String, Error> {
    if template.is_empty() || template.len() > 4096 {
        return Err(fail("InvalidProcessorArgument"));
    }
    if template.starts_with('[') && template.ends_with(']') {
        let c = &template[1..template.len() - 1];
        artifact_path(c)?;
        return artifact(c);
    }
    let mut rest = template;
    let mut out = String::new();
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let end = rest[start + 1..]
            .find('}')
            .map(|i| i + start + 1)
            .ok_or(fail("UnresolvedPlaceholder"))?;
        let key = &rest[start + 1..end];
        placeholder(key)?;
        out.push_str(&resolve(key).ok_or(fail("UnresolvedPlaceholder"))?);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    if out.contains(['{', '}']) {
        return Err(fail("UnresolvedPlaceholder"));
    }
    Ok(out)
}
pub fn merge(parent: &Value, p: &Value, patched: &str) -> Result<Value, Error> {
    if patched.is_empty()
        || patched.len() > 1024
        || patched.split('/').any(|c| {
            c.is_empty()
                || c == "."
                || c == ".."
                || !c.bytes().all(|b| (0x21..=0x7e).contains(&b) && b != b'\\')
        })
    {
        return Err(fail("InvalidRelativePath"));
    }
    let mut out = parent.clone();
    let mut libs = Vec::new();
    for l in p["libraries"].as_array().unwrap() {
        let a = &l["downloads"]["artifact"];
        libs.push(json!({"name":l["name"],"downloads":{"artifact":{"path":artifact_path(l["name"].as_str().unwrap())?,"sha1":a["sha1"],"size":a["size"],"url":a["url"]},"classifiers":null},"rules":l["rules"],"natives":l["natives"],"extract":l["extract"],"url":null}));
    }
    libs.extend(parent["libraries"].as_array().unwrap().iter().cloned());
    out["libraries"] = libs.into();
    for key in ["id", "mainClass"] {
        out[key] = p[key].clone();
    }
    for key in ["type", "releaseTime", "time"] {
        if !p[key].is_null() {
            out[key] = p[key].clone();
        }
    }
    out["inheritsFrom"] = Value::Null;
    out["patched_client"] = patched.into();
    if !parent["arguments"].is_null() || !p["arguments"].is_null() {
        let mut args = json!({"game":[],"jvm":[],"default-user-jvm":null});
        for key in ["game", "jvm", "default-user-jvm"] {
            let mut list = Vec::new();
            let mut present = key != "default-user-jvm";
            for v in [&parent["arguments"][key], &p["arguments"][key]] {
                if let Some(items) = v.as_array() {
                    present = true;
                    list.extend(items.iter().cloned());
                }
            }
            if present {
                args[key] = list.into();
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
    fn data_and_substitution_policy() {
        assert_eq!(
            artifact_path("a.b:c:1:client@lzma").unwrap(),
            "a/b/c/1/c-1-client.lzma"
        );
        assert_eq!(classify("'/data/file'"), Data::Literal("/data/file"));
        assert_eq!(classify("/data/file"), Data::Embedded("data/file"));
        assert_eq!(
            substitute("prefix-{A}", |_| Some("value".into()), |_| unreachable!()),
            Ok("prefix-value".into())
        );
        assert_eq!(
            substitute("{MISSING}", |_| None, |_| unreachable!()),
            Err(fail("UnresolvedPlaceholder"))
        );
        assert_eq!(
            substitute("[a:b:c]", |_| None, artifact_path),
            Ok("a/b/c/b-c.jar".into())
        );
        assert!(processor_applies(&json!({"sides":[]}), false));
        assert!(!processor_applies(&json!({"sides":["server"]}), false));
    }
}
