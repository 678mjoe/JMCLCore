//! Pure listing view and formatter; acquisition retains its String schema.
use super::*;

pub struct Listing(Value);
impl Listing {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        normalize(bytes, listing_manifest).map(Self)
    }
}
/// A selected entry retains bytes; conversion to transport text belongs to its caller.
#[derive(Debug, PartialEq)]
pub(crate) struct MetadataEntry<'a> {
    pub id: std::borrow::Cow<'a, [u8]>,
    pub url: std::borrow::Cow<'a, [u8]>,
    pub sha1: Option<std::borrow::Cow<'a, [u8]>>,
}
fn bytes(value: &Value) -> std::borrow::Cow<'_, [u8]> {
    match value.as_str() {
        Some(s) => std::borrow::Cow::Borrowed(s.as_bytes()),
        None => std::borrow::Cow::Owned(
            value
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b.as_u64().unwrap() as u8)
                .collect(),
        ),
    }
}
impl Listing {
    /// Parsing and validation of the entire manifest have already succeeded.
    /// Duplicate ID values are allowed: the first match wins, as in Zig.
    pub(crate) fn lookup_metadata_entry(&self, id: &[u8]) -> Result<MetadataEntry<'_>, Error> {
        let selected = self.0["versions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| bytes(&entry["id"]).as_ref() == id)
            .ok_or(Error::VersionNotFound)?;
        Ok(MetadataEntry {
            id: bytes(&selected["id"]),
            url: bytes(&selected["url"]),
            sha1: (!selected["sha1"].is_null()).then(|| bytes(&selected["sha1"])),
        })
    }
}
// Reuse the metadata lexeme tree, duplicate tracking and typed integer parser.
// Every []const u8 in the pinned listing schema has a string-or-array view.
fn byte_text(v: &Node) -> Result<Value, Error> {
    if v.is_string() {
        return text(v);
    }
    let bytes = v
        .as_array()
        .ok_or(Error::Json("UnexpectedToken"))?
        .iter()
        .map(|n| typed_unsigned(n, 8).map(|n| n as u8))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(match String::from_utf8(bytes) {
        Ok(s) => Value::String(s),
        Err(e) => Value::Array(e.into_bytes().into_iter().map(Value::from).collect()),
    })
}
fields!(listing_latest,"release"=>byte_text,"snapshot"=>byte_text);
fields!(listing_version,"id"=>byte_text,"type"=>version_type,"url"=>byte_text,"time"=>byte_text,"releaseTime"=>byte_text,"sha1"=>byte_text=Value::Null,"complianceLevel"=>u8v=Value::Null);
fn listing_versions(v: &Node) -> Result<Value, Error> {
    array(v, listing_version)
}
fields!(listing_manifest,"latest"=>listing_latest,"versions"=>listing_versions);
impl Listing {
    /// Matches writeSelection; kind and CLI limit validation remain at the caller.
    pub fn selection(&self, kind: Option<&str>, limit: usize) -> Value {
        super::select(&self.0, kind, limit)
    }
    /// Pure CLI formatting: no cache, Service, network or acquisition access.
    pub fn write(
        &self,
        out: &mut impl std::io::Write,
        source: Source,
        kind: Option<&str>,
        limit: usize,
        json: bool,
    ) -> std::io::Result<()> {
        let result = self.selection(kind, limit);
        if json {
            #[derive(Serialize)]
            struct Document {
                protocol: u32,
                source: Source,
                result: Value,
            }
            serde_json::to_writer(
                &mut *out,
                &Document {
                    protocol: 1,
                    source,
                    result,
                },
            )?;
            return out.write_all(b"\n");
        }
        if source == Source::Bmclapi {
            out.write_all(b"# source\tBMCLAPI\n")?;
        }
        for v in result["versions"].as_array().unwrap() {
            write_bytes(out, &v["id"])?;
            out.write_all(b"\t")?;
            write_bytes(out, &v["type"])?;
            out.write_all(b"\t")?;
            write_bytes(out, &v["releaseTime"])?;
            out.write_all(b"\n")?;
        }
        Ok(())
    }
}
fn write_bytes(out: &mut impl std::io::Write, value: &Value) -> std::io::Result<()> {
    if let Some(s) = value.as_str() {
        return out.write_all(s.as_bytes());
    }
    for b in value.as_array().unwrap() {
        out.write_all(&[b.as_u64().unwrap() as u8])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const INPUT: &[u8] = br#"{"latest":{"release":[255],"snapshot":[115]},"versions":[{"id":[255],"type":"release","url":"u","time":"t","releaseTime":"r"}]}"#;
    #[test]
    fn opaque_ids_are_preserved_without_widening_acquisition() {
        let list = Listing::parse(INPUT).unwrap();
        assert_eq!(list.0["latest"]["release"], serde_json::json!([255]));
        assert_eq!(list.0["versions"][0]["id"], serde_json::json!([255]));
        assert!(super::super::manifest(INPUT).is_err());
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;
    use serde_json::json;
    fn input() -> Vec<u8> {
        serde_json::to_vec(&json!({"latest":{"release":[255],"snapshot":[115]},"versions":[
            {"id":[255],"type":"release","url":[254],"time":[253],"releaseTime":[252],"sha1":[251]},
            {"id":"snap","type":"snapshot","url":"u","time":"t","releaseTime":"s"},
            {"id":[114],"type":"release","url":"u","time":"t","releaseTime":"r"}
        ]})).unwrap()
    }
    #[test]
    fn selection_preserves_order_filter_count_and_optional_fields() {
        let list = Listing::parse(&input()).unwrap();
        let selected = list.selection(Some("release"), 2);
        assert_eq!(selected["latest"], json!({"release":[255],"snapshot":"s"}));
        assert_eq!(selected["versions"].as_array().unwrap().len(), 2);
        assert_eq!(selected["versions"][0]["id"], json!([255]));
        assert_eq!(selected["versions"][1]["id"], "r");
        assert_eq!(selected["versions"][0]["url"], json!([254]));
        assert_eq!(selected["versions"][0]["sha1"], json!([251]));
        assert_eq!(selected["versions"][1]["sha1"], Value::Null);
        assert_eq!(selected["versions"][1]["complianceLevel"], Value::Null);
        assert_eq!(list.selection(None, 0)["versions"], json!([]));
        assert_eq!(
            list.selection(Some("release"), 1)["versions"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            list.selection(None, usize::MAX)["versions"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            list.selection(Some("snapshot"), 1)["versions"][0]["id"],
            "snap"
        );
    }
    #[test]
    fn formatter_preserves_raw_bytes_json_arrays_and_source_notice() {
        let list = Listing::parse(&input()).unwrap();
        for source in [Source::Official, Source::Bmclapi] {
            let mut raw = Vec::new();
            list.write(&mut raw, source, Some("release"), 1, false)
                .unwrap();
            let mut expected = if source == Source::Bmclapi {
                b"# source\tBMCLAPI\n".to_vec()
            } else {
                vec![]
            };
            expected.extend_from_slice(b"\xff\trelease\t\xfc\n");
            assert_eq!(raw, expected);
            let mut encoded = Vec::new();
            list.write(&mut encoded, source, Some("release"), 1, true)
                .unwrap();
            assert_eq!(encoded.last(), Some(&b'\n'));
            assert_eq!(
                serde_json::from_slice::<Value>(&encoded).unwrap(),
                json!({"protocol":1,"source":source,"result":list.selection(Some("release"),1)})
            );
        }
    }
    #[test]
    fn listing_reuses_required_field_enum_and_duplicate_guards() {
        for (input, error) in [
            (br#"{"latest":{"release":[255],"snapshot":"s"}}"#.as_slice(), "MissingField"),
            (br#"{"latest":{"release":[255],"release":[254],"snapshot":"s"},"versions":[]}"#, "DuplicateField"),
            (br#"{"latest":{"release":"r","snapshot":"s"},"versions":[{"id":[255],"type":"invalid","url":"u","time":"t","releaseTime":"r"}]}"#, "InvalidEnumTag"),
        ] {
            assert_eq!(Listing::parse(input).err(), Some(Error::Json(error)));
        }
    }
}

#[cfg(test)]
mod lookup_tests {
    use super::*;
    use serde_json::json;
    fn entry(id: Value, url: Value, sha1: Value) -> Value {
        json!({"id":id,"type":"release","url":url,"sha1":sha1,"time":"t","releaseTime":"t"})
    }
    fn input(entries: Vec<Value>) -> Vec<u8> {
        serde_json::to_vec(&json!({"latest":{"release":"demo","snapshot":"s"},"versions":entries}))
            .unwrap()
    }
    #[test]
    fn ordinary_lookup_is_not_rejected_by_unrelated_opaque_entries() {
        let bytes = input(vec![
            entry(json!([255]), json!([254]), json!([253])),
            entry(
                json!("demo"),
                json!("https://example.invalid/meta"),
                Value::Null,
            ),
            entry(json!([252]), json!([251]), Value::Null),
        ]);
        assert!(super::super::manifest(&bytes).is_err()); // Legacy API remains unchanged.
        let parsed = Listing::parse(&bytes).unwrap();
        let selected = parsed.lookup_metadata_entry(b"demo").unwrap();
        assert_eq!(selected.id.as_ref(), b"demo");
        assert_eq!(selected.url.as_ref(), b"https://example.invalid/meta");
        assert_eq!(selected.sha1, None);
    }
}

#[cfg(test)]
mod lookup_order_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn lookup_preserves_raw_fields_first_duplicate_and_absence() {
        let input = json!({"latest":{"release":[255],"snapshot":"s"},"versions":[
            {"id":[255],"type":"release","url":[254],"sha1":[253],"time":"t","releaseTime":"r"},
            {"id":[255],"type":"release","url":"second","sha1":null,"time":"t","releaseTime":"r"}
        ]});
        let listing = Listing::parse(input.to_string().as_bytes()).unwrap();
        let entry = listing.lookup_metadata_entry(&[255]).unwrap();
        assert_eq!(entry.id.as_ref(), &[255]);
        assert_eq!(entry.url.as_ref(), &[254]);
        assert_eq!(entry.sha1.as_deref(), Some([253].as_slice()));
        assert_eq!(
            listing.lookup_metadata_entry(b"absent"),
            Err(Error::VersionNotFound)
        );
    }
    #[test]
    fn full_manifest_errors_precede_found_or_missing_lookup() {
        for id in ["demo", "absent"] {
            let input=br#"{"latest":{"release":"demo","snapshot":"s"},"versions":[{"id":"demo","type":"release","url":"u","time":"t","releaseTime":"r"},{"id":[255],"type":"invalid","url":"u","time":"t","releaseTime":"r"}]}"#;
            let result = Listing::parse(input)
                .and_then(|m| m.lookup_metadata_entry(id.as_bytes()).map(|_| ()));
            assert_eq!(result, Err(Error::Json("InvalidEnumTag")));
        }
    }
}
