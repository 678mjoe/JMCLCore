use jmcl_core_rust::{
    content::{curseforge, modrinth, remote, rpc},
    install,
    store::Store,
};
use serde_json::{json, value::RawValue};
use sha1::{Digest, Sha1};
use std::fs;
use std::{
    collections::HashMap,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
struct Fixture(HashMap<String, Vec<u8>>);
impl remote::Metadata for Fixture {
    fn get(&self, u: &str, _: usize) -> modrinth::Result<Vec<u8>> {
        self.0
            .get(u)
            .cloned()
            .ok_or_else(|| modrinth::Error("MissingFixture".into()))
    }
}
impl curseforge::Metadata for Fixture {
    fn get(&self, u: &str, key: &str, _: usize) -> modrinth::Result<Vec<u8>> {
        assert_eq!(key, "synthetic-key");
        assert!(u.starts_with("https://api.curseforge.com/"));
        remote::Metadata::get(self, u, 0)
    }
}
impl install::Transport for Fixture {
    fn fetch(&self, _: &str, _: &mut dyn Write) -> Result<u16, install::Error> {
        panic!("fixture files must be already verified in CAS")
    }
}
fn docs(kind: &str, provider: &str) -> Fixture {
    let hash = format!("{:x}", Sha1::digest(b"content"));
    let ext = if kind == "mods" { "jar" } else { "zip" };
    let mut map = HashMap::new();
    if provider == "modrinth" {
        let ptype = match kind {
            "mods" => "mod",
            "resourcepacks" => "resourcepack",
            "shaderpacks" => "shader",
            _ => "datapack",
        };
        let loader = match kind {
            "mods" => "fabric",
            "resourcepacks" => "minecraft",
            _ => "datapack",
        };
        let project = json!({"id":"Proj1","slug":"project","project_type":ptype});
        let version = json!({"id":"Ver1","project_id":"Proj1","version_number":"1","version_type":"release","game_versions":["1.20.1"],"loaders":[loader],"files":[{"url":"https://cdn.invalid/content","filename":format!("one.{ext}"),"size":7,"hashes":{"sha1":hash}}]});
        map.insert(
            modrinth::project_url("project").unwrap(),
            serde_json::to_vec(&project).unwrap(),
        );
        map.insert(
            modrinth::versions_url(
                "Proj1",
                "1.20.1",
                if kind == "shaderpacks" {
                    None
                } else {
                    Some(loader)
                },
            )
            .unwrap(),
            serde_json::to_vec(&json!([version])).unwrap(),
        );
    } else {
        let class = match kind {
            "mods" => 6,
            "resourcepacks" => 12,
            "shaderpacks" => 6552,
            _ => 5290,
        };
        map.insert(
            "https://api.curseforge.com/v1/mods/42".into(),
            serde_json::to_vec(
                &json!({"data":{"id":42,"slug":"project","name":"Project","classId":class}}),
            )
            .unwrap(),
        );
        let f = json!({"id":1,"modId":42,"fileName":format!("one.{ext}"),"fileLength":7,"hashes":[{"algo":1,"value":hash}],"releaseType":1,"fileDate":"2024","gameVersions":["1.20.1","Fabric"],"downloadUrl":"https://cdn.invalid/content"});
        map.insert(
            curseforge::page_url(
                "42",
                "1.20.1",
                if kind == "mods" { Some("fabric") } else { None },
                0,
            )
            .unwrap(),
            serde_json::to_vec(&json!({"data":[f]})).unwrap(),
        );
    }
    Fixture(map)
}
fn invoke(
    zig: &std::ffi::OsStr,
    fixture_path: &Path,
    f: &Fixture,
    req: &serde_json::Value,
    is_zig: bool,
) -> Vec<u8> {
    if is_zig {
        let mut c = Command::new(zig)
            .arg("rpc")
            .env("JMCL_CONTENT_FIXTURES", fixture_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(c.stdin.take().unwrap(), "{req}").unwrap();
        let o = c.wait_with_output().unwrap();
        assert!(o.status.success());
        o.stdout
    } else {
        let raw = RawValue::from_string(req["params"].to_string()).unwrap();
        let mut out = vec![];
        rpc::handle_with(
            req["method"].as_str().unwrap(),
            Some(&raw),
            req["id"].as_str().unwrap(),
            &mut out,
            f,
            f,
        )
        .unwrap()
        .unwrap();
        out
    }
}
fn identity(path: &Path) -> (u64, u64) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = fs::metadata(path).unwrap();
        (m.dev(), m.ino())
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let f = fs::File::open(path).unwrap();
        let mut i = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        assert_ne!(
            unsafe { GetFileInformationByHandle(f.as_raw_handle(), i.as_mut_ptr()) },
            0
        );
        let i = unsafe { i.assume_init() };
        (
            u64::from(i.dwVolumeSerialNumber),
            (u64::from(i.nFileIndexHigh) << 32) | u64::from(i.nFileIndexLow),
        )
    }
}

