use jmcl_core_rust::{
    install,
    modpack::{planning::*, schema::Error, service},
};
use serde_json::json;
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::Write,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
struct NoProvider;
impl CurseforgeProvider for NoProvider {
    fn project(&mut self, _: u32) -> Result<Project, Error> {
        panic!()
    }
    fn file(&mut self, _: u32, _: u32) -> Result<ProviderFile, Error> {
        panic!()
    }
    fn download_url(&mut self, _: u32, _: u32) -> Result<Option<String>, Error> {
        panic!()
    }
}
struct Fake {
    body: Vec<u8>,
    calls: AtomicUsize,
}
impl install::Transport for Fake {
    fn fetch(&self, url: &str, sink: &mut dyn Write) -> Result<u16, install::Error> {
        assert!(url.starts_with("https://fixture/"));
        self.calls.fetch_add(1, Ordering::SeqCst);
        sink.write_all(&self.body).unwrap();
        Ok(200)
    }
}
fn pack(tmp: &std::path::Path, overrides: bool, bad_override: bool) -> String {
    let hash = format!("{:x}", Sha1::digest(b"hello"));
    let index = json!({"formatVersion":1,"game":"minecraft","name":"Pack","dependencies":{"minecraft":"1.21"},"files":[{"path":"mods/a.jar","hashes":{"sha1":hash},"fileSize":5,"downloads":["https://fixture/a.jar"]}]});
    let path = tmp.join("pack.mrpack");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    zip.start_file(
        "modrinth.index.json",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(&serde_json::to_vec(&index).unwrap()).unwrap();
    if overrides {
        zip.start_file(
            if bad_override {
                "overrides/../bad"
            } else {
                "overrides/mods/a.jar"
            },
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"private").unwrap();
    }
    zip.finish().unwrap();
    path.to_str().unwrap().into()
}
fn run(tmp: &std::path::Path, id: &str, pack: &str, t: &Fake) -> Result<service::Summary, Error> {
    service::install(
        service::Request {
            directory: tmp.join("instances").to_str().unwrap(),
            store_directory: tmp.join("store").to_str().unwrap(),
            id,
            file: pack,
            api_key: None,
        },
        install::Options {
            workers: 1,
            retries: 0,
            sync_files: false,
        },
        &mut NoProvider,
        t,
        None,
    )
}
#[test]
fn hardlink_content_private_overrides_and_cache_reuse() {
    let tmp = tempfile::tempdir().unwrap();
    let path = pack(tmp.path(), false, false);
    let t = Fake {
        body: b"hello".to_vec(),
        calls: AtomicUsize::new(0),
    };
    let result = run(tmp.path(), "first", &path, &t).unwrap();
    assert_eq!(result.files_installed, 1);
    assert_eq!(result.overrides_written, 0);
    let hash = format!("{:x}", Sha1::digest(b"hello"));
    let object = tmp
        .path()
        .join("store/objects/sha1")
        .join(&hash[..2])
        .join(&hash);
    assert_eq!(fs::read(&object).unwrap(), b"hello");
    let installed = tmp.path().join("instances/first/.minecraft/mods/a.jar");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            fs::metadata(&object).unwrap().ino(),
            fs::metadata(&installed).unwrap().ino()
        );
    }
    let path = pack(tmp.path(), true, false);
    let result = run(tmp.path(), "second", &path, &t).unwrap();
    assert_eq!(result.overrides_written, 1);
    assert_eq!(
        fs::read(tmp.path().join("instances/second/.minecraft/mods/a.jar")).unwrap(),
        b"private"
    );
    assert_eq!(fs::read(object).unwrap(), b"hello");
    assert_eq!(fs::read(installed).unwrap(), b"hello");
    assert_eq!(t.calls.load(Ordering::SeqCst), 1);
}
#[test]
fn rollback_preserves_verified_cache_and_retry_has_no_new_format() {
    let tmp = tempfile::tempdir().unwrap();
    let path = pack(tmp.path(), true, true);
    let t = Fake {
        body: b"hello".to_vec(),
        calls: AtomicUsize::new(0),
    };
    assert_eq!(
        run(tmp.path(), "retry", &path, &t).unwrap_err().name(),
        "UnsafePath"
    );
    assert!(!tmp.path().join("instances/retry").exists());
    let path = pack(tmp.path(), false, false);
    run(tmp.path(), "retry", &path, &t).unwrap();
    assert_eq!(t.calls.load(Ordering::SeqCst), 1)
}
#[test]
fn existing_directory_and_file_read_precedence() {
    let tmp = tempfile::tempdir().unwrap();
    let path = pack(tmp.path(), false, false);
    let t = Fake {
        body: vec![],
        calls: AtomicUsize::new(0),
    };
    fs::create_dir_all(tmp.path().join("instances/existing")).unwrap();
    assert_eq!(
        run(tmp.path(), "existing", "/missing/pack", &t)
            .unwrap_err()
            .name(),
        "FileNotFound"
    );
    assert_eq!(
        run(tmp.path(), "existing", &path, &t).unwrap_err().name(),
        "InstanceAlreadyExists"
    );
    fs::write(
        tmp.path().join("instances/existing/instance.json"),
        b"malformed",
    )
    .unwrap();
    assert_eq!(
        run(tmp.path(), "existing", &path, &t).unwrap_err().name(),
        "InstanceAlreadyExists"
    );
}
#[cfg(unix)]
#[test]
fn existing_instance_alias_is_only_read_and_never_deleted() {
    let tmp = tempfile::tempdir().unwrap();
    let path = pack(tmp.path(), false, false);
    let t = Fake {
        body: vec![],
        calls: AtomicUsize::new(0),
    };
    fs::create_dir_all(tmp.path().join("instances")).unwrap();
    fs::create_dir(tmp.path().join("outside")).unwrap();
    std::os::unix::fs::symlink(
        tmp.path().join("outside"),
        tmp.path().join("instances/link"),
    )
    .unwrap();
    assert_eq!(
        run(tmp.path(), "link", &path, &t).unwrap_err().name(),
        "InstanceAlreadyExists"
    );
    assert!(tmp.path().join("outside").is_dir())
}
#[test]
fn events_have_no_provider_key_and_sink_failure_rolls_back() {
    let tmp = tempfile::tempdir().unwrap();
    let path = pack(tmp.path(), false, false);
    let t = Fake {
        body: b"hello".to_vec(),
        calls: AtomicUsize::new(0),
    };
    let calls = Mutex::new(0);
    let sink = |_: install::Event| {
        *calls.lock().unwrap() += 1;
        Err(install::Error::named("WriteFailed"))
    };
    let result = service::install(
        service::Request {
            directory: tmp.path().join("instances").to_str().unwrap(),
            store_directory: tmp.path().join("store").to_str().unwrap(),
            id: "failed",
            file: &path,
            api_key: None,
        },
        install::Options {
            workers: 1,
            retries: 0,
            sync_files: false,
        },
        &mut NoProvider,
        &t,
        Some(&sink),
    );
    assert_eq!(result.unwrap_err().name(), "WriteFailed");
    assert!(!tmp.path().join("instances/failed").exists())
}

