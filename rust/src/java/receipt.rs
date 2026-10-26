use super::detect::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
pub const NAME: &str = ".jmcl-runtime.json";
pub const MAX_BYTES: usize = 16 * 1024;
pub const MAX_RUNTIMES: usize = 256;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Mojang,
    Zulu,
}
fn schema() -> u32 {
    1
}
macro_rules! number {
    ($name:ident,$ty:ty) => {
        pub(super) fn $name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<$ty, D::Error> {
            let v = Value::deserialize(d)?;
            let parsed = match &v {
                Value::String(s) => s.parse::<$ty>().ok(),
                Value::Number(n) => n
                    .as_i64()
                    .and_then(|n| <$ty>::try_from(n).ok())
                    .or_else(|| n.as_u64().and_then(|n| <$ty>::try_from(n).ok()))
                    .or_else(|| {
                        n.as_f64()
                            .filter(|n| {
                                n.is_finite()
                                    && n.fract() == 0.0
                                    && *n >= <$ty>::MIN as f64
                                    && *n <= <$ty>::MAX as f64
                            })
                            .and_then(|n| format!("{n:.0}").parse::<$ty>().ok())
                    }),
                _ => None,
            };
            parsed.ok_or_else(|| serde::de::Error::custom("invalid integer"))
        }
    };
}
// This existing helper is also consumed by runtime_plan. Keep its semantics
// isolated from persisted receipt parsing; acquisition/planning is not changed.
number!(de_u64, u64);
fn persisted_unsigned<'de, D: serde::Deserializer<'de>>(
    d: D,
    width: usize,
) -> Result<u64, D::Error> {
    let raw = <&serde_json::value::RawValue>::deserialize(d)?;
    crate::json_number::typed_raw(raw.get(), width).map_err(|e| serde::de::Error::custom(e.name()))
}
fn persisted_u16<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    persisted_unsigned(d, 16).map(|n| n as u16)
}
fn persisted_u32<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    persisted_unsigned(d, 32).map(|n| n as u32)
}
fn persisted_u64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    persisted_unsigned(d, 64)
}
fn persisted_i64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let raw = <&serde_json::value::RawValue>::deserialize(d)?;
    let token = raw.get();
    let decoded;
    let text = if token.starts_with('"') {
        decoded = crate::auth::json::Json(token)
            .string()
            .map_err(|_| serde::de::Error::custom("InvalidCharacter"))?;
        decoded.as_str()
    } else if token
        .as_bytes()
        .first()
        .is_some_and(|b| b.is_ascii_digit() || *b == b'-')
    {
        token
    } else {
        return Err(serde::de::Error::custom("UnexpectedToken"));
    };
    let negative = text.starts_with('-');
    let magnitude = text.strip_prefix(['+', '-']).unwrap_or(text);
    if magnitude.starts_with(['+', '-']) {
        return Err(serde::de::Error::custom("InvalidCharacter"));
    }
    let n = crate::json_number::unsigned_text(magnitude, 64)
        .map_err(|e| serde::de::Error::custom(e.name()))?;
    if negative && n == 1u64 << 63 {
        return Ok(i64::MIN);
    }
    let n = i64::try_from(n).map_err(|_| serde::de::Error::custom("Overflow"))?;
    Ok(if negative { -n } else { n })
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Receipt {
    #[serde(default = "schema", deserialize_with = "persisted_u32")]
    pub schema_version: u32,
    pub provider: Provider,
    pub id: String,
    pub platform: String,
    #[serde(deserialize_with = "persisted_u16")]
    pub major_version: u16,
    pub java_home: String,
    pub executable: String,
    pub version: Option<String>,
    pub vendor: Option<String>,
    pub component: Option<String>,
    pub package_uuid: Option<String>,
    pub package_name: Option<String>,
    pub source_url: String,
    #[serde(default, deserialize_with = "persisted_u32")]
    pub files_total: u32,
    #[serde(default, deserialize_with = "persisted_u64")]
    pub bytes_total: u64,
    #[serde(deserialize_with = "persisted_i64")]
    pub installed_at_ms: i64,
}
pub fn platform_target(platform: &str) -> Option<(&'static str, &'static str)> {
    match platform {
        "windows-x64" => Some(("windows", "x86_64")),
        "windows-x86" => Some(("windows", "x86")),
        "windows-arm64" => Some(("windows", "arm64")),
        "mac-os" => Some(("osx", "x86_64")),
        "mac-os-arm64" => Some(("osx", "arm64")),
        "linux" => Some(("linux", "x86_64")),
        "linux-i386" => Some(("linux", "x86")),
        "linux-arm64" => Some(("linux", "arm64")),
        _ => None,
    }
}
pub fn platform_name(os: &str, arch: &str) -> Option<&'static str> {
    [
        "windows-x64",
        "windows-x86",
        "windows-arm64",
        "mac-os",
        "mac-os-arm64",
        "linux",
        "linux-i386",
        "linux-arm64",
    ]
    .into_iter()
    .find(|v| platform_target(v) == Some((os, arch)))
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}
pub fn valid_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1024
        && !Path::new(path).is_absolute()
        && path.split('/').all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && !p.bytes().any(|b| b == 0 || b == b'\\' || b == b':')
        })
}
impl Receipt {
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema_version != 1 {
            return Err("UnsupportedReceiptSchema");
        };
        if !valid_id(&self.id) {
            return Err("InvalidRuntimeId");
        };
        if platform_target(&self.platform).is_none() {
            return Err("InvalidRuntimePlatform");
        };
        if self.major_version == 0 {
            return Err("InvalidRuntimeReceipt");
        };
        if !valid_relative(&self.executable)
            || (!self.java_home.is_empty() && !valid_relative(&self.java_home))
        {
            return Err("InvalidRelativePath");
        };
        if self.source_url.is_empty()
            || self.source_url.len() > 2048
            || !self.source_url.starts_with("https://")
        {
            return Err("InvalidRuntimeReceipt");
        };
        Ok(())
    }
    pub fn runtime_name(&self) -> Result<String, Error> {
        self.validate()?;
        Ok(format!(
            "{}-{}-{}",
            match self.provider {
                Provider::Mojang => "mojang",
                Provider::Zulu => "zulu",
            },
            self.id,
            self.platform
        ))
    }
    /// RPC deliberately omits schema_version/package_name, unlike disk format.
    pub fn rpc(&self, name: &str) -> Value {
        serde_json::json!({"name":name,"provider":self.provider,"id":self.id,"platform":self.platform,"major_version":self.major_version,"java_home":self.java_home,"executable":self.executable,"version":self.version,"vendor":self.vendor,"component":self.component,"package_uuid":self.package_uuid,"source_url":self.source_url,"files_total":self.files_total,"bytes_total":self.bytes_total,"installed_at_ms":self.installed_at_ms})
    }
    pub fn cli(&self, name: &str) -> Value {
        serde_json::json!({"name":name,"provider":self.provider,"id":self.id,"platform":self.platform,"major_version":self.major_version,"executable":self.executable,"version":self.version,"vendor":self.vendor})
    }
}
pub fn parse(bytes: &[u8]) -> Result<Receipt, Error> {
    if bytes.len() > MAX_BYTES || bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return Err("InvalidRuntimeReceipt");
    };
    let receipt: Receipt = serde_json::from_slice(bytes).map_err(|_| "InvalidRuntimeReceipt")?;
    receipt.validate().map_err(|_| "InvalidRuntimeReceipt")?;
    Ok(receipt)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> &'static str {
        r#"{"provider":"mojang","id":"java-runtime-gamma","platform":"linux","major_version":17,"java_home":"","executable":"bin/java","source_url":"https://example.invalid/runtime","installed_at_ms":123}"#
    }
    #[test]
    fn defaults_and_roundtrip() {
        let r = parse(fixture().as_bytes()).unwrap();
        assert_eq!(r.runtime_name().unwrap(), "mojang-java-runtime-gamma-linux");
        assert_eq!(r.files_total, 0);
        assert_eq!(parse(&serde_json::to_vec(&r).unwrap()).unwrap(), r);
        assert!(r.rpc("test").get("package_name").is_none());
    }
    #[test]
    fn malformed_limits() {
        for change in [
            fixture().replace("bin/java", "../java"),
            fixture().replace("https://", "http://"),
            fixture().replace("\"major_version\":17", "\"major_version\":0"),
            fixture().replace("\"linux\"", "\"linux-weird\""),
            fixture().replace("\"provider\":\"mojang\"", "\"provider\":\"other\""),
        ] {
            assert!(parse(change.as_bytes()).is_err())
        }
        assert!(parse(&vec![b' '; MAX_BYTES + 1]).is_err());
    }
    #[test]
    fn platforms_and_paths() {
        for p in [
            "windows-x64",
            "windows-x86",
            "windows-arm64",
            "mac-os",
            "mac-os-arm64",
            "linux",
            "linux-i386",
            "linux-arm64",
        ] {
            let (o, a) = platform_target(p).unwrap();
            assert_eq!(platform_name(o, a), Some(p))
        }
        for p in ["", "../a", "/a", "a//b", "a\\b", "a:b", "a/./b"] {
            assert!(!valid_relative(p))
        }
        assert!(valid_relative("jre.bundle/Contents/Home/bin/java"));
        assert!(!valid_name(".hidden"));
        assert!(valid_id(".."));
    }
    #[test]
    fn legacy_numeric_coercion() {
        let r = parse(
            fixture()
                .replace("\"major_version\":17", "\"major_version\":\"17\"")
                .as_bytes(),
        )
        .unwrap();
        assert_eq!(r.major_version, 17);
    }
}
