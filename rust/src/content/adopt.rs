//! Adopt ordinary unmanaged immutable content through an anchored CAS transaction.
use super::*;
use crate::store::Store;
use sha1::{Digest, Sha1};
use std::io::Read;
#[derive(Serialize)]
pub struct Adopted {
    pub file: ListedPath,
    pub sha1: String,
    pub size: u64,
    pub slug: Option<String>,
    pub as_dependency: bool,
}
impl Root {
    pub fn adopt(&self, kind: Kind, store: &Store) -> Result<Vec<Adopted>> {
        let mut registry = self.load_with(registry::parse_listing)?;
        let (_, unmanaged) = self.list_paths(kind)?;
        let mut adopted = vec![];
        for path in unmanaged {
            let (parent, name) = self.0.parent_bytes(&path.0)?;
            let mut file = match parent.file_bytes(&name, false) {
                Ok(f) => f,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            let mut hash = Sha1::new();
            let mut buf = [0u8; 65536];
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hash.update(&buf[..n]);
            }
            let size = file.metadata()?.len();
            if size == 0 || size > 4 * 1024 * 1024 * 1024 {
                return Err(Error::Registry("InvalidContentSize"));
            }
            let sha1 = format!("{:x}", hash.finalize());
            // Source/destination derive from the already-open directory, never an
            // ambient path reconstructed after validation.
            let anchored = self.0.artifact_root()?;
            let tx = store.transaction(&sha1)?;
            tx.import_bytes(&anchored, &path.0, size)?;
            tx.materialize_bytes(&anchored, &path.0, size)?;
            registry.entries.push(registry::ListingEntry {
                kind,
                file: path.clone(),
                sha1: sha1.clone(),
                size,
                source: registry::Source {
                    provider: registry::Provider::Manual,
                    project_id: None,
                    version_id: None,
                    slug: None,
                },
                enabled: true,
                as_dependency: false,
            });
            adopted.push(Adopted {
                file: path,
                sha1,
                size,
                slug: None,
                as_dependency: false,
            });
        }
        self.save_record(&registry)?;
        Ok(adopted)
    }
}
pub(super) fn handle(
    kind: Kind,
    params: Option<&RawValue>,
) -> std::result::Result<Value, RpcError> {
    let dp = kind == Kind::Datapack;
    let noun = if dp { "datapack" } else { "content" };
    let need = if dp {
        "a directory and world"
    } else {
        "a directory"
    };
    let raw = params.ok_or_else(|| invalid(format!("{noun} adoption requires {need}")))?;
    let parsed = (|| -> Option<(Vec<u8>, Option<ListedPath>, Vec<u8>)> {
        if !raw.get().trim_start().starts_with('{') {
            return None;
        }
        crate::auth::json::Json::parse(raw.get()).ok()?;
        let fields: std::collections::BTreeMap<String, &RawValue> =
            serde_json::from_str(raw.get()).ok()?;
        let directory = dynamic_file(fields.get("directory")?.get()).ok()?;
        let world = if dp {
            Some(ListedPath(dynamic_file(fields.get("world")?.get()).ok()?))
        } else {
            None
        };
        // Missing alone selects the existing default. Explicit null remains invalid.
        let store_directory = match fields.get("store_directory") {
            Some(value) => dynamic_file(value.get()).ok()?,
            None => crate::store::DEFAULT_DIRECTORY.as_bytes().to_vec(),
        };
        Some((directory, world, store_directory))
    })()
    .ok_or_else(|| invalid(format!("{noun} adoption parameters are invalid")))?;
    let (directory, world, store_directory) = parsed;
    if directory.is_empty() || world.as_ref().is_some_and(|w| w.0.is_empty()) {
        return Err(invalid(if dp {
            "datapack adoption requires a directory and world"
        } else {
            "content adoption directory must not be empty"
        }));
    }
    let map = |e| match e {
        Error::WorldNotFound => RpcError::new("WORLD_NOT_FOUND", "No world matches the given name"),
        Error::WorldLocked => {
            RpcError::new("WORLD_LOCKED", "The world is in use by a running game")
        }
        _ => RpcError::new(
            "CONTENT_ADOPT_FAILED",
            if dp {
                "Failed to adopt unmanaged datapacks"
            } else {
                "Failed to adopt unmanaged content"
            },
        ),
    };
    let store_error = |_| RpcError::new("STORE_ROOT_ERROR", "Failed to open the artifact store");
    let (root, store) = if dp {
        // Preserve source CAS-before-world ordering, including creation effects.
        let native_store = crate::store::path_from_bytes(&store_directory).map_err(store_error)?;
        let store = Store::open(&native_store).map_err(store_error)?;
        let native_root = crate::store::path_from_bytes(&directory)
            .map_err(Error::from)
            .map_err(map)?;
        (
            Root::world_bytes_native(&native_root, &world.as_ref().unwrap().0, true)
                .map_err(map)?,
            store,
        )
    } else {
        let root_error = || RpcError::new("INSTALL_ROOT_ERROR", "Failed to open the install root");
        let native_root = crate::store::path_from_bytes(&directory).map_err(|_| root_error())?;
        let root = Root::open_native(&native_root).map_err(|_| root_error())?;
        // Do not move this conversion/open before the configured game root.
        let native_store = crate::store::path_from_bytes(&store_directory).map_err(store_error)?;
        (root, Store::open(&native_store).map_err(store_error)?)
    };
    let adopted = root.adopt(kind, &store).map_err(map)?;
    let stage = format!("{}.adopt", kind.directory());
    let fields = vec![("adopted", val(adopted))];
    match world.as_ref() {
        None => Ok(result(&stage, None, fields)),
        Some(name) => match std::str::from_utf8(&name.0) {
            Ok(text) => Ok(result(&stage, Some(text), fields)),
            Err(_) => Ok(result_with_world(&stage, Some(name), fields)),
        },
    }
}
