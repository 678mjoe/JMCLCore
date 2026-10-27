//! Read-only instance presentation and receipt identity. Operational APIs stay String-based.
use super::manifest_ordered::{Name, number, source};
use super::{DisplayName, MAX_MANIFEST_BYTES, validate_id};
use crate::content::modrinth::project_ordered::{Cursor, Token, decode};
use crate::content::modrinth::{Error, Result};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Serialize)]
pub(super) struct Manifest {
    schema_version: u32,
    id: DisplayName,
    pub(super) name: DisplayName,
    pub(super) version_id: DisplayName,
    fabric_loader: Option<DisplayName>,
    neoforge_version: Option<DisplayName>,
    forge_version: Option<DisplayName>,
    pub(super) source: String,
}
struct Wire {
    schema: u32,
    id: Option<DisplayName>,
    name: Option<DisplayName>,
    version: Option<DisplayName>,
    fabric: Option<DisplayName>,
    neo: Option<DisplayName>,
    forge: Option<DisplayName>,
    source: String,
    os: Option<DisplayName>,
    arch: Option<DisplayName>,
}
fn optional(c: &mut Cursor<'_>) -> Result<Option<DisplayName>> {
    if c.peek_value()? == b'n' {
        c.next()?;
        Ok(None)
    } else {
        DisplayName::read(c).map(Some)
    }
}
// Source-ordered typed parsing: duplicate/error order is not a serde Value map.
// Unknown fields are skipped; only the chosen persisted schema is recognized.
fn wire(bytes: &[u8], receipt: bool) -> Result<Wire> {
    let mut c = Cursor::new(bytes);
    c.peek_value()?;
    if c.next()? != Token::OpenObject {
        return Err("UnexpectedToken".into());
    }
    let mut w = Wire {
        schema: 1,
        id: None,
        name: None,
        version: None,
        fabric: None,
        neo: None,
        forge: None,
        source: "official".into(),
        os: None,
        arch: None,
    };
    let names: &[&[u8]] = &[
        b"schema_version",
        b"id",
        b"name",
        b"version_id",
        b"fabric_loader",
        b"neoforge_version",
        b"forge_version",
        b"source",
        b"os",
        b"arch",
    ];
    let mut seen = 0u16;
    let mut key = c.object_key(true)?;
    if key != Token::CloseObject {
        loop {
            let Token::String(raw) = key else {
                return c.error();
            };
            let field = decode(raw)?;
            let index = names
                .iter()
                .position(|name| *name == field.as_slice())
                .filter(|i| if receipt { *i != 1 && *i != 2 } else { *i < 8 });
            if let Some(i) = index {
                let bit = 1u16 << i;
                if seen & bit != 0 {
                    return Err("DuplicateField".into());
                }
                seen |= bit;
            }
            c.expect(Token::Colon)?;
            match index {
                Some(0) => w.schema = number(&mut c)?,
                Some(1) => w.id = Some(DisplayName::read(&mut c)?),
                Some(2) => w.name = Some(DisplayName::read(&mut c)?),
                Some(3) => w.version = Some(DisplayName::read(&mut c)?),
                Some(4) => w.fabric = optional(&mut c)?,
                Some(5) => w.neo = optional(&mut c)?,
                Some(6) => w.forge = optional(&mut c)?,
                Some(7) => w.source = source(&mut c)?,
                Some(8) => w.os = Some(DisplayName::read(&mut c)?),
                Some(9) => w.arch = Some(DisplayName::read(&mut c)?),
                _ => c.skip()?,
            }
            match c.separator(Token::CloseObject, false)? {
                Token::CloseObject => break,
                Token::Comma => key = c.object_key(false)?,
                _ => return c.error(),
            }
        }
    }
    if w.version.is_none()
        || if receipt {
            w.os.is_none() || w.arch.is_none()
        } else {
            w.id.is_none() || w.name.is_none()
        }
    {
        return Err(Error("MissingField".into()));
    }
    c.finish()?;
    Ok(w)
}
fn category(error: Error) -> &'static str {
    match error.0.as_str() {
        "UnexpectedEndOfInput" => "UnexpectedEndOfInput",
        "UnexpectedToken" => "UnexpectedToken",
        "InvalidEnumTag" => "InvalidEnumTag",
        "Overflow" => "Overflow",
        "InvalidNumber" => "InvalidNumber",
        "InvalidCharacter" => "InvalidCharacter",
        "MissingField" => "MissingField",
        "DuplicateField" => "DuplicateField",
        _ => "SyntaxError",
    }
}
fn length(bytes: &DisplayName, max: usize) -> bool {
    !bytes.0.is_empty() && bytes.0.len() <= max
}
fn loaders<'a>(
    fabric: &'a Option<DisplayName>,
    neo: &'a Option<DisplayName>,
    forge: &'a Option<DisplayName>,
) -> [&'a Option<DisplayName>; 3] {
    [fabric, neo, forge]
}
impl Manifest {
    pub(super) fn parse(bytes: &[u8], id: &str) -> std::result::Result<Self, &'static str> {
        validate_id(id)?;
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err("InstanceManifestTooLarge");
        }
        let w = wire(bytes, false).map_err(category)?;
        let m = Self {
            schema_version: w.schema,
            id: w.id.unwrap(),
            name: w.name.unwrap(),
            version_id: w.version.unwrap(),
            fabric_loader: w.fabric,
            neoforge_version: w.neo,
            forge_version: w.forge,
            source: w.source,
        };
        let text = std::str::from_utf8(&m.id.0).map_err(|_| "InvalidInstanceId")?;
        validate_id(text)?;
        if m.schema_version != 1 {
            return Err("UnsupportedInstanceSchema");
        }
        if !length(&m.name, 256) {
            return Err("InvalidInstanceName");
        }
        if !length(&m.version_id, 256) {
            return Err("InvalidVersionId");
        }
        for (v, error) in [
            (m.fabric_loader.as_ref(), "InvalidFabricLoaderVersion"),
            (m.neoforge_version.as_ref(), "InvalidNeoForgeVersion"),
            (m.forge_version.as_ref(), "InvalidForgeVersion"),
        ] {
            if v.is_some_and(|v| !length(v, 256)) {
                return Err(error);
            }
        }
        if loaders(&m.fabric_loader, &m.neoforge_version, &m.forge_version)
            .iter()
            .filter(|v| v.is_some())
            .count()
            > 1
        {
            return Err("LoaderConflict");
        }
        if m.id.0 != id.as_bytes() {
            return Err("InstanceIdMismatch");
        }
        Ok(m)
    }
    pub(super) fn id(&self) -> &str {
        std::str::from_utf8(&self.id.0).expect("validated ASCII instance ID")
    }
    pub(super) fn fields(&self, installed: bool) -> Value {
        json!({"id":self.id,"name":self.name,"version_id":self.version_id,"fabric_loader":self.fabric_loader,"neoforge_version":self.neoforge_version,"forge_version":self.forge_version,"source":self.source,"installed":installed})
    }
    pub(super) fn matches_receipt(&self, bytes: &[u8]) -> bool {
        if bytes.len() > 16 * 1024 {
            return false;
        }
        let Ok(w) = wire(bytes, true) else {
            return false;
        };
        if w.schema != 1
            || !length(w.version.as_ref().unwrap(), 256)
            || !length(w.os.as_ref().unwrap(), 32)
            || !length(w.arch.as_ref().unwrap(), 32)
        {
            return false;
        }
        let values = loaders(&w.fabric, &w.neo, &w.forge);
        if values.iter().filter(|v| v.is_some()).count() > 1
            || values
                .iter()
                .filter_map(|v| v.as_ref())
                .any(|v| !length(v, 256))
        {
            return false;
        }
        // Zig compares only version + optional loader bytes after validating receipt.
        w.version.as_ref() == Some(&self.version_id)
            && w.fabric == self.fabric_loader
            && w.neo == self.neoforge_version
            && w.forge == self.forge_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document(version: &str, loader: &str) -> String {
        format!(r#"{{"id":"demo","name":[255],"version_id":{version},"fabric_loader":{loader}}}"#)
    }
    fn receipt(version: &str, loader: &str) -> String {
        format!(
            r#"{{"version_id":{version},"fabric_loader":{loader},"source":"bmclapi","os":[255],"arch":[254]}}"#
        )
    }
    #[test]
    fn persisted_byte_view_preserves_fields_without_widening_operational_api() {
        let raw = document("[254]", "[253]");
        assert_eq!(
            super::super::ReadManifest::parse(raw.as_bytes(), "demo").unwrap_err(),
            "UnexpectedToken"
        );
        assert!(super::super::Manifest::parse(raw.as_bytes(), "demo").is_err());
        let m = Manifest::parse(raw.as_bytes(), "demo").unwrap();
        assert_eq!(
            m.fields(false),
            json!({"id":"demo","name":[255],"version_id":[254],"fabric_loader":[253],"neoforge_version":null,"forge_version":null,"source":"official","installed":false})
        );
        let raw = raw.replace(r#""id":"demo""#, r#""id":[100,101,109,111]"#);
        assert_eq!(
            Manifest::parse(raw.as_bytes(), "demo").unwrap().id(),
            "demo"
        );
        for field in [
            "version_id",
            "fabric_loader",
            "neoforge_version",
            "forge_version",
        ] {
            let mut v = json!({"id":"demo","name":"Name","version_id":"v"});
            v[field] = json!([255]);
            assert_eq!(
                Manifest::parse(v.to_string().as_bytes(), "demo")
                    .unwrap()
                    .fields(false)[field],
                json!([255])
            );
            v[field] = json!([231, 137, 136]);
            assert_eq!(
                Manifest::parse(v.to_string().as_bytes(), "demo")
                    .unwrap()
                    .fields(false)[field],
                "版"
            );
        }
    }
    #[test]
    fn persisted_byte_errors_follow_schema_then_manifest_order() {
        for (raw, error) in [
            (r#"{"id":"demo","name":"N","version_id":[256]}"#, "Overflow"),
            (
                r#"{"id":"demo","name":"N","version_id":[65.00000000000000001]}"#,
                "InvalidNumber",
            ),
            (
                r#"{"id":"demo","name":"N","version_id":[255],"version_id":null}"#,
                "DuplicateField",
            ),
            (
                r#"{"id":[255],"name":[],"version_id":[255],"schema_version":0}"#,
                "InvalidInstanceId",
            ),
            (
                r#"{"id":"demo","name":[],"version_id":[255],"schema_version":0}"#,
                "UnsupportedInstanceSchema",
            ),
            (
                r#"{"id":"other","name":[],"version_id":[]}"#,
                "InvalidInstanceName",
            ),
            (
                r#"{"id":"other","name":[255],"version_id":[]}"#,
                "InvalidVersionId",
            ),
            (
                r#"{"id":"other","name":[255],"version_id":[254]}"#,
                "InstanceIdMismatch",
            ),
            (
                r#"{"id":"demo","name":"N","version_id":"v","fabric_loader":[],"forge_version":[255]}"#,
                "InvalidFabricLoaderVersion",
            ),
            (
                r#"{"id":"demo","name":"N","version_id":"v","fabric_loader":[255],"forge_version":[254]}"#,
                "LoaderConflict",
            ),
            (
                r#"{"id":"demo","name":"N","version_id":"v","source":"wrong","id":"demo"}"#,
                "InvalidEnumTag",
            ),
        ] {
            assert_eq!(
                Manifest::parse(raw.as_bytes(), "demo").unwrap_err(),
                error,
                "{raw}"
            );
        }
        let mut v = json!({"id":"demo","name":"N","version_id":vec![255;256]});
        assert!(Manifest::parse(v.to_string().as_bytes(), "demo").is_ok());
        v["version_id"] = json!(vec![255; 257]);
        assert_eq!(
            Manifest::parse(v.to_string().as_bytes(), "demo").unwrap_err(),
            "InvalidVersionId"
        );
        assert_eq!(
            Manifest::parse(&vec![b' '; MAX_MANIFEST_BYTES + 1], "demo").unwrap_err(),
            "InstanceManifestTooLarge"
        );
        assert_eq!(
            Manifest::parse(b"{}", "../bad").unwrap_err(),
            "InvalidInstanceId"
        );
    }
    #[test]
    fn installed_identity_compares_bytes_after_receipt_validation() {
        let m = Manifest::parse(document("[254]", "[253]").as_bytes(), "demo").unwrap();
        assert!(m.matches_receipt(receipt("[254]", "[253]").as_bytes()));
        assert!(!m.matches_receipt(receipt("[255]", "[253]").as_bytes()));
        assert!(!m.matches_receipt(receipt("[254]", "[252]").as_bytes()));
        assert!(!m.matches_receipt(receipt("[254]", "null").as_bytes()));
        let good = receipt("[254]", "[253]");
        for (from, to) in [
            (r#""os":[255]"#, r#""os":[]"#),
            (r#""arch":[254]"#, r#""arch":null"#),
            (r#""source":"bmclapi""#, r#""source":"wrong""#),
            (
                r#""version_id":[254]"#,
                r#""schema_version":0,"version_id":[254]"#,
            ),
            (
                r#""fabric_loader":[253]"#,
                r#""fabric_loader":[253],"forge_version":[252]"#,
            ),
            (
                r#""version_id":[254]"#,
                r#""version_id":[254],"version_id":[254]"#,
            ),
        ] {
            assert!(
                !m.matches_receipt(good.replace(from, to).as_bytes()),
                "{to}"
            );
        }
        assert!(
            m.matches_receipt(
                good.replace(
                    r#""version_id":[254]"#,
                    r#""future":1,"future":2,"version_id":[254]"#
                )
                .as_bytes()
            )
        );
        assert!(!m.matches_receipt(&vec![b' '; 16 * 1024 + 1]));
        let utf = Manifest::parse(document(r#""v""#, "null").as_bytes(), "demo").unwrap();
        assert!(utf.matches_receipt(receipt("[118]", "null").as_bytes()));
    }

    #[cfg(unix)]
    fn snapshot(
        root: &std::path::Path,
    ) -> std::collections::BTreeMap<std::path::PathBuf, (Vec<u8>, u64, u64, u64, u32, i64, i64)>
    {
        use std::os::unix::fs::MetadataExt;
        fn walk(
            root: &std::path::Path,
            p: &std::path::Path,
            out: &mut std::collections::BTreeMap<
                std::path::PathBuf,
                (Vec<u8>, u64, u64, u64, u32, i64, i64),
            >,
        ) {
            let m = std::fs::symlink_metadata(p).unwrap();
            let bytes = if m.is_file() {
                std::fs::read(p).unwrap()
            } else {
                vec![]
            };
            out.insert(
                p.strip_prefix(root).unwrap().to_owned(),
                (
                    bytes,
                    m.dev(),
                    m.ino(),
                    m.nlink(),
                    m.mode(),
                    m.mtime(),
                    m.mtime_nsec(),
                ),
            );
            if m.is_dir() {
                for item in std::fs::read_dir(p).unwrap() {
                    walk(root, &item.unwrap().path(), out);
                }
            }
        }
        let mut out = std::collections::BTreeMap::new();
        walk(root, root, &mut out);
        out
    }
    #[cfg(unix)]
    #[test]
    fn owned_get_list_and_cli_reads_leave_fixture_bytes_and_identity_unchanged() {
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path().join("instances");
        std::fs::create_dir_all(root.join("demo/.minecraft/.jmcl")).unwrap();
        std::fs::write(root.join("demo/instance.json"), document("[254]", "[253]")).unwrap();
        std::fs::write(
            root.join("demo/.minecraft/.jmcl/install.json"),
            receipt("[254]", "[253]"),
        )
        .unwrap();
        std::fs::create_dir(root.join("broken")).unwrap();
        std::fs::write(root.join("broken/instance.json"), b"{}").unwrap();
        std::fs::write(owner.path().join("sentinel"), b"unchanged").unwrap();
        std::fs::hard_link(owner.path().join("sentinel"), owner.path().join("peer")).unwrap();
        let before = snapshot(owner.path());
        let read = |method: &str, id: Option<&str>| {
            assert!(matches!(method, "instance.get" | "instance.list"));
            assert!(root.is_dir() && root.starts_with(owner.path()));
            let mut value = json!({"directory":root.to_str().unwrap()});
            if let Some(id) = id {
                value["id"] = json!(id);
            }
            let raw = serde_json::value::RawValue::from_string(value.to_string()).unwrap();
            super::super::handle(method, Some(&raw)).unwrap()
        };
        let got = read("instance.get", Some("demo")).unwrap();
        assert_eq!(got["version_id"], json!([254]));
        assert_eq!(got["installed"], true);
        let list = read("instance.list", None).unwrap();
        assert_eq!(list["instances"].as_array().unwrap().len(), 1);
        assert_eq!(list["instances"][0]["fabric_loader"], json!([253]));
        assert_eq!(
            read("instance.get", Some("broken")).unwrap_err().code,
            "INSTANCE_LOAD_FAILED"
        );
        assert_eq!(
            read("instance.get", Some("absent")).unwrap_err().code,
            "INSTANCE_NOT_FOUND"
        );
        for command in ["list", "show"] {
            for as_json in [false, true] {
                assert!(matches!(command, "list" | "show"));
                let mut args = vec![command.to_owned()];
                if command == "show" {
                    args.push("demo".into());
                }
                args.extend([
                    "--instances-directory".into(),
                    root.to_str().unwrap().into(),
                ]);
                if as_json {
                    args.push("--json".into());
                }
                let (mut out, mut err) = (vec![], vec![]);
                assert_eq!(
                    super::super::cli(&args, &mut out, &mut err)
                        .unwrap()
                        .unwrap(),
                    0
                );
                assert!(err.is_empty());
                if as_json {
                    let value: Value = serde_json::from_slice(&out).unwrap();
                    let m = if command == "show" {
                        &value["instance"]
                    } else {
                        &value["instances"][0]
                    };
                    assert_eq!(m["version_id"], json!([254]));
                } else {
                    assert!(out.contains(&254));
                    assert!(out.contains(&255));
                }
            }
        }
        assert_eq!(snapshot(owner.path()), before);
    }
}
