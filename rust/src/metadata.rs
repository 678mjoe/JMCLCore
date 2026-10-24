//! Mojang metadata normalization and source policy, matching the pinned Zig types.
//! Unknown JSON fields are discarded; optional/default fields are emitted explicitly.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha1::{Digest, Sha1};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidJson,
    Json(&'static str),
    VersionNotFound,
    MetadataHashMismatch,
    MetadataIdMismatch,
    AssetIndexHashMismatch,
    ResponseBodyTooLarge,
    UnsuccessfulHttpStatus,
    MetadataFetchTimeout,
    Network,
    UnsupportedLoader,
    Plan(&'static str),
}
impl Error {
    pub fn name(&self) -> &'static str {
        match self {
            Self::InvalidJson => "SyntaxError",
            Self::Json(name) => name,
            Self::VersionNotFound => "VersionNotFound",
            Self::MetadataHashMismatch => "MetadataHashMismatch",
            Self::MetadataIdMismatch => "MetadataIdMismatch",
            Self::AssetIndexHashMismatch => "AssetIndexHashMismatch",
            Self::ResponseBodyTooLarge => "ResponseBodyTooLarge",
            Self::UnsuccessfulHttpStatus => "UnsuccessfulHttpStatus",
            Self::MetadataFetchTimeout => "MetadataFetchTimeout",
            Self::Network => "NetworkError",
            Self::UnsupportedLoader => "NotImplemented",
            Self::Plan(s) => s,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Official,
    Bmclapi,
}
impl Source {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "official" => Some(Self::Official),
            "bmclapi" => Some(Self::Bmclapi),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Bmclapi => "bmclapi",
        }
    }
    pub fn display(self) -> &'static str {
        match self {
            Self::Official => "Mojang",
            Self::Bmclapi => "BMCLAPI",
        }
    }
    pub fn manifest_url(self) -> &'static str {
        match self {
            Self::Official => "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
            Self::Bmclapi => "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json",
        }
    }
    pub fn rewrite(self, url: &str) -> Option<String> {
        const BASE: &str = "https://bmclapi2.bangbang93.com";
        if self == Self::Official || url.starts_with(BASE) {
            return None;
        }
        for prefix in [
            "https://piston-meta.mojang.com/",
            "https://piston-data.mojang.com/",
            "https://launchermeta.mojang.com/",
            "http://launchermeta.mojang.com/",
            "https://launcher.mojang.com/",
            "http://launcher.mojang.com/",
        ] {
            if let Some(rest) = url.strip_prefix(prefix) {
                return Some(format!("{BASE}/{rest}"));
            }
        }
        for (prefix, path) in [
            ("https://resources.download.minecraft.net/", "assets"),
            ("https://libraries.minecraft.net/", "maven"),
        ] {
            if let Some(rest) = url.strip_prefix(prefix) {
                return Some(format!("{BASE}/{path}/{rest}"));
            }
        }
        None
    }
}
pub fn verify_sha1(bytes: &[u8], expected: &str) -> bool {
    expected.len() == 40 && format!("{:x}", Sha1::digest(bytes)).eq_ignore_ascii_case(expected)
}
pub fn parse_json(bytes: &[u8]) -> Result<Value, Error> {
    serde_json::from_slice(bytes).map_err(|_| Error::InvalidJson)
}
// Track duplicate object members before serde_json::Value can overwrite them.
// Zig ignores unknown fields, including their duplicates; only members retained
// by the typed normalized schema participate in DuplicateField detection.
struct Tracked {
    value: Value,
    duplicates: Vec<Vec<String>>,
}
impl<'de> Deserialize<'de> for Tracked {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Tracked;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON metadata")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Tracked, E> {
                Ok(Tracked {
                    value: v.into(),
                    duplicates: vec![],
                })
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Tracked, E> {
                Ok(Tracked {
                    value: v.into(),
                    duplicates: vec![],
                })
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Tracked, E> {
                Ok(Tracked {
                    value: v.into(),
                    duplicates: vec![],
                })
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Tracked, E> {
                Ok(Tracked {
                    value: serde_json::Number::from_f64(v)
                        .map(Value::Number)
                        .ok_or_else(|| E::custom("invalid number"))?,
                    duplicates: vec![],
                })
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Tracked, E> {
                Ok(Tracked {
                    value: v.into(),
                    duplicates: vec![],
                })
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Tracked, E> {
                Ok(Tracked {
                    value: v.into(),
                    duplicates: vec![],
                })
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Tracked, E> {
                Ok(Tracked {
                    value: Value::Null,
                    duplicates: vec![],
                })
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Tracked, A::Error> {
                let mut values = vec![];
                let mut duplicates = vec![];
                while let Some(mut item) = seq.next_element::<Tracked>()? {
                    for path in &mut item.duplicates {
                        path.insert(0, values.len().to_string());
                    }
                    duplicates.extend(item.duplicates);
                    values.push(item.value);
                }
                Ok(Tracked {
                    value: Value::Array(values),
                    duplicates,
                })
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Tracked, A::Error> {
                let mut values = Map::new();
                let mut duplicates = vec![];
                while let Some((key, mut item)) = map.next_entry::<String, Tracked>()? {
                    if values.contains_key(&key) {
                        duplicates.push(vec![key.clone()]);
                    }
                    for path in &mut item.duplicates {
                        path.insert(0, key.clone());
                    }
                    duplicates.extend(item.duplicates);
                    values.insert(key, item.value);
                }
                Ok(Tracked {
                    value: Value::Object(values),
                    duplicates,
                })
            }
        }
        d.deserialize_any(Visitor)
    }
}
fn normalize(bytes: &[u8], schema: Norm) -> Result<Value, Error> {
    let tracked: Tracked = serde_json::from_slice(bytes).map_err(|_| Error::InvalidJson)?;
    let value = schema(&tracked.value)?;
    for path in tracked.duplicates {
        let mut node = Some(&value);
        for key in path {
            node = node.and_then(|v| match v {
                Value::Array(a) => key.parse::<usize>().ok().and_then(|i| a.get(i)),
                Value::Object(o) => o.get(&key),
                _ => None,
            });
        }
        if node.is_some() {
            return Err(Error::Json("DuplicateField"));
        }
    }
    Ok(value)
}

type Norm = fn(&Value) -> Result<Value, Error>;
fn text(v: &Value) -> Result<Value, Error> {
    v.as_str()
        .map(|s| Value::String(s.into()))
        .ok_or(Error::Json("UnexpectedToken"))
}
fn boolean(v: &Value) -> Result<Value, Error> {
    v.as_bool()
        .map(Value::Bool)
        .ok_or(Error::Json("UnexpectedToken"))
}
pub fn unsigned(v: &Value, max: u64) -> Result<u64, Error> {
    fn float(v: f64) -> Result<u64, Error> {
        if !v.is_finite() || v.fract() != 0.0 {
            return Err(Error::Json("InvalidNumber"));
        }
        if !(0.0..18446744073709551616.0).contains(&v) {
            return Err(Error::Json("Overflow"));
        }
        Ok(v as u64)
    }
    let n = match v {
        Value::Number(n) => {
            if let Some(v) = n.as_u64() {
                v
            } else {
                float(n.as_f64().ok_or(Error::Json("InvalidNumber"))?)?
            }
        }
        Value::String(s) => {
            if let Ok(v) = s.parse::<u64>() {
                v
            } else {
                float(
                    s.parse::<f64>()
                        .map_err(|_| Error::Json("InvalidCharacter"))?,
                )?
            }
        }
        _ => return Err(Error::Json("UnexpectedToken")),
    };
    if n > max {
        Err(Error::Json("Overflow"))
    } else {
        Ok(n)
    }
}
fn u64v(v: &Value) -> Result<Value, Error> {
    unsigned(v, u64::MAX).map(Value::from)
}
fn u32v(v: &Value) -> Result<Value, Error> {
    unsigned(v, u32::MAX.into()).map(Value::from)
}
fn u16v(v: &Value) -> Result<Value, Error> {
    unsigned(v, u16::MAX.into()).map(Value::from)
}
fn u8v(v: &Value) -> Result<Value, Error> {
    unsigned(v, u8::MAX.into()).map(Value::from)
}
fn array(v: &Value, f: Norm) -> Result<Value, Error> {
    v.as_array()
        .ok_or(Error::Json("UnexpectedToken"))?
        .iter()
        .map(f)
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}
fn strings(v: &Value) -> Result<Value, Error> {
    array(v, text)
}
fn mapping(v: &Value, f: Norm) -> Result<Value, Error> {
    let mut result = Map::new();
    for (k, v) in v.as_object().ok_or(Error::Json("UnexpectedToken"))? {
        result.insert(k.clone(), f(v)?);
    }
    Ok(Value::Object(result))
}
fn string_map(v: &Value) -> Result<Value, Error> {
    mapping(v, text)
}
fn bool_map(v: &Value) -> Result<Value, Error> {
    mapping(v, boolean)
}
struct Field(&'static str, Norm, Option<Value>);
fn object(v: &Value, fields: &[Field]) -> Result<Value, Error> {
    let input = v.as_object().ok_or(Error::Json("UnexpectedToken"))?;
    let mut out = Map::new();
    for Field(name, f, default) in fields {
        let value = match input.get(*name) {
            Some(Value::Null) if default.as_ref() == Some(&Value::Null) => Value::Null,
            Some(v) => f(v)?,
            None => default.clone().ok_or(Error::Json("MissingField"))?,
        };
        out.insert((*name).into(), value);
    }
    Ok(Value::Object(out))
}
macro_rules! fields {($name:ident, $( $field:literal => $norm:ident $(= $default:expr)? ),* $(,)?)=>{fn $name(v:&Value)->Result<Value,Error>{object(v,&[$(Field($field,$norm,fields!(@default $($default)?))),*])}};(@default)=>{None};(@default $v:expr)=>{Some($v)};}
fn version_type(v: &Value) -> Result<Value, Error> {
    match v.as_str() {
        Some("release" | "snapshot" | "old_beta" | "old_alpha") => text(v),
        Some(_) => Err(Error::Json("InvalidEnumTag")),
        None => Err(Error::Json("UnexpectedToken")),
    }
}
fn action(v: &Value) -> Result<Value, Error> {
    match v.as_str() {
        Some("allow" | "disallow") => text(v),
        Some(_) => Err(Error::Json("InvalidEnumTag")),
        None => Err(Error::Json("UnexpectedToken")),
    }
}
fields!(latest,"release"=>text,"snapshot"=>text);
fields!(version,"id"=>text,"type"=>version_type,"url"=>text,"time"=>text,"releaseTime"=>text,"sha1"=>text=Value::Null,"complianceLevel"=>u8v=Value::Null);
fn versions(v: &Value) -> Result<Value, Error> {
    array(v, version)
}
fields!(manifest_value,"latest"=>latest,"versions"=>versions);
pub fn manifest(bytes: &[u8]) -> Result<Value, Error> {
    normalize(bytes, manifest_value)
}
pub fn select(manifest: &Value, kind: Option<&str>, limit: usize) -> Value {
    let mut out = Map::new();
    out.insert("latest".into(), manifest["latest"].clone());
    out.insert(
        "versions".into(),
        Value::Array(
            manifest["versions"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| kind.is_none_or(|k| v["type"] == k))
                .take(limit)
                .cloned()
                .collect(),
        ),
    );
    Value::Object(out)
}
fields!(range,"min"=>text=Value::Null,"max"=>text=Value::Null);
fields!(os,"name"=>text=Value::Null,"version"=>text=Value::Null,"arch"=>text=Value::Null,"versionRange"=>range=Value::Null);
fields!(rule,"action"=>action,"os"=>os=Value::Null,"features"=>bool_map=Value::Null);
fn rules(v: &Value) -> Result<Value, Error> {
    array(v, rule)
}
fn argument_value(v: &Value) -> Result<Value, Error> {
    if v.is_string() { text(v) } else { strings(v) }
}
fields!(structured,"rules"=>rules=Value::Array(vec![]),"value"=>argument_value);
fn argument(v: &Value) -> Result<Value, Error> {
    if v.is_string() {
        text(v)
    } else {
        structured(v)
    }
}
fn argument_list(v: &Value) -> Result<Value, Error> {
    array(v, argument)
}
fields!(arguments,"game"=>argument_list,"jvm"=>argument_list=Value::Array(vec![]),"default-user-jvm"=>argument_list=Value::Null);
fields!(download,"sha1"=>text,"size"=>u64v,"url"=>text);
fields!(artifact,"path"=>text,"sha1"=>text,"size"=>u64v,"url"=>text);
fn artifacts(v: &Value) -> Result<Value, Error> {
    mapping(v, artifact)
}
fields!(library_downloads,"artifact"=>artifact=Value::Null,"classifiers"=>artifacts=Value::Null);
fields!(extract,"exclude"=>strings=Value::Array(vec![]));
fields!(library,"name"=>text,"downloads"=>library_downloads=Value::Null,"rules"=>rules=Value::Null,"natives"=>string_map=Value::Null,"extract"=>extract=Value::Null,"url"=>text=Value::Null);
fn libraries(v: &Value) -> Result<Value, Error> {
    array(v, library)
}
fields!(asset_descriptor,"id"=>text,"sha1"=>text,"size"=>u64v,"totalSize"=>u64v,"url"=>text);
fields!(downloads,"client"=>download,"server"=>download=Value::Null,"client_mappings"=>download=Value::Null,"server_mappings"=>download=Value::Null,"windows_server"=>download=Value::Null);
fields!(java,"component"=>text,"majorVersion"=>u16v);
fields!(logging_file,"id"=>text,"sha1"=>text,"size"=>u64v,"url"=>text);
fields!(logging_client,"argument"=>text,"file"=>logging_file,"type"=>text);
fields!(logging,"client"=>logging_client);
fields!(metadata_value,"id"=>text,"type"=>text,"mainClass"=>text,"libraries"=>libraries,"downloads"=>downloads,"arguments"=>arguments=Value::Null,"minecraftArguments"=>text=Value::Null,"assetIndex"=>asset_descriptor=Value::Null,"assets"=>text=Value::Null,"javaVersion"=>java=Value::Null,"logging"=>logging=Value::Null,"minimumLauncherVersion"=>u32v=Value::Null,"complianceLevel"=>u8v=Value::Null,"releaseTime"=>text=Value::Null,"time"=>text=Value::Null,"inheritsFrom"=>text=Value::Null,"patched_client"=>text=Value::Null);
pub fn metadata(bytes: &[u8]) -> Result<Value, Error> {
    normalize(bytes, metadata_value)
}
fields!(fabric_library,"name"=>text,"url"=>text,"sha1"=>text=Value::Null,"size"=>u64v=Value::Null);
fn fabric_libraries(v: &Value) -> Result<Value, Error> {
    array(v, fabric_library)
}
fields!(fabric_profile_value,"id"=>text,"inheritsFrom"=>text,"mainClass"=>text,"libraries"=>fabric_libraries,"arguments"=>arguments=Value::Null,"releaseTime"=>text=Value::Null,"time"=>text=Value::Null,"type"=>text=Value::Null);
pub fn fabric_profile(bytes: &[u8]) -> Result<Value, Error> {
    normalize(bytes, fabric_profile_value)
}
fields!(neo_data_value,"client"=>text,"server"=>text=Value::Null);
fn neo_data(v: &Value) -> Result<Value, Error> {
    mapping(v, neo_data_value)
}
fields!(neo_processor,"jar"=>text,"sides"=>strings=Value::Null,"classpath"=>strings=Value::Array(vec![]),"args"=>strings=Value::Array(vec![]));
fn neo_processors(v: &Value) -> Result<Value, Error> {
    array(v, neo_processor)
}
fields!(neo_install,"spec"=>u32v,"version"=>text,"minecraft"=>text,"data"=>neo_data,"processors"=>neo_processors=Value::Array(vec![]),"libraries"=>libraries=Value::Array(vec![]));
fields!(neo_version,"id"=>text,"inheritsFrom"=>text,"mainClass"=>text,"type"=>text=Value::Null,"time"=>text=Value::Null,"releaseTime"=>text=Value::Null,"arguments"=>arguments=Value::Null,"minecraftArguments"=>text=Value::Null,"libraries"=>libraries=Value::Array(vec![]));
pub fn neoforge_install(bytes: &[u8]) -> Result<Value, Error> {
    normalize(bytes, neo_install)
}
pub fn neoforge_version(bytes: &[u8]) -> Result<Value, Error> {
    normalize(bytes, neo_version)
}
fields!(forge_library,"name"=>text,"url"=>text=Value::Null,"checksums"=>strings=Value::Null,"serverreq"=>boolean=Value::Bool(false),"clientreq"=>boolean=Value::Bool(false));
fn forge_libraries(v: &Value) -> Result<Value, Error> {
    array(v, forge_library)
}
fields!(forge_install,"path"=>text,"minecraft"=>text,"filePath"=>text=Value::Null,"target"=>text=Value::Null,"version"=>text=Value::Null,"profileName"=>text=Value::Null);
fields!(forge_info,"id"=>text,"mainClass"=>text,"minecraftArguments"=>text,"type"=>text=Value::Null,"time"=>text=Value::Null,"releaseTime"=>text=Value::Null,"assets"=>text=Value::Null,"inheritsFrom"=>text=Value::Null,"jar"=>text=Value::Null,"minimumLauncherVersion"=>u32v=Value::Null,"libraries"=>forge_libraries=Value::Array(vec![]));
fields!(forge_legacy_value,"install"=>forge_install,"versionInfo"=>forge_info);
pub fn forge_legacy(bytes: &[u8]) -> Result<Value, Error> {
    normalize(bytes, forge_legacy_value)
}
fields!(asset,"hash"=>text,"size"=>u64v);
fn objects(v: &Value) -> Result<Value, Error> {
    mapping(v, asset)
}
fields!(asset_index_value,"objects"=>objects,"virtual"=>boolean=Value::Bool(false),"map_to_resources"=>boolean=Value::Bool(false));
pub fn asset_index(bytes: &[u8]) -> Result<Value, Error> {
    normalize(bytes, asset_index_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalized_defaults_and_unknown_fields() {
        let v=metadata(br#"{"id":"x","type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"sha1":"a","size":"12","url":"https://x"}},"unknown":true}"#).unwrap();
        assert_eq!(v["downloads"]["client"]["size"], 12);
        assert!(v.get("unknown").is_none());
        assert!(v["arguments"].is_null());
        assert_eq!(v.as_object().unwrap().len(), 17);
    }
    #[test]
    fn nested_arguments_normalize() {
        let v = arguments(&serde_json::json!({"game":["literal",{"value":["a","b"]}]})).unwrap();
        assert_eq!(v["game"][1]["rules"], serde_json::json!([]));
        assert_eq!(v["jvm"], serde_json::json!([]));
        assert!(v["default-user-jvm"].is_null());
    }
    #[test]
    fn sha1_and_mirror() {
        assert!(verify_sha1(
            b"abc",
            "A9993E364706816ABA3E25717850C26C9CD0D89D"
        ));
        assert_eq!(
            Source::Bmclapi.rewrite("https://resources.download.minecraft.net/ab/cd"),
            Some("https://bmclapi2.bangbang93.com/assets/ab/cd".into())
        );
        assert_eq!(Source::Official.rewrite("https://example.invalid"), None);
    }
    #[test]
    fn duplicate_known_fields_rejected_unknown_ignored() {
        assert_eq!(
            manifest(br#"{"latest":{"release":"a","snapshot":"b"},"versions":[],"versions":[]}"#),
            Err(Error::Json("DuplicateField"))
        );
        assert!(manifest(br#"{"latest":{"release":"a","snapshot":"b"},"versions":[],"unknown":1,"unknown":2}"#).is_ok());
    }
    #[test]
    fn metadata_parse_errors_keep_zig_categories() {
        assert_eq!(metadata(b"{}"), Err(Error::Json("MissingField")));
        assert_eq!(metadata(b"[]"), Err(Error::Json("UnexpectedToken")));
        assert_eq!(u8v(&serde_json::json!(256)), Err(Error::Json("Overflow")));
        assert_eq!(
            u8v(&serde_json::json!(1.5)),
            Err(Error::Json("InvalidNumber"))
        );
        assert_eq!(
            version_type(&serde_json::json!("unknown")),
            Err(Error::Json("InvalidEnumTag"))
        );
    }
    #[test]
    fn numbers_reject_out_of_range() {
        assert!(u8v(&serde_json::json!(256)).is_err());
        assert!(u64v(&serde_json::json!(18446744073709551616.0)).is_err());
        assert!(u64v(&serde_json::json!(-1)).is_err());
    }
}
