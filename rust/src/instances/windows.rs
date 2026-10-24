//! Windows instance adapter using shared handle-relative NT filesystem primitives.
use super::{MAX_MANIFEST_BYTES, Manifest, installed_bytes, validate_id};
use crate::store::windows as fs;
use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
pub struct Root {
    dir: File,
}
fn invalid(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s.to_owned())
}
fn read(dir: &File, name: &str, max: usize) -> io::Result<Vec<u8>> {
    let file = if name == "instance.json" {
        fs::read_follow_at(dir, name.as_ref())?
    } else {
        fs::read_at(dir, name.as_ref())?
    };
    let mut bytes = Vec::new();
    file.take(max as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(invalid("TooLarge"));
    }
    Ok(bytes)
}
fn atomic_write(dir: &File, name: &str, data: &[u8]) -> io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let temp = format!(
        ".jmcl-manifest-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let mut file = fs::create_file(dir, temp.as_ref())?;
    let result = (|| {
        file.write_all(data)?;
        file.sync_all()?;
        fs::named_op(&file, dir, name.as_ref(), true)
    })();
    if result.is_err() {
        let _ = fs::unlink(&file);
    }
    result
}
impl Root {
    pub fn open(path: &str) -> io::Result<Self> {
        if path.is_empty() {
            return Err(invalid("EmptyInstancesRoot"));
        }
        Ok(Self {
            dir: fs::walk(Path::new(path), true)?,
        })
    }
    pub fn create(&self, m: &Manifest) -> io::Result<()> {
        m.validate().map_err(invalid)?;
        let child = fs::create_dir(&self.dir, m.id.as_ref())?;
        let result = (|| {
            let game = fs::create_dir(&child, ".minecraft".as_ref())?;
            drop(game);
            atomic_write(&child, "instance.json", &m.encode().map_err(invalid)?)
        })();
        drop(child);
        if result.is_err() {
            let _ = fs::remove_tree(&self.dir, m.id.as_ref());
        }
        result
    }
    pub fn load(&self, id: &str) -> io::Result<Manifest> {
        validate_id(id).map_err(invalid)?;
        let child = fs::child(&self.dir, id.as_ref(), true, false, false)?;
        Manifest::parse(
            &read(&child, "instance.json", MAX_MANIFEST_BYTES).map_err(|e| {
                if e.to_string() == "TooLarge" {
                    invalid("InstanceManifestTooLarge")
                } else {
                    e
                }
            })?,
            id,
        )
        .map_err(invalid)
    }
    pub fn list(&self) -> io::Result<Vec<Manifest>> {
        let mut values = Vec::new();
        for name in fs::names(&self.dir)? {
            let Some(id) = name.to_str() else { continue };
            if validate_id(id).is_err() {
                continue;
            }
            let Ok(child) = fs::child(&self.dir, &name, true, false, false) else {
                continue;
            };
            drop(child);
            if values.len() == 4096 {
                return Err(invalid("TooManyInstances"));
            }
            if let Ok(m) = self.load(id) {
                values.push(m)
            }
        }
        values.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(values)
    }
    pub fn delete(&self, id: &str) -> io::Result<()> {
        self.load(id)?;
        fs::remove_tree(&self.dir, id.as_ref())
    }
    pub fn installed(&self, m: &Manifest) -> io::Result<bool> {
        let child = fs::child(&self.dir, m.id.as_ref(), true, false, false)?;
        let game = match fs::child(&child, ".minecraft".as_ref(), true, false, false) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        let meta = match fs::child(&game, ".jmcl".as_ref(), true, false, false) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        match read(&meta, "install.json", 16 * 1024) {
            Ok(b) => Ok(installed_bytes(&b, m)),
            Err(e) if e.kind() == io::ErrorKind::NotFound || e.to_string() == "TooLarge" => {
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_uses_existing_manifest_schema() {
        let t = tempfile::tempdir().unwrap();
        let root = Root::open(t.path().to_str().unwrap()).unwrap();
        let m = Manifest::parse(br#"{"id":"a","name":"A","version_id":"1.21"}"#, "a").unwrap();
        root.create(&m).unwrap();
        assert_eq!(root.load("a").unwrap(), m);
        assert!(root.create(&m).is_err());
        assert_eq!(root.list().unwrap(), vec![m.clone()]);
        assert!(!root.installed(&m).unwrap());
        root.delete("a").unwrap();
        assert!(root.list().unwrap().is_empty());
    }
}
