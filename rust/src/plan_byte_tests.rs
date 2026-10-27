use super::*;
use crate::metadata;
use serde_json::json;
fn target() -> Target {
    Target {
        os: "linux".into(),
        arch: "x86_64".into(),
        os_version: None,
    }
}
fn raw(id: Value) -> Value {
    json!({"id":id,"type":"release","mainClass":"Main","libraries":[],"downloads":{"client":{"url":"https://piston-data.mojang.com/client","sha1":"a".repeat(40),"size":10}}})
}
fn normalize(v: &Value) -> Value {
    metadata::metadata(&serde_json::to_vec(v).unwrap()).unwrap()
}
fn rich(id: Value) -> Value {
    let mut v = raw(id);
    v["assetIndex"] = json!({"id":"index","url":"https://example.invalid/index","sha1":"b".repeat(40),"size":2,"totalSize":3});
    v["logging"] = json!({"client":{"argument":"-Dlog=${path}","type":"log4j2-xml","file":{"id":"log.xml","url":"https://example.invalid/log","sha1":"c".repeat(40),"size":4}}});
    v["libraries"] = json!([{"name":"example:lib:1","downloads":{"artifact":{"path":"example/lib.jar","url":"https://example.invalid/lib","sha1":"d".repeat(40),"size":5}}}]);
    v
}
fn assets() -> Value {
    metadata::asset_index(&serde_json::to_vec(&json!({"objects":{"sound/a":{"hash":"e".repeat(40),"size":3}},"virtual":true,"map_to_resources":true})).unwrap()).unwrap()
}
fn string_cases() -> Value {
    let mut results = Vec::new();
    for (id, source) in [("1.21", Source::Official), ("版本", Source::Bmclapi)] {
        results.push(json!({"name":id,"result":serde_json::to_string(&build(&normalize(&raw(id.into())),None,source,target()).unwrap()).unwrap()}));
    }
    let mut v = rich("demo".into());
    results.push(json!({"name":"rich","result":serde_json::to_string(&build(&normalize(&v),Some(&assets()),Source::Official,target()).unwrap()).unwrap()}));
    v["patched_client"] = "patched/client.jar".into();
    results.push(json!({"name":"patched","result":serde_json::to_string(&build(&normalize(&v),Some(&assets()),Source::Bmclapi,target()).unwrap()).unwrap()}));
    for (name, id, url, hash) in [
        ("path-first", "../bad", "http://x", "bad"),
        ("url-first", "demo", "http://x", "bad"),
        ("hash", "demo", "https://x", "bad"),
    ] {
        let mut v = raw(id.into());
        v["downloads"]["client"]["url"] = url.into();
        v["downloads"]["client"]["sha1"] = hash.into();
        results.push(json!({"name":name,"error":build(&normalize(&v),None,Source::Official,target()).unwrap_err().name()}));
    }
    results.push(json!({"name":"unexpected-assets","error":build(&normalize(&raw("demo".into())),Some(&assets()),Source::Official,target()).unwrap_err().name()}));
    Value::Array(results)
}
#[test]
fn ordinary_string_plan_baseline_is_complete() {
    assert_eq!(
        string_cases(),
        serde_json::from_str::<Value>(include_str!("../compat/plan-string-baseline.json")).unwrap()
    );
}