fn filename(f: &mut Fixture, provider: &str, bytes: &[u8]) {
    for (url, body) in &mut f.0 {
        let mut v: serde_json::Value = serde_json::from_slice(body).unwrap();
        if provider == "modrinth" && url.contains("/version?") {
            v[0]["files"][0]["filename"] = json!(bytes);
        } else if provider == "curseforge" && url.contains("/files?") {
            v["data"][0]["fileName"] = json!(bytes);
        }
        *body = serde_json::to_vec(&v).unwrap();
    }
}
fn native(bytes: &[u8]) -> std::ffi::OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(bytes.to_vec())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        if let Ok(s) = std::str::from_utf8(bytes) {
            return s.into();
        }
        assert!(bytes.starts_with(b"\xed\xa0\x80"));
        std::ffi::OsString::from_wide(
            &std::iter::once(0xd800)
                .chain(bytes[3..].iter().map(|b| u16::from(*b)))
                .collect::<Vec<_>>(),
        )
    }
}
fn wire(bytes: &[u8]) -> serde_json::Value {
    match std::str::from_utf8(bytes) {
        Ok(s) => json!(s),
        Err(_) => json!(bytes),
    }
}
fn serial_fixture(f: &Fixture, p: &Path) {
    let map: HashMap<_, _> =
        f.0.iter()
            .map(|(k, v)| (k, std::str::from_utf8(v).unwrap()))
            .collect();
    std::fs::write(p, serde_json::to_vec(&map).unwrap()).unwrap();
}
fn records(out: &[u8]) -> Vec<serde_json::Value> {
    out.split(|b| *b == b'\n')
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::from_slice(s).unwrap())
        .collect()
}
fn assert_cached(out: &[u8], file: &[u8]) {
    let rows = records(out);
    assert_eq!(
        rows.last().unwrap()["event"],
        "result",
        "{}",
        String::from_utf8_lossy(out)
    );
    let files: Vec<_> = rows.iter().filter(|v| v["event"] == "file").collect();
    assert_eq!(files.len(), 1, "{rows:?}");
    assert_eq!(files[0]["file"]["path"], wire(file));
    assert_eq!(files[0]["file"]["status"], "cached");
    assert_eq!(files[0]["file"]["placement"], "hard_link");
    assert_eq!(files[0]["file"]["size"], 7);
    assert!(!rows.iter().any(|v| v["event"] == "retry"));
}
#[test]
#[ignore = "requires JMCL_CONTENT_FILENAME_ORACLE built with --deny-artifact-fetch"]
fn fake_install_preserves_raw_events_registry_and_hardlinks() {
    let zig = guarded_oracle();
    let mut pairs = 0;
    for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
        for provider in ["modrinth", "curseforge"] {
            for prefix in [
                b"normal".as_slice(),
                "世界".as_bytes(),
                b"\xed\xa0\x80",
                b"\xff",
            ] {
                if cfg!(windows) && prefix == b"\xff" {
                    continue;
                }
                let mut name = prefix.to_vec();
                name.extend_from_slice(if family == "mods" { b".jar" } else { b".zip" });
                let t = tempfile::tempdir().unwrap();
                let game = t.path().join("game");
                let cache = t.path().join("cache");
                let mut f = docs(family, provider);
                filename(&mut f, provider, &name);
                let fixture = t.path().join("fixtures.json");
                serial_fixture(&f, &fixture);
                let hash = format!("{:x}", Sha1::digest(b"content"));
                let source = t.path().join("source");
                std::fs::write(&source, b"content").unwrap();
                Store::open(&cache)
                    .unwrap()
                    .import_verified(&source, &hash, 7)
                    .unwrap();
                let object = cache.join(jmcl_core_rust::store::object_path(&hash).unwrap());
                let peer = t.path().join("peer");
                std::fs::hard_link(&object, &peer).unwrap();
                let old = identity(&peer);
                let mut p = json!({"directory":game,"minecraft_version":"1.20.1","project":if provider=="modrinth"{"project"}else{"42"},"provider":provider,"store_directory":cache,"workers":1,"retries":0});
                if family == "mods" {
                    p["loader"] = json!("fabric")
                }
                if family == "datapacks" {
                    p["world"] = json!("World")
                }
                if provider == "curseforge" {
                    p["api_key"] = json!("synthetic-key")
                }
                let req = json!({"protocol":1,"id":"bytes","method":format!("{family}.install"),"params":p});
                let target = if family == "datapacks" {
                    game.join("saves/World")
                } else {
                    game.clone()
                };
                let mut pair = Vec::new();
                for z in [true, false] {
                    std::fs::create_dir_all(&target).unwrap();
                    let out = invoke(&zig, &fixture, &f, &req, z);
                    let registry = target.join(".jmcl/content.json");
                    let dest = target.join(family).join(native(&name));
                    let rows = records(&out);
                    if rows.last().unwrap()["event"] == "result" {
                        let mut relative = family.as_bytes().to_vec();
                        relative.push(b'/');
                        relative.extend_from_slice(&name);
                        assert_cached(&out, &relative);
                        assert_eq!(std::fs::read(&dest).unwrap(), b"content");
                        assert_eq!(identity(&dest), old);
                        assert!(std::fs::metadata(&dest).unwrap().permissions().readonly());
                    } else {
                        eprintln!(
                            "filename rejection {family}/{provider}/{name:?}/zig={z}: {}",
                            String::from_utf8_lossy(&out)
                        );
                        #[cfg(unix)]
                        {
                            let probe = target.join(native(&name));
                            let err = std::fs::File::create(&probe).unwrap_err();
                            assert_eq!(err.raw_os_error(), Some(libc::EILSEQ));
                            assert!(!registry.exists());
                            eprintln!(
                                "raw filename materialization unavailable: EILSEQ; comparing exact rejection and registry absence"
                            );
                        }
                        #[cfg(windows)]
                        panic!(
                            "unexpected fixture failure: {}",
                            String::from_utf8_lossy(&out)
                        );
                    }
                    pair.push((out, std::fs::read(&registry).ok()));
                    assert_eq!(identity(&peer), old);
                    assert_eq!(std::fs::read(&peer).unwrap(), b"content");
                    assert!(std::fs::metadata(&peer).unwrap().permissions().readonly());
                    std::fs::remove_dir_all(&game).unwrap();
                }
                assert_eq!(pair[0], pair[1], "{family}/{provider}/{name:?}");
                pairs += 1;
            }
        }
    }
    eprintln!(
        "{pairs} cached raw filename RPC pairs; exact events/registry and immutable hardlinks checked; no artifact fetch"
    );
}

