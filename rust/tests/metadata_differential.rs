use jmcl_core_rust::{
    metadata::{self, Source},
    plan::{self, Target},
};
use serde_json::{Value, json};
use std::{fs, process::Command};
fn base() -> Value {
    json!({"id":"1.0","type":"release","mainClass":"example.Main","downloads":{"client":{"sha1":"1".repeat(40),"size":10,"url":"https://piston-data.mojang.com/client"}},"libraries":[]})
}
fn oracle(
    mode: &str,
    v: &Value,
    assets: Option<&Value>,
    source: Source,
    target: &Target,
) -> String {
    let executable = std::env::var("JMCL_ZIG_METADATA_ORACLE")
        .expect("set JMCL_ZIG_METADATA_ORACLE to compiled pinned fixture adapter");
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("metadata.json");
    fs::write(&path, v.to_string()).unwrap();
    let mut cmd = Command::new(executable);
    cmd.arg(mode).arg(path);
    if matches!(mode, "plan" | "server") {
        if let Some(assets) = assets {
            let path = tmp.path().join("assets.json");
            fs::write(&path, assets.to_string()).unwrap();
            cmd.arg(path);
        } else {
            cmd.arg("-");
        }
        cmd.args([source.name(), &target.os, &target.arch]);
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim_end().into()
}
fn target() -> Target {
    Target {
        os: "osx".into(),
        arch: "arm64".into(),
        os_version: None,
    }
}
#[test]
#[ignore = "requires pinned Zig metadata oracle adapter"]
fn metadata_and_plans_match_zig() {
    let mut modern = base();
    modern["javaVersion"] = json!({"component":"runtime","majorVersion":"21"});
    modern["arguments"] = json!({"game":["--username",{"value":["x","y"],"rules":[{"action":"allow","features":{"demo":true}}]}],"jvm":[],"default-user-jvm":[{"value":"-Xmx2G"}]});
    modern["libraries"] = json!([{"name":"demo:common:1","downloads":{"artifact":{"path":"demo/common.jar","sha1":"2".repeat(40),"size":20,"url":"https://libraries.minecraft.net/common"}}},{"name":"demo:native:1","downloads":{"classifiers":{"natives-osx-64":{"path":"demo/native.jar","sha1":"3".repeat(40),"size":30,"url":"https://libraries.minecraft.net/native"}}},"natives":{"osx":"natives-osx-${arch}"},"extract":{"exclude":["META-INF/"]}},{"name":"not-matching","rules":[{"action":"allow","os":{"name":"windows"}}]}]);
    modern["assetIndex"] = json!({"id":"legacy","sha1":"4".repeat(40),"size":40,"totalSize":100,"url":"https://piston-meta.mojang.com/assets"});
    modern["logging"] = json!({"client":{"argument":"log","file":{"id":"log.xml","sha1":"5".repeat(40),"size":50,"url":"https://piston-data.mojang.com/log"},"type":"log4j2-xml"}});
    let assets = json!({"objects":{"sound/z":{"hash":"6".repeat(40),"size":60},"sound/a":{"hash":"6".repeat(40),"size":60}},"virtual":true,"map_to_resources":true});
    for v in [base(), modern.clone()] {
        let normalized = metadata::metadata(v.to_string().as_bytes()).unwrap();
        assert_eq!(
            serde_json::to_string(&normalized).unwrap(),
            oracle("metadata", &v, None, Source::Official, &target())
        );
        for source in [Source::Official, Source::Bmclapi] {
            let normalized_assets = if v.get("assetIndex").is_some() {
                Some(metadata::asset_index(assets.to_string().as_bytes()).unwrap())
            } else {
                None
            };
            let p = plan::build(&normalized, normalized_assets.as_ref(), source, target()).unwrap();
            assert_eq!(
                serde_json::to_string(&p).unwrap(),
                oracle("plan", &v, normalized_assets.as_ref(), source, &target())
            );
        }
    }
    for (field, value, error) in [
        ("id", json!("../escape"), "UnsafePath"),
        ("patched_client", json!("NUL.txt"), "UnsafePath"),
    ] {
        let mut v = base();
        v[field] = value;
        let normalized = metadata::metadata(v.to_string().as_bytes()).unwrap();
        assert_eq!(
            plan::build(&normalized, None, Source::Official, target())
                .unwrap_err()
                .name(),
            error
        );
        assert_eq!(
            oracle("plan", &v, None, Source::Official, &target()),
            json!({"error":error}).to_string()
        );
    }
    let mut server = base();
    server["downloads"]["server"] =
        json!({"sha1":"7".repeat(40),"size":77,"url":"https://piston-data.mojang.com/server"});
    let normalized = metadata::metadata(server.to_string().as_bytes()).unwrap();
    assert_eq!(
        serde_json::to_string(
            &plan::build_server(&normalized, Source::Bmclapi, target(), &[]).unwrap()
        )
        .unwrap(),
        oracle("server", &server, None, Source::Bmclapi, &target())
    );
    let manifest = json!({"latest":{"release":"r","snapshot":"s"},"versions":[{"id":"r","type":"release","url":"https://example.invalid/r","time":"now","releaseTime":"then","complianceLevel":"1"},{"id":"s","type":"snapshot","url":"https://example.invalid/s","time":"now","releaseTime":"then"}]});
    assert_eq!(
        serde_json::to_string(&metadata::manifest(manifest.to_string().as_bytes()).unwrap())
            .unwrap(),
        oracle("manifest", &manifest, None, Source::Official, &target())
    );
    assert_eq!(
        serde_json::to_string(&metadata::asset_index(assets.to_string().as_bytes()).unwrap())
            .unwrap(),
        oracle("assets", &assets, None, Source::Official, &target())
    );
}
#[test]
#[ignore = "requires pinned Zig metadata oracle adapter"]
fn fabric_matches_zig() {
    use jmcl_core_rust::version::fabric;
    let p = json!({"id":"fabric-loader-0.16.10-1.0","inheritsFrom":"1.0","mainClass":"fabric.Main","libraries":[{"name":"net.fabricmc:loader:0.16.10:client","url":"https://maven.fabricmc.net/"}],"arguments":{"game":["--fabric"],"jvm":["-Dfabric=true"],"default-user-jvm":["-Xmx2G"]}});
    let parsed = fabric::parse(p.to_string().as_bytes()).unwrap();
    assert_eq!(
        parsed.to_string(),
        oracle("fabric", &p, None, Source::Official, &target())
    );
    for args in [
        Value::Null,
        json!({"game":["--parent"],"jvm":["-Dparent=true"],"default-user-jvm":["-Xms1G"]}),
    ] {
        let mut parent = base();
        parent["arguments"] = args;
        let parent = metadata::metadata(parent.to_string().as_bytes()).unwrap();
        let integrities = vec![("2".repeat(40), 30)];
        let merged = fabric::merge(&parent, &parsed, &integrities).unwrap();
        let fixture =
            json!({"parent":parent,"profile":p,"integrities":[{"sha1":"2".repeat(40),"size":30}]});
        assert_eq!(
            merged.to_string(),
            oracle("fabric-merge", &fixture, None, Source::Official, &target())
        );
    }
    for (field, value) in [
        ("id", json!("../bad")),
        ("mainClass", json!("bad/class")),
        (
            "libraries",
            json!([{"name":"bad","url":"https://maven.fabricmc.net/"}]),
        ),
        ("libraries", json!([{"name":"a:b:c","url":"http://repo/"}])),
    ] {
        let mut invalid = p.clone();
        invalid[field] = value;
        let error = fabric::parse(invalid.to_string().as_bytes()).unwrap_err();
        assert_eq!(
            json!({"error":error.name()}).to_string(),
            oracle("fabric", &invalid, None, Source::Official, &target())
        );
    }
}
#[test]
#[ignore = "requires pinned Zig metadata oracle adapter"]
fn neoforge_profiles_match_zig() {
    use jmcl_core_rust::version::neoforge as n;
    let lib = json!({"name":"net.neoforged:loader:1","downloads":{"artifact":{"path":"ignored.jar","sha1":"3".repeat(40),"size":33,"url":"https://example.invalid/loader.jar"}}});
    let profile = json!({"id":"neoforge-1","inheritsFrom":"1.0","mainClass":"Neo.Main","libraries":[lib.clone()],"arguments":{"game":["neo"],"jvm":[]}});
    let install = json!({"spec":1,"version":"neoforge-1","minecraft":"1.0","data":{"PATCHED":{"client":"[net.neoforged:neoforge:1:client]","server":"[net.neoforged:neoforge:1:server]"},"BINPATCH":{"client":"/data/client.lzma"}},"processors":[{"jar":"org.example:processor:1","args":["--task","{PATCHED}"],"sides":["client"]}],"libraries":[lib]});
    let parsed = n::version(profile.to_string().as_bytes()).unwrap();
    assert_eq!(
        parsed.to_string(),
        oracle("neo-version", &profile, None, Source::Official, &target())
    );
    let parsed_install = n::install(install.to_string().as_bytes()).unwrap();
    assert_eq!(
        parsed_install.to_string(),
        oracle("neo-install", &install, None, Source::Official, &target())
    );
    let parent = metadata::metadata(base().to_string().as_bytes()).unwrap();
    let patched = n::patched_path(&parsed_install, false).unwrap();
    let merged = n::merge(&parent, &parsed, &patched).unwrap();
    let fixture = json!({"parent":parent,"profile":profile,"patched":patched});
    assert_eq!(
        merged.to_string(),
        oracle("neo-merge", &fixture, None, Source::Official, &target())
    );
    for (field, value) in [
        ("spec", json!(2)),
        ("minecraft", json!("bad/parent")),
        ("data", json!({"BAD-KEY":{"client":"x"}})),
        ("data", json!({"A":{"client":"/../escape"}})),
        ("processors", json!([{"jar":"a:b:c","sides":["other"]}])),
    ] {
        let mut invalid = install.clone();
        invalid[field] = value;
        let e = n::install(invalid.to_string().as_bytes()).unwrap_err();
        assert_eq!(
            json!({"error":e.name()}).to_string(),
            oracle("neo-install", &invalid, None, Source::Official, &target())
        );
    }
}
fn stored_zip(name: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut local = vec![0; 30];
    local[..4].copy_from_slice(b"PK\x03\x04");
    local[18..22].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    local[22..26].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    local[26..28].copy_from_slice(&(name.len() as u16).to_le_bytes());
    local.extend(name);
    local.extend(payload);
    let offset = local.len();
    let mut central = vec![0; 46];
    central[..4].copy_from_slice(b"PK\x01\x02");
    central[20..24].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    central[24..28].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    central[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
    central.extend(name);
    let size = central.len();
    local.extend(central);
    let mut end = vec![0; 22];
    end[..4].copy_from_slice(b"PK\x05\x06");
    end[8..10].copy_from_slice(&1u16.to_le_bytes());
    end[10..12].copy_from_slice(&1u16.to_le_bytes());
    end[12..16].copy_from_slice(&(size as u32).to_le_bytes());
    end[16..20].copy_from_slice(&(offset as u32).to_le_bytes());
    local.extend(end);
    local
}
#[test]
#[ignore = "requires pinned Zig metadata oracle adapter"]
fn installer_zip_matches_zig() {
    use jmcl_core_rust::version::zip;
    let archive = stored_zip(b"version.json", b"profile-data");
    let mut encrypted = archive.clone();
    encrypted[6] = 1;
    let mut unsupported = archive.clone();
    let central = 30 + 12 + 12;
    unsupported[central + 10] = 99;
    for (bytes, name, max) in [
        (archive.clone(), "version.json", 100),
        (archive.clone(), "missing", 100),
        (archive.clone(), "version.json", 3),
        (encrypted, "version.json", 100),
        (unsupported, "version.json", 100),
        (stored_zip(b"bad\\name", b"data"), "version.json", 100),
        (vec![1, 2, 3], "version.json", 100),
    ] {
        let result = match zip::read_entry(&bytes, name, max) {
            Ok(Some(v)) => String::from_utf8(v).map(Value::String).unwrap(),
            Ok(None) => Value::Null,
            Err(e) => json!({"error":e.name()}),
        };
        let fixture = json!({"archive":bytes,"name":name,"max":max});
        assert_eq!(
            result.to_string(),
            oracle("zip-entry", &fixture, None, Source::Official, &target())
        );
    }
}
#[test]
#[ignore = "requires pinned Zig metadata oracle adapter"]
fn forge_profiles_match_zig() {
    use jmcl_core_rust::version::forge;
    let legacy = json!({"install":{"path":"net.minecraftforge:forge:1-2","minecraft":"1"},"versionInfo":{"id":"1-Forge2","mainClass":"Legacy.Main","minecraftArguments":"--legacy true","libraries":[{"name":"net.minecraftforge:forge:1-2","url":"http://files.minecraftforge.net/maven/","checksums":["1".repeat(40)]}]}});
    assert_eq!(
        forge::legacy(legacy.to_string().as_bytes())
            .unwrap()
            .to_string(),
        oracle("forge-legacy", &legacy, None, Source::Official, &target())
    );
    for url in ["http://unknown.invalid/", "https:///", "https://host/../"] {
        let mut v = legacy.clone();
        v["versionInfo"]["libraries"][0]["url"] = url.into();
        let e = forge::legacy(v.to_string().as_bytes()).unwrap_err();
        assert_eq!(
            json!({"error":e.name()}).to_string(),
            oracle("forge-legacy", &v, None, Source::Official, &target())
        );
    }
    let mut profile = json!({"id":"1-forge-2","inheritsFrom":"1","mainClass":"Modern.Main","libraries":[{"name":"net.minecraftforge:forge:1-2:client","downloads":{"artifact":{"path":"ignored","sha1":"2".repeat(40),"size":22,"url":""}}}]});
    for patched in ["net.minecraftforge:forge:1-2:client", "other:library:1"] {
        let fixture = json!({"profile":profile,"patched":patched});
        let result = forge::modern(profile.to_string().as_bytes(), patched)
            .unwrap_or_else(|e| json!({"error":e.name()}));
        assert_eq!(
            result.to_string(),
            oracle("forge-modern", &fixture, None, Source::Official, &target())
        );
    }
    profile["libraries"][0]["downloads"]["artifact"]["url"] = "http://unknown.invalid/a".into();
    let patched = "net.minecraftforge:forge:1-2:client";
    let fixture = json!({"profile":profile,"patched":patched});
    let e = forge::modern(profile.to_string().as_bytes(), patched).unwrap_err();
    assert_eq!(
        json!({"error":e.name()}).to_string(),
        oracle("forge-modern", &fixture, None, Source::Official, &target())
    );
}
