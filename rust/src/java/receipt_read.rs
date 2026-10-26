//! Listing-only persisted byte view. Operational Receipt parsing is unchanged.
use super::*;
use serde_json::value::RawValue;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::java) struct ReceiptBytes(pub Vec<u8>);
impl<'de> Deserialize<'de> for ReceiptBytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = <&RawValue>::deserialize(d)?;
        if raw.get().starts_with('"') {
            return serde_json::from_str::<String>(raw.get())
                .map(|s| Self(s.into_bytes()))
                .map_err(serde::de::Error::custom);
        }
        let values: Vec<&RawValue> =
            serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
        values
            .into_iter()
            .map(|v| {
                crate::json_number::typed_raw(v.get(), 8)
                    .map(|n| n as u8)
                    .map_err(|e| serde::de::Error::custom(e.name()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }
}
impl Serialize for ReceiptBytes {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match std::str::from_utf8(&self.0) {
            Ok(text) => s.serialize_str(text),
            Err(_) => self.0.serialize(s),
        }
    }
}
fn read_provider<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Provider, D::Error> {
    let raw = <&RawValue>::deserialize(d)?;
    let token = raw.get();
    let value = if token.starts_with('"') {
        serde_json::from_str::<String>(token).map_err(serde::de::Error::custom)?
    } else if token.starts_with('-') || token.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        token.to_owned()
    } else {
        return Err(serde::de::Error::custom("UnexpectedToken"));
    };
    match value.as_str() {
        "mojang" => return Ok(Provider::Mojang),
        "zulu" => return Ok(Provider::Zulu),
        _ => {}
    }
    if value != "-0"
        && !value.bytes().any(|b| matches!(b, b'.' | b'e' | b'E'))
        && let Ok(n) = crate::json_number::unsigned_text(&value, 1)
    {
        return Ok(if n == 0 {
            Provider::Mojang
        } else {
            Provider::Zulu
        });
    }
    Err(serde::de::Error::custom("InvalidEnumTag"))
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(in crate::java) struct ReadReceipt {
    #[serde(default = "schema", deserialize_with = "persisted_u32")]
    pub schema_version: u32,
    #[serde(deserialize_with = "read_provider")]
    pub provider: Provider,
    pub id: ReceiptBytes,
    pub platform: ReceiptBytes,
    #[serde(deserialize_with = "persisted_u16")]
    pub major_version: u16,
    pub java_home: ReceiptBytes,
    pub executable: ReceiptBytes,
    pub version: Option<ReceiptBytes>,
    pub vendor: Option<ReceiptBytes>,
    pub component: Option<ReceiptBytes>,
    pub package_uuid: Option<ReceiptBytes>,
    pub package_name: Option<ReceiptBytes>,
    pub source_url: ReceiptBytes,
    #[serde(default, deserialize_with = "persisted_u32")]
    pub files_total: u32,
    #[serde(default, deserialize_with = "persisted_u64")]
    pub bytes_total: u64,
    #[serde(deserialize_with = "persisted_i64")]
    pub installed_at_ms: i64,
}

fn relative(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len() <= 1024
        && !bytes.starts_with(b"/")
        && bytes.split(|b| *b == b'/').all(|p| {
            !p.is_empty()
                && p != b"."
                && p != b".."
                && !p.iter().any(|b| matches!(b, 0 | b'\\' | b':'))
        })
}
impl ReadReceipt {
    fn validate(&self) -> Result<(), Error> {
        if self.schema_version != 1 {
            return Err("UnsupportedReceiptSchema");
        }
        if !std::str::from_utf8(&self.id.0).is_ok_and(valid_id) {
            return Err("InvalidRuntimeId");
        }
        if !std::str::from_utf8(&self.platform.0).is_ok_and(|p| platform_target(p).is_some()) {
            return Err("InvalidRuntimePlatform");
        }
        if self.major_version == 0 {
            return Err("InvalidRuntimeReceipt");
        }
        if !relative(&self.executable.0)
            || (!self.java_home.0.is_empty() && !relative(&self.java_home.0))
        {
            return Err("InvalidRelativePath");
        }
        if self.source_url.0.len() > 2048 || !self.source_url.0.starts_with(b"https://") {
            return Err("InvalidRuntimeReceipt");
        }
        Ok(())
    }
    /// RPC deliberately omits schema_version/package_name, unlike disk format.
    pub fn rpc(&self, name: &str) -> Value {
        serde_json::json!({"name":name,"provider":self.provider,"id":self.id,"platform":self.platform,"major_version":self.major_version,"java_home":self.java_home,"executable":self.executable,"version":self.version,"vendor":self.vendor,"component":self.component,"package_uuid":self.package_uuid,"source_url":self.source_url,"files_total":self.files_total,"bytes_total":self.bytes_total,"installed_at_ms":self.installed_at_ms})
    }
    pub fn cli(&self, name: &str) -> Value {
        serde_json::json!({"name":name,"provider":self.provider,"id":self.id,"platform":self.platform,"major_version":self.major_version,"executable":self.executable,"version":self.version,"vendor":self.vendor})
    }
}
pub(in crate::java) fn parse_read(bytes: &[u8]) -> Result<ReadReceipt, Error> {
    if bytes.len() > MAX_BYTES || bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return Err("InvalidRuntimeReceipt");
    }
    // Ignored fields still belong to a well-formed typed JSON document.
    // RawValue/serde's ignored visitor alone does not validate their strings.
    let text = std::str::from_utf8(bytes).map_err(|_| "InvalidRuntimeReceipt")?;
    crate::auth::json::Json::parse(text).map_err(|_| "InvalidRuntimeReceipt")?;
    let receipt: ReadReceipt =
        serde_json::from_slice(bytes).map_err(|_| "InvalidRuntimeReceipt")?;
    receipt.validate().map_err(|_| "InvalidRuntimeReceipt")?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        serde_json::json!({"provider":"mojang","id":"fixture","platform":"linux","major_version":17,"java_home":"","executable":"bin/absent","source_url":"https://example.invalid/metadata-only","installed_at_ms":123})
    }
    #[test]
    fn listing_bytes_preserve_operational_boundary_and_wire_shapes() {
        let mut v = fixture();
        let ordinary = serde_json::to_vec(&v).unwrap();
        let read = parse_read(&ordinary).unwrap();
        let operational = parse(&ordinary).unwrap();
        assert_eq!(
            read.rpc("directory-name"),
            operational.rpc("directory-name")
        );
        assert_eq!(
            read.cli("directory-name"),
            operational.cli("directory-name")
        );
        for field in ["id", "platform", "java_home", "executable", "source_url"] {
            v[field] = serde_json::to_value(v[field].as_str().unwrap().as_bytes()).unwrap();
        }
        v["vendor"] = serde_json::json!([255, 9, 10, 0]);
        v["provider"] = serde_json::json!(0);
        let raw = serde_json::to_vec(&v).unwrap();
        let read = parse_read(&raw).unwrap();
        assert_eq!(read.provider, Provider::Mojang);
        assert_eq!(read.rpc("directory-name")["vendor"], v["vendor"]);
        assert!(read.rpc("name").get("package_name").is_none());
        assert!(read.cli("name").get("source_url").is_none());
        assert!(parse(&raw).is_err()); // Shared managed-selection parser unchanged.
        assert!(serde_json::from_str::<Provider>("0").is_err());
        assert!(serde_json::from_str::<Provider>("\"0\"").is_err());
    }
    #[test]
    fn listing_typed_byte_precision_and_provider_grammar() {
        for good in ["[65]", "[65.0]", "[\"6_5\"]", "[\"65.0\"]"] {
            assert_eq!(serde_json::from_str::<ReceiptBytes>(good).unwrap().0, b"A");
        }
        for bad in [
            "[65.0000000000000001]",
            "[\"65.0000000000000001\"]",
            "[256]",
            "[-1]",
            "[true]",
            "[null]",
            "{}",
            "false",
        ] {
            assert!(serde_json::from_str::<ReceiptBytes>(bad).is_err(), "{bad}");
        }
        for (token, expected) in [
            ("0", Provider::Mojang),
            ("1", Provider::Zulu),
            ("\"0__1\"", Provider::Zulu),
            ("\"-00\"", Provider::Mojang),
        ] {
            let mut value = fixture();
            value.as_object_mut().unwrap().remove("provider");
            let raw = serde_json::to_string(&value).unwrap();
            let raw = format!("{},\"provider\":{token}}}", &raw[..raw.len() - 1]);
            assert_eq!(parse_read(raw.as_bytes()).unwrap().provider, expected);
        }
    }
    #[test]
    fn listing_validation_order_matches_normal_receipts() {
        let base = fixture();
        let changes = [
            (
                "schema_version",
                serde_json::json!(2),
                "UnsupportedReceiptSchema",
            ),
            ("id", serde_json::json!("bad/id"), "InvalidRuntimeId"),
            (
                "platform",
                serde_json::json!("unknown"),
                "InvalidRuntimePlatform",
            ),
            (
                "major_version",
                serde_json::json!(0),
                "InvalidRuntimeReceipt",
            ),
            (
                "executable",
                serde_json::json!("../absent"),
                "InvalidRelativePath",
            ),
            (
                "source_url",
                serde_json::json!("http://invalid"),
                "InvalidRuntimeReceipt",
            ),
        ];
        for (i, (_, _, expected)) in changes.iter().enumerate() {
            let mut v = base.clone();
            for (key, value, _) in &changes[i..] {
                v[*key] = value.clone();
            }
            let raw = serde_json::to_vec(&v).unwrap();
            let read: ReadReceipt = serde_json::from_slice(&raw).unwrap();
            let ordinary: Receipt = serde_json::from_slice(&raw).unwrap();
            assert_eq!(read.validate(), Err(*expected));
            assert_eq!(read.validate(), ordinary.validate());
        }
        for path in [
            "",
            "/a",
            "a//b",
            "a/./b",
            "a/../b",
            "a\\b",
            "a:b",
            "a\0b",
            "bin/absent",
            "jre.bundle/Contents/Home/bin/absent",
        ] {
            assert_eq!(relative(path.as_bytes()), valid_relative(path), "{path:?}");
        }
        assert!(relative(b"bin/\xff")); // Metadata validation only: never opened.
    }
}