struct Cf {
    projects: usize,
    files: usize,
    url_calls: usize,
    unavailable: bool,
}
impl CurseforgeProvider for Cf {
    fn project(&mut self, id: u32) -> Result<Project, Error> {
        assert!([1, 2].contains(&id));
        self.projects += 1;
        Ok(Project {
            class_id: 6,
            slug: "fixture".into(),
        })
    }
    fn file(&mut self, project: u32, id: u32) -> Result<ProviderFile, Error> {
        assert!([1, 2].contains(&project));
        self.files += 1;
        Ok(ProviderFile {
            id,
            project,
            name: format!("{id}.jar"),
            sha1: format!("{:x}", Sha1::digest(b"hello")),
            size: 5,
            url: None,
        })
    }
    fn download_url(&mut self, project: u32, id: u32) -> Result<Option<String>, Error> {
        assert!([1, 2].contains(&project));
        self.url_calls += 1;
        Ok((!self.unavailable).then(|| format!("https://fixture/{id}.jar")))
    }
}
#[test]
fn curseforge_required_optional_cached_project_and_caller_key() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("cf.zip");
    let mut z = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    z.start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(&serde_json::to_vec(&json!({"manifestType":"minecraftModpack","manifestVersion":1,"name":"CF","minecraft":{"version":"1.21"},"files":[{"projectID":1,"fileID":2},{"projectID":2,"fileID":3},{"projectID":9,"fileID":9,"required":false}]})).unwrap()).unwrap();
    z.finish().unwrap();
    let mut provider = Cf {
        projects: 0,
        files: 0,
        url_calls: 0,
        unavailable: false,
    };
    let t = Fake {
        body: b"hello".to_vec(),
        calls: AtomicUsize::new(0),
    };
    let result = service::install(
        service::Request {
            directory: tmp.path().join("instances").to_str().unwrap(),
            store_directory: tmp.path().join("store").to_str().unwrap(),
            id: "cf",
            file: path.to_str().unwrap(),
            api_key: Some("fixture-not-a-real-key"),
        },
        install::Options {
            workers: 1,
            retries: 0,
            sync_files: false,
        },
        &mut provider,
        &t,
        None,
    )
    .unwrap();
    assert_eq!(result.files_installed, 2);
    assert_eq!(result.files_skipped, 1);
    assert_eq!(
        (provider.projects, provider.files, provider.url_calls),
        (2, 2, 2)
    );
    assert_eq!(t.calls.load(Ordering::SeqCst), 1);
    let registry = fs::read_to_string(
        tmp.path()
            .join("instances/cf/.minecraft/.jmcl/content.json"),
    )
    .unwrap();
    assert!(!registry.contains("fixture-not-a-real-key"));
    assert!(registry.contains("curseforge"));
    provider.unavailable = true;
    let err = service::install(
        service::Request {
            directory: tmp.path().join("instances").to_str().unwrap(),
            store_directory: tmp.path().join("store").to_str().unwrap(),
            id: "unavailable",
            file: path.to_str().unwrap(),
            api_key: Some("fixture-not-a-real-key"),
        },
        install::Options::default(),
        &mut provider,
        &t,
        None,
    )
    .unwrap_err();
    assert_eq!(err.name(), "CurseforgeDownloadUnavailable");
    assert!(!tmp.path().join("instances/unavailable").exists());
}
