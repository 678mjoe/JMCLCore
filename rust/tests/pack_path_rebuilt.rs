//! Pure schema/planning evidence freshly rebuilt from a4ad20ac.
use jmcl_core_rust::modpack::{
    Error,
    planning::{CurseforgeProvider, Downloads, Project, ProviderFile},
    provider,
    schema::{Curseforge, Mrpack},
};
use serde_json::{Value, json};
fn file(path: Value) -> Value {
    json!({"path":path,"hashes":{"sha1":"1111111111111111111111111111111111111111"},"downloads":["https://example.test/a","https://example.test/b"],"fileSize":1})
}
fn pack(files: Vec<Value>) -> Result<Mrpack, Error> {
    Mrpack::parse(&serde_json::to_vec(&json!({"formatVersion":1,"game":"minecraft","name":"fixture","files":files,"dependencies":{"minecraft":"1.21"}})).unwrap())
}
fn provider_file(name: Value) -> Result<ProviderFile, Error> {
    provider::file(&serde_json::to_vec(&json!({"data":{"id":1,"modId":1,"fileName":name,"releaseType":1,"hashes":[{"value":"1111111111111111111111111111111111111111","algo":1}],"fileDate":"","fileLength":1,"downloadUrl":"https://example.test/a","gameVersions":["1.21"]}})).unwrap())
}
#[test]
fn path_bytes_plan_registry_and_utf8() {
    for (path, expected) in [
        (
            json!([109, 111, 100, 115, 47, 255]),
            json!([109, 111, 100, 115, 47, 255]),
        ),
        (json!([109, 111, 100, 115, 47, 195, 169]), json!("mods/é")),
        (json!("mods/a"), json!("mods/a")),
    ] {
        let d = Downloads::mrpack(&pack(vec![file(path)]).unwrap());
        let p = serde_json::to_value(d.plan("1.21").unwrap()).unwrap();
        assert_eq!(p["files"][0]["path"], expected);
        assert_eq!(p["files"][0]["usage"]["mod"], true);
        assert_eq!(p["files"][0]["fallback_url"], "https://example.test/b");
        assert_eq!(p["version_id"], "1.21");
        assert_eq!(p["source"], "official");
        assert_eq!(p["loader"], "vanilla");
        assert_eq!(p["java_major_version"], 0);
        assert_eq!(p["total_download_size"], 1);
        let r = jmcl_core_rust::content::registry::RegistryRecord {
            schema_version: 1,
            entries: d.entries,
        };
        r.validate().unwrap();
        assert_eq!(
            serde_json::to_value(r).unwrap()["entries"][0]["file"],
            expected
        );
    }
}
#[test]
fn path_validation_duplicate_skip_and_prefix_order() {
    let mut a = file(json!([109, 111, 100, 115, 47, 255]));
    a["env"] = json!({"client":"unsupported"});
    let mut b = a.clone();
    b["hashes"]["sha1"] = json!("");
    assert_eq!(
        pack(vec![a.clone(), b]).unwrap_err().name(),
        "DuplicateModpackFile"
    );
    let d = Downloads::mrpack(
        &pack(vec![
            a,
            file(json!([109, 111, 100, 115, 255, 47, 97])),
            file(json!("resourcepacks/a")),
            file(json!("shaderpacks/b")),
        ])
        .unwrap(),
    );
    assert_eq!(d.skipped, 1);
    assert_eq!(d.entries.len(), 2);
    assert!(!d.files[0].usage.mod_file);
    for path in [
        json!([255, 0]),
        json!("mods/CON.jar"),
        json!("/a"),
        json!("a/"),
        json!("a\\b"),
        json!("a/../b"),
    ] {
        let mut f = file(path);
        f["fileSize"] = json!(0);
        assert_eq!(pack(vec![f]).unwrap_err().name(), "UnsafePath");
    }
    assert!(pack(vec![file(json!(vec![255; 512]))]).is_ok());
    assert_eq!(
        pack(vec![file(json!(vec![255; 513]))]).unwrap_err().name(),
        "InvalidModpackFile"
    );
    assert!(pack(vec![file(json!(["109", 111.0, 100, 115, 47, 255]))]).is_ok());
    for path in [json!([256]), json!([65.5]), Value::Null, json!({})] {
        assert!(pack(vec![file(path)]).is_err());
    }
}
struct Fake {
    calls: usize,
}
impl CurseforgeProvider for Fake {
    fn project(&mut self, _: u32) -> Result<Project, Error> {
        self.calls += 1;
        Ok(Project {
            class_id: 6,
            slug: "fixture".into(),
        })
    }
    fn file(&mut self, _: u32, _: u32) -> Result<ProviderFile, Error> {
        self.calls += 1;
        provider_file(json!([255]))
    }
    fn download_url(&mut self, _: u32, _: u32) -> Result<Option<String>, Error> {
        panic!("existing URL")
    }
}
fn cf(files: Value) -> Curseforge {
    Curseforge::parse(&serde_json::to_vec(&json!({"manifestType":"minecraftModpack","manifestVersion":1,"name":"fixture","minecraft":{"version":"1.21","modLoaders":[]},"files":files})).unwrap()).unwrap()
}
#[test]
fn provider_name_bytes_and_planning() {
    let mut p = Fake { calls: 0 };
    let pack = cf(json!([{"projectID":1,"fileID":1,"required":false},{"projectID":1,"fileID":1}]));
    let d = Downloads::curseforge(&pack, &mut p).unwrap();
    assert_eq!(d.skipped, 1);
    assert_eq!(p.calls, 2);
    let v = serde_json::to_value(d.plan("1.21").unwrap()).unwrap();
    assert_eq!(v["files"][0]["path"], json!([109, 111, 100, 115, 47, 255]));
    assert_eq!(
        serde_json::to_value(d.entries).unwrap()[0]["file"],
        v["files"][0]["path"]
    );
    let pack = cf(json!([{"projectID":1,"fileID":1},{"projectID":1,"fileID":2}]));
    assert_eq!(
        Downloads::curseforge(&pack, &mut p).unwrap_err().name(),
        "DuplicateModpackFile"
    );
    for n in [
        json!([0]),
        json!([]),
        json!("CON.jar"),
        json!("x/y"),
        json!("x."),
        json!(vec![255; 257]),
    ] {
        assert_eq!(
            provider_file(n).err().unwrap().name(),
            "InvalidCurseforgeFilename"
        );
    }
    assert!(provider_file(json!(vec![255; 256])).is_ok());
}