fn next_version(f: &mut Fixture, provider: &str, name: &[u8]) {
    if provider == "modrinth" {
        let body = f.0.iter().find(|(k, _)| k.contains("/version?")).unwrap().1;
        let mut v: serde_json::Value = serde_json::from_slice(body).unwrap();
        let mut v = v[0].take();
        v["id"] = json!("Ver2");
        v["files"][0]["filename"] = json!(name);
        f.0.insert(
            modrinth::version_url("Ver2").unwrap(),
            serde_json::to_vec(&v).unwrap(),
        );
    } else {
        let body = f.0.iter().find(|(k, _)| k.contains("/files?")).unwrap().1;
        let mut v: serde_json::Value = serde_json::from_slice(body).unwrap();
        let mut v = v["data"][0].take();
        v["id"] = json!(2);
        v["fileName"] = json!(name);
        f.0.insert(
            "https://api.curseforge.com/v1/mods/42/files/2".into(),
            serde_json::to_vec(&json!({"data":v})).unwrap(),
        );
    }
}
#[test]
#[ignore = "requires JMCL_CONTENT_FILENAME_ORACLE built with --deny-artifact-fetch"]
fn disabled_switch_preserves_byte_paths_and_immutable_peers() {
    let zig = guarded_oracle();
    let mut strict_failures = Vec::new();
    for provider in ["modrinth", "curseforge"] {
        for prefix in [b"unicode".as_slice(), b"\xed\xa0\x80"] {
            let mut old = prefix.to_vec();
            old.extend_from_slice(b"-old.jar");
            let mut new = prefix.to_vec();
            new.extend_from_slice(b"-new.jar");
            let t = tempfile::tempdir().unwrap();
            let game = t.path().join("game");
            let cache = t.path().join("cache");
            let hash = format!("{:x}", Sha1::digest(b"content"));
            let source = t.path().join("source");
            std::fs::write(&source, b"content").unwrap();
            Store::open(&cache)
                .unwrap()
                .import_verified(&source, &hash, 7)
                .unwrap();
            let object = cache.join(jmcl_core_rust::store::object_path(&hash).unwrap());
            let peer = t.path().join("peer");
            std::fs::hard_link(&object, &peer).unwrap();
            let id = identity(&peer);
            let mut f = docs("mods", provider);
            filename(&mut f, provider, &old);
            next_version(&mut f, provider, &new);
            let fixture = t.path().join("fixtures.json");
            serial_fixture(&f, &fixture);
            let project = if provider == "modrinth" {
                "project"
            } else {
                "42"
            };
            let mut p = json!({"directory":game,"minecraft_version":"1.20.1","loader":"fabric","provider":provider,"project":project,"api_key":"synthetic-key","store_directory":cache,"workers":1,"retries":0});
            if provider == "modrinth" {
                p.as_object_mut().unwrap().remove("api_key");
            }
            let setup = json!({"protocol":1,"id":"setup","method":"mods.install","params":p});
            let mut change = setup.clone();
            change["params"].as_object_mut().unwrap().remove("provider");
            change["method"] = json!("mods.set-version");
            change["params"]["version_id"] =
                json!(if provider == "modrinth" { "Ver2" } else { "2" });
            let mut pair = Vec::new();
            let mut observations = Vec::new();
            for z in [true, false] {
                std::fs::create_dir_all(&game).unwrap();
                let first = invoke(&zig, &fixture, &f, &setup, z);
                if records(&first).last().unwrap()["event"] != "result" {
                    eprintln!("setup {provider} {z}: {}", String::from_utf8_lossy(&first));
                    #[cfg(unix)]
                    {
                        assert_eq!(
                            std::fs::File::create(game.join(native(&old)))
                                .unwrap_err()
                                .raw_os_error(),
                            Some(libc::EILSEQ)
                        );
                    }
                    #[cfg(windows)]
                    panic!("unexpected: {}", String::from_utf8_lossy(&first));
                    std::fs::remove_dir_all(&game).unwrap();
                    continue;
                }
                let reg = game.join(".jmcl/content.json");
                let unrelated = json!({"kind":"mod","file":b"mods/unrelated-\xff.jar".to_vec(),"sha1":hash,"size":7,"source":{"provider":"manual","project_id":null,"version_id":null,"slug":null},"enabled":true,"as_dependency":false});
                let mut registry: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&reg).unwrap()).unwrap();
                registry["entries"]
                    .as_array_mut()
                    .unwrap()
                    .push(unrelated.clone());
                std::fs::write(&reg, serde_json::to_vec(&registry).unwrap()).unwrap();
                let toggle =
                    RawValue::from_string(json!({"directory":game,"project":project}).to_string())
                        .unwrap();
                jmcl_core_rust::content::handle("mods.disable", Some(&toggle))
                    .unwrap()
                    .unwrap();
                let mut old_disabled = old.clone();
                old_disabled.extend_from_slice(b".disabled");
                let out = invoke(&zig, &fixture, &f, &change, z);
                let mut relative = b"mods/".to_vec();
                relative.extend_from_slice(&new);
                let mut new_disabled = new.clone();
                new_disabled.extend_from_slice(b".disabled");
                let files=[&old,&old_disabled,&new,&new_disabled].map(|name| {
                        let path=game.join("mods").join(native(name));
                        let state=match std::fs::symlink_metadata(&path) {
                            Ok(meta)=>json!({"bytes":std::fs::read(&path).unwrap(),"readonly":meta.permissions().readonly(),"identity":identity(&path)}),
                            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>serde_json::Value::Null,
                            Err(e)=>panic!("failed diagnostic inspection: {e}"),
                        };
                        json!({"name":name,"state":state})
                    });
                let observed = json!({"subject":if z {"zig"} else {"rust"},"case":format!("{provider}/{prefix:?}"),"records":records(&out),"registry_bytes":std::fs::read(&reg).unwrap(),"files":files,"peer_identity":identity(&peer),"peer_bytes":std::fs::read(&peer).unwrap(),"peer_readonly":std::fs::metadata(&peer).unwrap().permissions().readonly()});
                if cfg!(windows) {
                    eprintln!("ZIG007_FILENAME_DIAGNOSTIC {observed}");
                }
                observations.push(observed.clone());
                if records(&out).last().unwrap()["event"] != "result" {
                    assert!(
                        z,
                        "Rust correction failed: {}",
                        String::from_utf8_lossy(&out)
                    );
                    strict_failures.push(observed);
                    assert_eq!(identity(&peer), id);
                    assert_eq!(std::fs::read(&peer).unwrap(), b"content");
                    assert!(std::fs::metadata(&peer).unwrap().permissions().readonly());
                    std::fs::remove_dir_all(&game).unwrap();
                    continue;
                }
                if z {
                    let disk = std::fs::read(&reg).unwrap();
                    let repeated = invoke(&zig, &fixture, &f, &change, true);
                    pair.push((out, disk, repeated));
                    std::fs::remove_dir_all(&game).unwrap();
                    continue;
                }
                assert_cached(&out, &relative);
                let mut disabled = new.clone();
                disabled.extend_from_slice(b".disabled");
                let dest = game.join("mods").join(native(&disabled));
                assert_eq!(identity(&dest), id);
                assert_eq!(std::fs::read(&dest).unwrap(), b"content");
                assert!(std::fs::metadata(&dest).unwrap().permissions().readonly());
                assert!(
                    matches!(std::fs::symlink_metadata(game.join("mods").join(native(&old_disabled))),Err(e)if e.kind()==std::io::ErrorKind::NotFound)
                );
                assert!(
                    matches!(std::fs::symlink_metadata(game.join("mods").join(native(&new))),Err(e)if e.kind()==std::io::ErrorKind::NotFound)
                );
                let disk = std::fs::read(&reg).unwrap();
                let parsed: serde_json::Value = serde_json::from_slice(&disk).unwrap();
                assert_eq!(parsed["entries"].as_array().unwrap().len(), 2);
                assert_eq!(parsed["entries"][1], unrelated);
                assert_eq!(parsed["entries"][0]["enabled"], false);
                assert_eq!(parsed["entries"][0]["file"], wire(&relative));
                let repeated = invoke(&zig, &fixture, &f, &change, z);
                assert_eq!(
                    records(&repeated).last().unwrap()["result"]["changed"],
                    false
                );
                assert_eq!(std::fs::read(&reg).unwrap(), disk);
                pair.push((out, disk, repeated));
                assert_eq!(identity(&peer), id);
                assert_eq!(std::fs::read(&peer).unwrap(), b"content");
                assert!(std::fs::metadata(&peer).unwrap().permissions().readonly());
                std::fs::remove_dir_all(&game).unwrap();
            }
            if pair.len() == 2 {
                assert_eq!(pair[0], pair[1], "{observations:?}");
                assert_eq!(
                    observations[0]["files"], observations[1]["files"],
                    "{observations:?}"
                );
            }
        }
    }
    assert!(
        strict_failures.is_empty(),
        "strict disabled filename comparisons failed; no new ZIG007 allowance: {strict_failures:?}"
    );
}

