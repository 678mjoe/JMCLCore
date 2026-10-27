//! Private in-memory representation. Not accepted by acquisition or materialization.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct PlanBytes(pub(super) Vec<u8>);
impl From<String> for PlanBytes {
    fn from(s: String) -> Self {
        Self(s.into_bytes())
    }
}
impl From<&str> for PlanBytes {
    fn from(s: &str) -> Self {
        Self(s.as_bytes().to_vec())
    }
}
impl Serialize for PlanBytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match std::str::from_utf8(&self.0) {
            Ok(s) => serializer.serialize_str(s),
            Err(_) => self.0.serialize(serializer),
        }
    }
}
impl PlanBytes {
    // Used only by String entry points or paths derived from validated nested Strings.
    pub(super) fn into_string(self) -> String {
        String::from_utf8(self.0).expect("String planning entry preserves UTF-8 paths")
    }
    pub(super) fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("nested metadata path remains a String")
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct ReadFile {
    pub path: PlanBytes,
    pub url: String,
    pub fallback_url: Option<String>,
    pub sha1: String,
    pub size: u64,
    pub usage: Usage,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ReadPlan {
    pub(super) version_id: PlanBytes,
    pub(super) source: Source,
    pub(super) target: Target,
    pub(super) files: Vec<ReadFile>,
    pub(super) classpath: Vec<PlanBytes>,
    pub(super) native_extractions: Vec<NativeExtraction>,
    pub(super) asset_projections: Vec<AssetProjection>,
    pub(super) total_download_size: u64,
    pub(super) java_major_version: u16,
    pub(super) loader: String,
}
impl ReadPlan {
    pub(super) fn into_string(self) -> Plan {
        Plan {
            version_id: self.version_id.into_string(),
            source: self.source,
            target: self.target,
            files: self
                .files
                .into_iter()
                .map(|f| File {
                    path: f.path.into_string(),
                    url: f.url,
                    fallback_url: f.fallback_url,
                    sha1: f.sha1,
                    size: f.size,
                    usage: f.usage,
                })
                .collect(),
            classpath: self
                .classpath
                .into_iter()
                .map(PlanBytes::into_string)
                .collect(),
            native_extractions: self.native_extractions,
            asset_projections: self.asset_projections,
            total_download_size: self.total_download_size,
            java_major_version: self.java_major_version,
            loader: self.loader,
        }
    }
}