fn read(v: &Value) -> metadata::ReadMetadata {
    metadata::ReadMetadata::parse(&serde_json::to_vec(v).unwrap()).unwrap()
}
#[test]
fn read_plan_matches_complete_string_outputs_and_only_varies_version_bytes() {
    for (id, source) in [("1.21", Source::Official), ("版本", Source::Bmclapi)] {
        let v = rich(id.into());
        let bytes =
            serde_json::to_vec(&build_read(&read(&v), Some(&assets()), source, target()).unwrap())
                .unwrap();
        assert_eq!(
            bytes,
            serde_json::to_vec(&build(&normalize(&v), Some(&assets()), source, target()).unwrap())
                .unwrap()
        );
    }
    let mut v = rich(json!([255, 128]));
    for patched in [false, true] {
        if patched {
            v["patched_client"] = "patched/client.jar".into();
        }
        let result = serde_json::to_value(
            build_read(&read(&v), Some(&assets()), Source::Bmclapi, target()).unwrap(),
        )
        .unwrap();
        let mut ordinary = v.clone();
        ordinary["id"] = "demo".into();
        let mut expected = serde_json::to_value(
            build(
                &normalize(&ordinary),
                Some(&assets()),
                Source::Bmclapi,
                target(),
            )
            .unwrap(),
        )
        .unwrap();
        let path = [
            b"versions/".as_slice(),
            &[255, 128],
            b"/",
            &[255, 128],
            b".jar",
        ]
        .concat();
        expected["version_id"] = json!([255, 128]);
        expected["files"][0]["path"] = json!(path);
        if !patched {
            let last = expected["classpath"].as_array().unwrap().len() - 1;
            expected["classpath"][last] = json!(path);
        }
        assert_eq!(result, expected);
        assert!(result["native_extractions"].as_array().unwrap().is_empty());
    }
}
#[test]
fn read_plan_validation_order_and_nested_string_scope_stay_intact() {
    for (id, url, hash, error) in [
        (json!([255, 47]), "http://x", "bad", "UnsafePath"),
        (json!([255]), "http://x", "bad", "InsecureDownloadUrl"),
        (json!([255]), "https://x", "bad", "InvalidSha1"),
    ] {
        let mut v = raw(id);
        v["downloads"]["client"]["url"] = url.into();
        v["downloads"]["client"]["sha1"] = hash.into();
        assert_eq!(
            build_read(&read(&v), None, Source::Official, target()).unwrap_err(),
            err(error)
        );
    }
    let mut v = rich(json!([255]));
    v["logging"]["client"]["file"]["id"] = json!([255]);
    assert_eq!(
        metadata::ReadMetadata::parse(&serde_json::to_vec(&v).unwrap()).err(),
        Some(Error::Json("UnexpectedToken"))
    );
    let mut v = raw(json!([255]));
    v["patched_client"] = "../bad".into();
    assert_eq!(
        build_read(&read(&v), None, Source::Official, target()).unwrap_err(),
        err("UnsafePath")
    );
    assert_eq!(
        build_read(
            &read(&raw(json!([255]))),
            Some(&assets()),
            Source::Official,
            target()
        )
        .unwrap_err(),
        err("UnexpectedAssetIndex")
    );
}
#[test]
fn byte_builder_validates_before_dedup_and_retains_first_order() {
    let v = normalize(&raw("demo".into()));
    let mut b = Builder::with_id(&v, &[255], Source::Official, target());
    let p = PlanBytes(b"versions/\xff/\xff.jar".to_vec());
    let hash = "a".repeat(40);
    b.add_file(
        p.clone(),
        "https://first",
        &hash,
        3,
        Usage {
            client: true,
            ..Default::default()
        },
    )
    .unwrap();
    b.classpath(p.clone());
    b.add_file(
        p.clone(),
        "https://second",
        &"A".repeat(40),
        3,
        Usage {
            library: true,
            ..Default::default()
        },
    )
    .unwrap();
    b.classpath(p.clone());
    b.classpath("libraries/x.jar".into());
    b.classpath(p.clone());
    assert_eq!(b.plan.files.len(), 1);
    assert_eq!(b.plan.files[0].url, "https://first");
    assert_eq!(b.plan.files[0].sha1, hash);
    assert_eq!(b.plan.total_download_size, 3);
    assert!(b.plan.files[0].usage.client && b.plan.files[0].usage.library);
    assert_eq!(b.plan.classpath, vec![p.clone(), "libraries/x.jar".into()]);
    for (url, sha, size, error) in [
        ("http://x", "bad", 4, "InsecureDownloadUrl"),
        ("https://x", "bad", 4, "InvalidSha1"),
        ("https://x", hash.as_str(), 4, "ConflictingDestination"),
    ] {
        assert_eq!(
            b.add_file(p.clone(), url, sha, size, Usage::default()),
            Err(err(error))
        );
    }
    assert_eq!(
        b.add_file(
            PlanBytes(b"\xff/../x".to_vec()),
            "http://x",
            "bad",
            0,
            Usage::default()
        ),
        Err(err("UnsafePath"))
    );
    assert_eq!(b.plan.files.len(), 1);
}
#[test]
fn byte_builder_file_limit_zero_size_and_overflow_remain_ordered() {
    let v = normalize(&raw("demo".into()));
    let hash = "a".repeat(40);
    let mut b = Builder::new(&v, Source::Official, target());
    for i in 0..10_000 {
        b.add_file(
            format!("f{i}").into(),
            "https://x",
            &hash,
            0,
            Usage::default(),
        )
        .unwrap();
    }
    assert_eq!(b.plan.total_download_size, 0);
    b.add_file("f0".into(), "https://x", &hash, 0, Usage::default())
        .unwrap();
    assert_eq!(
        b.add_file("new".into(), "https://x", "bad", 0, Usage::default()),
        Err(err("InvalidSha1"))
    );
    assert_eq!(
        b.add_file("new".into(), "https://x", &hash, 0, Usage::default()),
        Err(err("TooManyFiles"))
    );
    let mut b = Builder::new(&v, Source::Official, target());
    b.add_file("a".into(), "https://x", &hash, u64::MAX, Usage::default())
        .unwrap();
    assert_eq!(
        b.add_file("b".into(), "https://x", &hash, 1, Usage::default()),
        Err(err("DownloadSizeOverflow"))
    );
    // Existing insertion occurs before checked addition; preserve this internal order.
    assert_eq!(b.plan.files.len(), 2);
    assert_eq!(b.plan.total_download_size, u64::MAX);
}
#[test]
fn byte_relative_validator_preserves_string_domain_and_opaque_components() {
    for v in [
        "a/b",
        "版本/é",
        ".hidden/a",
        "",
        "/a",
        "a/",
        "a//b",
        "../a",
        "a\\b",
        "a/CON.txt",
        "a:b",
        "a/\0",
    ] {
        assert_eq!(
            validate_relative_path(v),
            validate_relative_path_bytes(v.as_bytes())
        );
    }
    assert_eq!(
        validate_relative_path_bytes(b"versions/\xff/\xff.jar"),
        Ok(())
    );
    for v in [
        b"\xff/..".as_slice(),
        b"/\xff",
        b"\xff//x",
        b"\xff/CON",
        b"\xff\\x",
        b"\xff/\0",
    ] {
        assert_eq!(validate_relative_path_bytes(v), Err(err("UnsafePath")));
    }
}
