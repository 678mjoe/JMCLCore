use super::*;
use serde_json::{Value, json};
use std::{cell::RefCell, fs, path::Path, process::Command};
fn file() -> Value {
    json!({"id":1,"modId":42,"fileName":"a.jar","releaseType":1,"hashes":[{"algo":1,"value":"a".repeat(40)}],"fileDate":"2024","fileLength":7,"downloadUrl":null,"gameVersions":["1.20.1","Fabric"]})
}
fn parsed(bytes: &[u8]) -> Value {
    parse_page(bytes).map(|(f,p)|json!({"ids":f.iter().map(|f|f.id).collect::<Vec<_>>(),"pagination":p.map(|p|json!({"index":p.index,"pageSize":p.page_size,"resultCount":p.result_count,"totalCount":p.total}))})).unwrap_or_else(|e|json!({"error":e.0}))
}
#[test]
fn complete_page_typing_precedes_earlier_item_semantics() {
    let mut first = file();
    first["id"] = json!(0);
    let mut second = file();
    second.as_object_mut().unwrap().remove("fileName");
    let bytes = serde_json::to_vec(&json!({"data":[first,second]})).unwrap();
    assert_eq!(parsed(&bytes), json!({"error":"MissingField"}));
}
#[test]
#[ignore = "requires pinned JMCL_ZIG_SOURCE and zig; pure page parser only"]
fn typed_page_matches_pinned_error_order_and_boundaries() {
    let source = std::env::var("JMCL_ZIG_SOURCE").unwrap();
    let head = Command::new("git")
        .args(["-C", &source, "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        "5a4310debbcee26b3d2391bab2339f79cd6ec52e"
    );
    assert!(
        Command::new("git")
            .args(["-C", &source, "diff", "--quiet", "HEAD", "--", "src"])
            .status()
            .unwrap()
            .success()
    );
    let temp = tempfile::tempdir().unwrap();
    let exe = temp.path().join(if cfg!(windows) {
        "oracle.exe"
    } else {
        "oracle"
    });
    let build = Command::new("zig")
        .args(["build-exe", "--dep", "jmcl_core"])
        .arg(format!(
            "-Mroot={}/tests/fixtures/curseforge_metadata_bytes.zig",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(format!(
            "-Mjmcl_core={}",
            Path::new(&source).join("src/root.zig").display()
        ))
        .arg(format!("-femit-bin={}", exe.display()))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let input = temp.path().join("input.json");
    let mut count = 0;
    let mut check = |bytes: Vec<u8>| {
        fs::write(&input, &bytes).unwrap();
        let before = fs::metadata(&input).unwrap().modified().unwrap();
        let out = Command::new(&exe)
            .args(["page", input.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            parsed(&bytes),
            expected,
            "case {count}, prefix {:?}",
            &bytes[..bytes.len().min(1024)]
        );
        assert_eq!(fs::read(&input).unwrap(), bytes);
        assert_eq!(fs::metadata(&input).unwrap().modified().unwrap(), before);
        count += 1;
    };
    for raw in [
        "null",
        "[]",
        "{}",
        r#"{"data":null}"#,
        r#"{"data":{}}"#,
        r#"{"data":[],"data":[]}"#,
        r#"{"data":[],"pagination":{},"pagination":{}}"#,
    ] {
        check(raw.as_bytes().to_vec());
    }
    let mut oversized = vec![b' '; 8 * 1024 * 1024 + 1];
    oversized[..11].copy_from_slice(b"{\"data\":[]}");
    check(oversized);
    for length in [0, 1, 49, 50, 51] {
        for mode in ["valid", "semantic_first", "typed_last", "both"] {
            let mut files = vec![file(); length];
            if length > 0 {
                if matches!(mode, "semantic_first" | "both") {
                    files[0]["id"] = json!(0);
                }
                if matches!(mode, "typed_last" | "both") {
                    files
                        .last_mut()
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .remove("fileName");
                }
            }
            check(serde_json::to_vec(&json!({"data":files})).unwrap());
        }
    }
    for field in ["index", "pageSize", "resultCount", "totalCount"] {
        for token in [
            "0",
            "1",
            "4294967295",
            "4294967296",
            "1.5",
            "1.0000000000000001",
            "\"1\"",
            "\"+1_0\"",
            "\"1.0\"",
            "true",
            "null",
        ] {
            for pagination_first in [false, true] {
                for bad_data in [false, true] {
                    let mut f = file();
                    if bad_data {
                        f["id"] = json!(true);
                    }
                    let data = serde_json::to_string(&vec![f]).unwrap();
                    let pagination = format!("{{\"{field}\":{token}}}");
                    let bytes = if pagination_first {
                        format!("{{\"pagination\":{pagination},\"data\":{data}}}")
                    } else {
                        format!("{{\"data\":{data},\"pagination\":{pagination}}}")
                    };
                    check(bytes.into_bytes());
                }
            }
        }
    }
    for pagination in [
        Value::Null,
        json!({}),
        json!({"totalCount":50}),
        json!({"index":0,"pageSize":50,"resultCount":1,"totalCount":100}),
    ] {
        check(serde_json::to_vec(&json!({"data":[file()],"pagination":pagination})).unwrap());
    }
    for semantic in [false, true] {
        let mut f = file();
        if semantic {
            f["id"] = json!(0);
        }
        let data = serde_json::to_vec(&vec![f]).unwrap();
        for raw in [
            b"\"\xff\"".as_slice(),
            br#""\ud800""#,
            b"1e400",
            br#""\\ud800""#,
        ] {
            for first in [false, true] {
                let bytes = if first {
                    [b"{\"unknown\":".as_slice(), raw, b",\"data\":", &data, b"}"].concat()
                } else {
                    [b"{\"data\":".as_slice(), &data, b",\"unknown\":", raw, b"}"].concat()
                };
                check(bytes);
            }
        }
    }
    eprintln!("{count} exact complete-page grammar/order/limit cases matched");
}
#[test]
fn injected_metadata_page_loop_preserves_stop_and_aggregate_rules() {
    struct Mock {
        replies: Vec<Vec<u8>>,
        calls: RefCell<Vec<String>>,
    }
    impl Metadata for Mock {
        fn get(&self, url: &str, key: &str, limit: usize) -> mr::Result<Vec<u8>> {
            assert_eq!(key, "owned-fixture-key");
            assert_eq!(limit, 8 * 1024 * 1024);
            let mut calls = self.calls.borrow_mut();
            let i = calls.len();
            calls.push(url.to_owned());
            Ok(self.replies[i.min(self.replies.len() - 1)].clone())
        }
    }
    let page = |n, total| {
        serde_json::to_vec(&json!({"data":vec![file();n],"pagination":{"totalCount":total}}))
            .unwrap()
    };
    for (replies, expected, calls) in [
        (vec![page(50, 51), page(1, 51)], Ok(51), 2),
        (vec![page(0, 100)], Ok(0), 1),
        (
            vec![serde_json::to_vec(&json!({"data":vec![file();50]})).unwrap()],
            Ok(50),
            1,
        ),
        (vec![page(50, 1000)], Err("TooManyCurseforgeFiles"), 11),
        (vec![page(51, 100)], Err("TooManyCurseforgeFiles"), 1),
    ] {
        let mock = Mock {
            replies,
            calls: RefCell::new(vec![]),
        };
        let registry = Registry::default();
        let resolver = Resolver {
            transport: &mock,
            key: "owned-fixture-key",
            kind: Kind::Mod,
            game: "1.20.1",
            loader: Some("fabric"),
            dependencies: false,
            registry: &registry,
            seen: HashSet::new(),
            result: Resolution {
                files: vec![],
                skipped: 0,
            },
        };
        let actual = resolver.pages("42").map(|f| f.len()).map_err(|e| e.0);
        assert_eq!(actual, expected.map_err(str::to_owned));
        let urls = mock.calls.borrow();
        assert_eq!(urls.len(), calls);
        for (i, url) in urls.iter().enumerate() {
            assert!(url.ends_with(&format!("&index={}&pageSize=50", i * 50)));
        }
    }
}
