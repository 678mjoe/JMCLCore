//! Private typed wire schema; constrained bytes are validated before conversion.
use super::*;
#[derive(Deserialize)]
pub(super) struct WireProject {
    pub id: MetadataBytes,
    pub slug: MetadataBytes,
    #[serde(default)]
    pub title: Option<MetadataBytes>,
    pub project_type: MetadataBytes,
    #[serde(default)]
    pub loaders: Vec<MetadataBytes>,
}
#[derive(Deserialize)]
pub(super) struct WireHashes {
    pub sha1: MetadataBytes,
    #[serde(default)]
    pub sha512: Option<MetadataBytes>,
}
#[derive(Deserialize)]
pub(super) struct WireFile {
    #[serde(deserialize_with = "string")]
    pub url: String,
    #[serde(deserialize_with = "string")]
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    pub hashes: WireHashes,
    #[serde(deserialize_with = "number")]
    pub size: u64,
}
#[derive(Deserialize)]
pub(super) struct WireDependency {
    #[serde(default)]
    pub version_id: Option<MetadataBytes>,
    #[serde(default)]
    pub project_id: Option<MetadataBytes>,
    #[serde(default)]
    pub file_name: Option<MetadataBytes>,
    pub dependency_type: DependencyType,
}
#[derive(Deserialize)]
pub(super) struct WireVersion {
    pub id: MetadataBytes,
    pub project_id: MetadataBytes,
    pub version_number: MetadataBytes,
    pub version_type: MetadataBytes,
    pub game_versions: Vec<MetadataBytes>,
    pub loaders: Vec<MetadataBytes>,
    pub files: Vec<WireFile>,
    #[serde(default)]
    pub dependencies: Vec<WireDependency>,
}
// Every caller validates source ASCII/enum/hex constraints first; conversion is
// nevertheless checked, never lossy, and cannot create an unchecked String.
fn text(v: MetadataBytes) -> Result<String> {
    String::from_utf8(v.into_bytes()).map_err(|_| Error("UnexpectedToken".into()))
}
fn texts(v: Vec<MetadataBytes>) -> Result<Vec<String>> {
    v.into_iter().map(text).collect()
}
impl WireProject {
    pub(super) fn into_public(self) -> Result<Project> {
        Ok(Project {
            id: text(self.id)?,
            slug: text(self.slug)?,
            title: self.title,
            project_type: text(self.project_type)?,
            loaders: texts(self.loaders)?,
        })
    }
}
impl WireVersion {
    pub(super) fn into_public(self) -> Result<Version> {
        Ok(Version {
            id: text(self.id)?,
            project_id: text(self.project_id)?,
            version_number: self.version_number,
            version_type: text(self.version_type)?,
            game_versions: texts(self.game_versions)?,
            loaders: texts(self.loaders)?,
            files: self
                .files
                .into_iter()
                .map(|f| {
                    Ok(File {
                        url: f.url,
                        filename: f.filename,
                        primary: f.primary,
                        hashes: Hashes {
                            sha1: text(f.hashes.sha1)?,
                            sha512: f.hashes.sha512,
                        },
                        size: f.size,
                    })
                })
                .collect::<Result<_>>()?,
            dependencies: self
                .dependencies
                .into_iter()
                .map(|d| {
                    Ok(Dependency {
                        version_id: d.version_id.map(text).transpose()?,
                        project_id: d.project_id.map(text).transpose()?,
                        file_name: d.file_name,
                        dependency_type: d.dependency_type,
                    })
                })
                .collect::<Result<_>>()?,
        })
    }
}