struct WrongBody(std::sync::atomic::AtomicUsize);
impl install::Transport for WrongBody {
    fn fetch(&self, url: &str, out: &mut dyn Write) -> Result<u16, install::Error> {
        assert_eq!(url, "https://cdn.invalid/content");
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        out.write_all(b"WRONG!!")?;
        Ok(200)
    }
}
struct RejectFile;
impl Write for RejectFile {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "owned fixture writer",
        ))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn hash_failure_and_reporter_failure_match_owned_state() {
    for cached in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let game = t.path().join("game");
        let cache = t.path().join("cache");
        std::fs::create_dir(&game).unwrap();
        let mut f = docs("mods", "modrinth");
        let name = "世界.jar".as_bytes();
        filename(&mut f, "modrinth", name);
        let hash = format!("{:x}", Sha1::digest(b"content"));
        let source = t.path().join("source");
        std::fs::write(&source, b"content").unwrap();
        let store = Store::open(&cache).unwrap();
        let peer = t.path().join("peer");
        if cached {
            store.import_verified(&source, &hash, 7).unwrap();
            std::fs::hard_link(
                cache.join(jmcl_core_rust::store::object_path(&hash).unwrap()),
                &peer,
            )
            .unwrap();
        } else {
            std::fs::write(&peer, b"unrelated").unwrap();
        }
        let peer_id = identity(&peer);
        let peer_bytes = std::fs::read(&peer).unwrap();
        let permissions = std::fs::metadata(&peer).unwrap().permissions();
        let p=RawValue::from_string(json!({"directory":game,"minecraft_version":"1.20.1","project":"project","loader":"fabric","store_directory":cache,"workers":1,"retries":1}).to_string()).unwrap();
        let wrong = WrongBody(std::sync::atomic::AtomicUsize::new(0));
        if cached {
            let err =
                rpc::handle_with("mods.install", Some(&p), "failure", &mut RejectFile, &f, &f)
                    .unwrap()
                    .unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe);
            let dest = game.join("mods").join(native(name));
            assert_eq!(identity(&dest), peer_id);
            assert_eq!(std::fs::read(dest).unwrap(), b"content");
        } else {
            let mut out = vec![];
            rpc::handle_with("mods.install", Some(&p), "failure", &mut out, &f, &wrong)
                .unwrap()
                .unwrap();
            let rows = records(&out);
            assert_eq!(
                rows.last().unwrap()["error"]["code"],
                "CONTENT_INSTALL_FAILED"
            );
            assert_eq!(wrong.0.load(std::sync::atomic::Ordering::SeqCst), 2);
            let retry = rows.iter().find(|v| v["event"] == "retry").unwrap();
            assert_eq!(retry["retry"]["path"], "mods/世界.jar");
            assert_eq!(retry["retry"]["error"], "DownloadHashMismatch");
            assert!(
                matches!(std::fs::symlink_metadata(game.join("mods").join(native(name))),Err(e)if e.kind()==std::io::ErrorKind::NotFound)
            );
        }
        assert!(
            matches!(std::fs::symlink_metadata(game.join(".jmcl/content.json")),Err(e)if e.kind()==std::io::ErrorKind::NotFound)
        );
        assert_eq!(identity(&peer), peer_id);
        assert_eq!(std::fs::read(&peer).unwrap(), peer_bytes);
        assert_eq!(std::fs::metadata(&peer).unwrap().permissions(), permissions);
    }
}

