//! Pure listing view and formatter; acquisition retains its String schema.
use super::*;

pub struct Listing(Value);
impl Listing {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        normalize(bytes, listing_manifest).map(Self)
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
