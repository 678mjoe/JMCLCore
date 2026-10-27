//! Private byte plan. Public String APIs remain unchanged; guarded ordinary completion is internal.
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

/// A capability for ordinary files only; selection is pure and precedes roots.
pub(crate) struct OrdinaryFiles {
    files: Vec<crate::install::ByteFile>,
    total: u64,
}
impl ReadPlan {
    pub(crate) fn ordinary_files(&self) -> Option<OrdinaryFiles> {
        if self.loader != "none"
            || !self.native_extractions.is_empty()
            || !self.asset_projections.is_empty()
        {
            return None;
        }
        Some(OrdinaryFiles {
            files: self
                .files
                .iter()
                .map(|f| crate::install::ByteFile {
                    path: crate::install::BytePath(f.path.0.clone()),
                    url: f.url.clone(),
                    fallback_url: f.fallback_url.clone(),
                    sha1: f.sha1.clone(),
                    size: f.size,
                })
                .collect(),
            total: self.total_download_size,
        })
    }
}
impl OrdinaryFiles {
    pub(crate) fn execute<T: crate::install::Transport>(
        &self,
        destination: &crate::store::ArtifactRoot,
        store: &crate::store::Store,
        options: crate::install::Options,
        transport: &T,
        reporter: Option<
            &(dyn Fn(crate::install::ByteEvent) -> Result<(), crate::install::Error> + Sync),
        >,
    ) -> Result<crate::install::Summary, crate::install::Error> {
        crate::install::execute_bytes(
            destination,
            store,
            &self.files,
            self.total,
            options,
            transport,
            reporter,
        )
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "plan_ordinary_tests.rs"]
mod ordinary_tests;

#[path = "plan_completion.rs"]
mod completion;
pub(crate) use completion::{
    CompletionInfo, Failure as CompletionFailure, Presentation as CompletionPresentation,
};

impl ReadPlan {
    /// Isolated projection view; opaque version/client identity is never converted.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "projection-only seam awaits preparation routing")
    )]
    pub(crate) fn project_assets(
        &self,
        root: &crate::store::ArtifactRoot,
    ) -> Result<crate::install::Preparation, crate::install::Error> {
        let mut result = crate::install::Preparation {
            native_directory: None,
            native_files: 0,
            projections_created: 0,
            projections_cached: 0,
        };
        crate::install::prepare::project_only(
            root,
            &self.asset_projections,
            |path| {
                self.files
                    .iter()
                    .find(|f| f.path.0 == path.as_bytes())
                    .map(|f| (f.sha1.as_str(), f.size))
            },
            &mut result,
        )?;
        Ok(result)
    }
}
#[cfg(all(test, target_os = "linux"))]
#[path = "plan_projection_tests.rs"]
mod projection_tests;

/// Private capability selected before opening destination roots. No archive callbacks.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "zero-archive preparation awaits routing")
)]
pub(crate) struct ZeroNativePreparation<'a>(&'a ReadPlan);
impl ReadPlan {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "zero-archive preparation awaits routing")
    )]
    pub(crate) fn zero_native_preparation(&self) -> Option<ZeroNativePreparation<'_>> {
        self.native_extractions
            .is_empty()
            .then_some(ZeroNativePreparation(self))
    }
}
impl ZeroNativePreparation<'_> {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "zero-archive preparation awaits routing")
    )]
    pub(crate) fn prepare(
        &self,
        root: &crate::store::ArtifactRoot,
    ) -> Result<crate::install::Preparation, crate::install::Error> {
        self.prepare_with(root, crate::install::prepare::secure_nonce)
    }
    fn prepare_with(
        &self,
        root: &crate::store::ArtifactRoot,
        nonce: impl FnMut(&mut [u8; 8]) -> Result<(), crate::install::Error>,
    ) -> Result<crate::install::Preparation, crate::install::Error> {
        let mut result = self.0.project_assets(root)?;
        let (relative, native) = crate::install::prepare::create_native_directory(root, nonce)?;
        drop(native);
        result.native_directory = Some(relative);
        Ok(result)
    }
}
#[cfg(all(test, target_os = "linux"))]
#[path = "plan_zero_native_tests.rs"]
mod zero_native_tests;

impl ReadPlan {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "game-native adapter awaits routing")
    )]
    pub(crate) fn prepare_game_native(
        &self,
        root: &crate::store::ArtifactRoot,
    ) -> Result<crate::install::Preparation, crate::install::Error> {
        self.prepare_game_native_with(root, crate::install::prepare::secure_nonce)
    }
    fn prepare_game_native_with(
        &self,
        root: &crate::store::ArtifactRoot,
        nonce: impl FnMut(&mut [u8; 8]) -> Result<(), crate::install::Error>,
    ) -> Result<crate::install::Preparation, crate::install::Error> {
        crate::install::prepare::prepare_with(
            root,
            &self.asset_projections,
            &self.native_extractions,
            |path| {
                self.files
                    .iter()
                    .find(|f| f.path.0 == path.as_bytes())
                    .map(|f| (f.sha1.as_str(), f.size))
            },
            nonce,
        )
    }
}
#[cfg(all(test, target_os = "linux"))]
#[path = "plan_game_native_tests.rs"]
mod game_native_tests;