#[test]
#[ignore = "requires JMCL_CONTENT_FILENAME_ORACLE built with --deny-artifact-fetch"]
fn cached_cli_and_server_filename_bytes_match() {
    let zig = guarded_oracle();
    let mut pairs = 0;
    for server in [false, true] {
        if server && !cfg!(target_os = "linux") {
            continue;
        }
        for family in ["mods", "resourcepacks", "shaderpacks", "datapacks"] {
            if server && !matches!(family, "mods" | "datapacks") {
                continue;
            }
            for provider in ["modrinth", "curseforge"] {
                for prefix in ["世界".as_bytes(), b"\xed\xa0\x80"] {
                    let mut name = prefix.to_vec();
                    name.extend_from_slice(if family == "mods" { b".jar" } else { b".zip" });
                    let t = tempfile::tempdir().unwrap();
                    let root = t.path().join("homes");
                    let home = root.join("demo");
                    let game = if server {
                        home.clone()
                    } else {
                        home.join(".minecraft")
                    };
                    let cache = t.path().join("cache");
                    let mut f = docs(family, provider);
                    filename(&mut f, provider, &name);
                    let fixture = t.path().join("fixtures.json");
                    serial_fixture(&f, &fixture);
                    let hash = format!("{:x}", Sha1::digest(b"content"));
                    let source = t.path().join("source");
                    std::fs::write(&source, b"content").unwrap();
                    Store::open(&cache)
                        .unwrap()
                        .import_verified(&source, &hash, 7)
                        .unwrap();
                    let object = cache.join(jmcl_core_rust::store::object_path(&hash).unwrap());
                    let old = identity(&object);
                    let project = if provider == "modrinth" {
                        "project"
                    } else {
                        "42"
                    };
                    let mut args = vec![
                        "demo".into(),
                        "--project".into(),
                        project.into(),
                        "--provider".into(),
                        provider.into(),
                        if server {
                            "--servers-directory".into()
                        } else {
                            "--instances-directory".into()
                        },
                        root.to_str().unwrap().into(),
                        "--store-directory".into(),
                        cache.to_str().unwrap().into(),
                        "--workers".into(),
                        "1".into(),
                        "--retries".into(),
                        "0".into(),
                    ];
                    if family == "datapacks" && !server {
                        args.extend(["--world".into(), "World".into()]);
                    }
                    if provider == "curseforge" {
                        args.extend(["--api-key".into(), "synthetic-key".into()]);
                    }
                    let mut pair = vec![];
                    for z in [true, false] {
                        std::fs::create_dir_all(&game).unwrap();
                        if server {
                            std::fs::write(home.join("server.json"),br#"{"id":"demo","name":"Demo","version_id":"1.20.1","fabric_loader":"0.15.0"}"#).unwrap();
                            std::fs::write(home.join("server.properties"), b"level-name=World\n")
                                .unwrap();
                        } else {
                            std::fs::write(home.join("instance.json"),br#"{"schema_version":1,"id":"demo","name":"Demo","version_id":"1.20.1","fabric_loader":"0.15.0","source":"official"}"#).unwrap();
                        }
                        let target = if family == "datapacks" {
                            if server {
                                game.join("World")
                            } else {
                                game.join("saves/World")
                            }
                        } else {
                            game.clone()
                        };
                        std::fs::create_dir_all(&target).unwrap();
                        let result = if z {
                            let o = Command::new(&zig)
                                .args([
                                    if server { "server" } else { "instance" },
                                    family,
                                    "install",
                                ])
                                .args(&args)
                                .env("JMCL_CONTENT_FIXTURES", &fixture)
                                .output()
                                .unwrap();
                            (o.status.code().unwrap() as u8, o.stdout, o.stderr)
                        } else {
                            let (mut out, mut err) = (vec![], vec![]);
                            let code = if server {
                                let mut all = vec![family.into(), "install".into()];
                                all.extend(args.clone());
                                jmcl_core_rust::server_content::cli_with(
                                    &all, &mut out, &mut err, &f, &f,
                                )
                                .unwrap()
                                .unwrap()
                            } else {
                                jmcl_core_rust::content::cli_remote::run_with(
                                    jmcl_core_rust::content::registry::Kind::from_directory(family)
                                        .unwrap(),
                                    "install",
                                    &args,
                                    &mut out,
                                    &mut err,
                                    &f,
                                    &f,
                                )
                                .unwrap()
                            };
                            (code, out, err)
                        };
                        if result.0 == 0 {
                            assert_eq!(identity(&target.join(family).join(native(&name))), old);
                            assert!(result.1.windows(name.len()).any(|w| w == name));
                        } else {
                            #[cfg(unix)]
                            assert_eq!(
                                std::fs::File::create(target.join(native(&name)))
                                    .unwrap_err()
                                    .raw_os_error(),
                                Some(libc::EILSEQ)
                            );
                            #[cfg(windows)]
                            panic!("unexpected CLI failure: {:?}", result);
                        }
                        pair.push((
                            result,
                            std::fs::read(target.join(".jmcl/content.json")).ok(),
                        ));
                        assert_eq!(identity(&object), old);
                        assert_eq!(std::fs::read(&object).unwrap(), b"content");
                        assert!(std::fs::metadata(&object).unwrap().permissions().readonly());
                        std::fs::remove_dir_all(&root).unwrap();
                    }
                    assert_eq!(pair[0], pair[1], "{server}/{family}/{provider}/{name:?}");
                    pairs += 1;
                }
            }
        }
    }
    eprintln!(
        "{pairs} exact cached client/server CLI filename pairs, immutable CAS identity checked"
    );
}

fn guarded_oracle() -> std::ffi::OsString {
    let path = std::env::var_os("JMCL_CONTENT_FILENAME_ORACLE")
        .expect("dedicated guarded oracle required");
    let dir = Path::new(&path).parent().unwrap();
    let provenance: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("provenance.json")).unwrap()).unwrap();
    assert_eq!(
        provenance["source_head"],
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    assert_eq!(provenance["deny_artifact_fetch"], true);
    let patch = std::fs::read_to_string(dir.join("artifact-fetch-guard.patch")).unwrap();
    assert!(patch.contains("+    return error.FixtureArtifactFetchForbidden;"));
    path
}

#[allow(clippy::too_many_arguments)]
fn cli_call(
    zig: &std::ffi::OsStr,
    fixture: &Path,
    f: &Fixture,
    z: bool,
    server: bool,
    op: &str,
    args: &[String],
) -> (u8, Vec<u8>, Vec<u8>) {
    if z {
        let o = Command::new(zig)
            .args([if server { "server" } else { "instance" }, "mods", op])
            .args(args)
            .env("JMCL_CONTENT_FIXTURES", fixture)
            .output()
            .unwrap();
        (o.status.code().unwrap() as u8, o.stdout, o.stderr)
    } else {
        let (mut out, mut err) = (vec![], vec![]);
        let code = if server {
            let mut all = vec!["mods".into(), op.into()];
            all.extend_from_slice(args);
            jmcl_core_rust::server_content::cli_with(&all, &mut out, &mut err, f, f)
                .unwrap()
                .unwrap()
        } else {
            jmcl_core_rust::content::cli_remote::run_with(
                jmcl_core_rust::content::registry::Kind::Mod,
                op,
                args,
                &mut out,
                &mut err,
                f,
                f,
            )
            .unwrap()
        };
        (code, out, err)
    }
}
#[test]
#[ignore = "requires JMCL_CONTENT_FILENAME_ORACLE built with --deny-artifact-fetch"]
fn raw_switch_cli_and_server_rpc_preserve_results() {
    let zig = guarded_oracle();
    for server in [false, true] {
        if server && !cfg!(target_os = "linux") {
            continue;
        }
        for provider in ["modrinth", "curseforge"] {
            for prefix in ["世界".as_bytes(), b"\xed\xa0\x80"] {
                let mut old = prefix.to_vec();
                old.extend_from_slice(b"-old.jar");
                let mut new = prefix.to_vec();
                new.extend_from_slice(b"-new.jar");
                let t = tempfile::tempdir().unwrap();
                let root = t.path().join("homes");
                let home = root.join("demo");
                let game = if server {
                    home.clone()
                } else {
                    home.join(".minecraft")
                };
                let cache = t.path().join("cache");
                let mut f = docs("mods", provider);
                filename(&mut f, provider, &old);
                next_version(&mut f, provider, &new);
                let fixture = t.path().join("fixtures.json");
                serial_fixture(&f, &fixture);
                let hash = format!("{:x}", Sha1::digest(b"content"));
                let source = t.path().join("source");
                std::fs::write(&source, b"content").unwrap();
                Store::open(&cache)
                    .unwrap()
                    .import_verified(&source, &hash, 7)
                    .unwrap();
                let object = cache.join(jmcl_core_rust::store::object_path(&hash).unwrap());
                let inode = identity(&object);
                let project = if provider == "modrinth" {
                    "project"
                } else {
                    "42"
                };
                let version = if provider == "modrinth" { "Ver2" } else { "2" };
                let mut args = vec![
                    "demo".into(),
                    "--project".into(),
                    project.into(),
                    if server {
                        "--servers-directory".into()
                    } else {
                        "--instances-directory".into()
                    },
                    root.to_str().unwrap().into(),
                    "--store-directory".into(),
                    cache.to_str().unwrap().into(),
                    "--workers".into(),
                    "1".into(),
                    "--retries".into(),
                    "0".into(),
                ];
                if provider == "curseforge" {
                    args.extend(["--api-key".into(), "synthetic-key".into()]);
                }
                let mut pair = vec![];
                for z in [true, false] {
                    std::fs::create_dir_all(&game).unwrap();
                    if server {
                        std::fs::write(home.join("server.json"),br#"{"id":"demo","name":"Demo","version_id":"1.20.1","fabric_loader":"0.15.0"}"#).unwrap();
                    } else {
                        std::fs::write(home.join("instance.json"),br#"{"schema_version":1,"id":"demo","name":"Demo","version_id":"1.20.1","fabric_loader":"0.15.0","source":"official"}"#).unwrap();
                    }
                    let mut install_args = args.clone();
                    install_args.extend(["--provider".into(), provider.into()]);
                    let initial = if server {
                        let mut params = json!({"directory":root,"id":"demo","project":project,"provider":provider,"store_directory":cache,"workers":1,"retries":0});
                        if provider == "curseforge" {
                            params["api_key"] = json!("synthetic-key");
                        }
                        let req = json!({"protocol":1,"id":"install","method":"server.mods.install","params":params});
                        let out = if z {
                            invoke(&zig, &fixture, &f, &req, true)
                        } else {
                            let raw = RawValue::from_string(params.to_string()).unwrap();
                            let mut out = vec![];
                            jmcl_core_rust::server_content::handle_with(
                                "server.mods.install",
                                Some(&raw),
                                "install",
                                &mut out,
                                &f,
                                &f,
                            )
                            .unwrap()
                            .unwrap();
                            out
                        };
                        let mut relative = b"mods/".to_vec();
                        relative.extend_from_slice(&old);
                        assert_cached(&out, &relative);
                        (0, out, vec![])
                    } else {
                        cli_call(&zig, &fixture, &f, z, server, "install", &install_args)
                    };
                    if initial.0 != 0 {
                        #[cfg(unix)]
                        assert_eq!(
                            std::fs::File::create(game.join(native(&old)))
                                .unwrap_err()
                                .raw_os_error(),
                            Some(libc::EILSEQ)
                        );
                        #[cfg(windows)]
                        panic!("unexpected: {initial:?}");
                        std::fs::remove_dir_all(&root).unwrap();
                        continue;
                    }
                    let mut switch_args = args.clone();
                    switch_args.extend(["--version".into(), version.into()]);
                    let changed =
                        cli_call(&zig, &fixture, &f, z, server, "set-version", &switch_args);
                    assert_eq!(changed.0, 0, "{changed:?}");
                    assert!(changed.1.windows(new.len()).any(|s| s == new));
                    assert_eq!(identity(&game.join("mods").join(native(&new))), inode);
                    assert!(
                        matches!(std::fs::symlink_metadata(game.join("mods").join(native(&old))),Err(e)if e.kind()==std::io::ErrorKind::NotFound)
                    );
                    let mut params = if server {
                        json!({"directory":root,"id":"demo","project":project,"version_id":version,"store_directory":cache,"workers":1,"retries":0})
                    } else {
                        json!({"directory":game,"minecraft_version":"1.20.1","loader":"fabric","project":project,"version_id":version,"store_directory":cache,"workers":1,"retries":0})
                    };
                    if provider == "curseforge" {
                        params["api_key"] = json!("synthetic-key");
                    }
                    let method = if server {
                        "server.mods.set-version"
                    } else {
                        "mods.set-version"
                    };
                    let req = json!({"protocol":1,"id":"same","method":method,"params":params});
                    let repeated = if z || !server {
                        invoke(&zig, &fixture, &f, &req, z)
                    } else {
                        let raw = RawValue::from_string(params.to_string()).unwrap();
                        let mut out = vec![];
                        jmcl_core_rust::server_content::handle_with(
                            method,
                            Some(&raw),
                            "same",
                            &mut out,
                            &f,
                            &f,
                        )
                        .unwrap()
                        .unwrap();
                        out
                    };
                    assert_eq!(
                        records(&repeated).last().unwrap()["result"]["changed"],
                        false
                    );
                    pair.push((
                        initial,
                        changed,
                        repeated,
                        std::fs::read(game.join(".jmcl/content.json")).unwrap(),
                    ));
                    assert_eq!(identity(&object), inode);
                    assert!(std::fs::metadata(&object).unwrap().permissions().readonly());
                    std::fs::remove_dir_all(&root).unwrap();
                }
                if pair.len() == 2 {
                    assert_eq!(pair[0], pair[1]);
                }
            }
        }
    }
}

#[test]
fn public_string_provider_wrappers_preserve_typed_error_order() {
    for provider in ["modrinth", "curseforge"] {
        let mut f = docs("mods", provider);
        filename(&mut f, provider, b"\xff.jar");
        let body =
            f.0.iter()
                .find(|(k, _)| {
                    k.contains(if provider == "modrinth" {
                        "/version?"
                    } else {
                        "/files?"
                    })
                })
                .unwrap()
                .1;
        let mut v: serde_json::Value = serde_json::from_slice(body).unwrap();
        if provider == "modrinth" {
            let mut v = v[0].take();
            v["id"] = json!("");
            assert_eq!(
                modrinth::parse_version(&serde_json::to_vec(&v).unwrap())
                    .err()
                    .unwrap()
                    .0,
                "UnexpectedToken"
            );
            v["files"][0]["filename"] = json!("normal.jar");
            assert_eq!(
                modrinth::parse_version(&serde_json::to_vec(&v).unwrap())
                    .err()
                    .unwrap()
                    .0,
                "InvalidModrinthVersionId"
            );
        } else {
            let mut file = v["data"][0].take();
            file["id"] = json!(0);
            let mut v = json!({"data":file});
            assert_eq!(
                curseforge::parse_file(&serde_json::to_vec(&v).unwrap())
                    .err()
                    .unwrap()
                    .0,
                "UnexpectedToken"
            );
            v["data"]["fileName"] = json!("normal.jar");
            assert_eq!(
                curseforge::parse_file(&serde_json::to_vec(&v).unwrap())
                    .err()
                    .unwrap()
                    .0,
                "InvalidCurseforgeId"
            );
        }
    }
}
