//! Finite installer oracle corpus. All files are temporary and HTTP is loopback.
use jmcl_core_rust::{
    install::{self, Event, Transport},
    metadata::Source,
    plan::{File, Plan, Target, Usage},
    store::{ArtifactRoot, Store},
};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    sync::Mutex,
    time::{Duration, Instant},
};
struct Loopback(reqwest::blocking::Client);
impl Transport for Loopback {
    fn fetch(&self, url: &str, sink: &mut dyn Write) -> Result<u16, install::Error> {
        assert!(url.starts_with("http://127.0.0.1:"));
        let mut response = self
            .0
            .get(url)
            .send()
            .map_err(|_| install::Error::named("FixtureTransport"))?;
        let status = response.status().as_u16();
        std::io::copy(&mut response, sink).map_err(|_| install::Error::named("WriteFailed"))?;
        Ok(status)
    }
}
fn fixture_server(
    responses: Vec<(u16, Vec<u8>)>,
) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        let mut paths = vec![];
        for (status, body) in responses {
            let start = Instant::now();
            let (mut s, _) = loop {
                match listener.accept() {
                    Ok(s) => break s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            start.elapsed() < Duration::from_secs(10),
                            "fixture request timed out"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut request = Vec::new();
            let mut b = [0; 1];
            while !request.ends_with(b"\r\n\r\n") {
                assert!(request.len() < 8192);
                s.read_exact(&mut b).unwrap();
                request.push(b[0]);
            }
            paths.push(
                String::from_utf8(request)
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .into(),
            );
            write!(
                s,
                "HTTP/1.1 {status} {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                if status == 200 { "OK" } else { "Error" },
                body.len()
            )
            .unwrap();
            s.write_all(&body).unwrap();
        }
        paths
    });
    (url, task)
}
fn run(
    rust: bool,
    responses: Vec<(u16, Vec<u8>)>,
    seed: bool,
    retries: u8,
) -> (Vec<Value>, Vec<String>) {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("game");
    let store = t.path().join("store");
    fs::create_dir(&root).unwrap();
    if seed {
        fs::write(root.join("test.jar"), b"abc").unwrap();
    }
    let (url, task) = fixture_server(responses);
    let files = vec![File {
        path: "test.jar".into(),
        url: format!("{url}/primary"),
        fallback_url: Some(format!("{url}/fallback")),
        sha1: format!("{:x}", Sha1::digest(b"abc")),
        size: 3,
        usage: Usage {
            library: true,
            ..Default::default()
        },
    }];
    let output = if rust {
        let p = Plan {
            files,
            total_download_size: 3,
            version_id: "fixture".into(),
            source: Source::Official,
            target: Target::host().unwrap(),
            classpath: vec![],
            native_extractions: vec![],
            asset_projections: vec![],
            java_major_version: 0,
            loader: "none".into(),
        };
        let captured = Mutex::new(Vec::<Value>::new());
        let reporter = |event| {
            captured.lock().unwrap().push(match event {
                Event::File(v) => json!({"event":"file","file":v}),
                Event::Retry(v) => json!({"event":"retry","retry":v}),
                Event::Progress(v) => json!({"event":"progress","progress":v}),
            });
            Ok(())
        };
        let result = install::execute(
            &ArtifactRoot::open(&root).unwrap(),
            &Store::open(&store).unwrap(),
            &p,
            install::Options {
                workers: 1,
                retries,
                sync_files: true,
            },
            &Loopback(
                reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build()
                    .unwrap(),
            ),
            Some(&reporter),
        );
        let mut events = captured.into_inner().unwrap();
        events.push(match result {
            Ok(v) => json!({"result":v}),
            Err(e) => json!({"error":e.name()}),
        });
        events
    } else {
        let fixture = t.path().join("fixture.json");
        fs::write(
            &fixture,
            json!({"files":files,"total_download_size":3,"workers":1,"retries":retries})
                .to_string(),
        )
        .unwrap();
        let out = Command::new(
            std::env::var("JMCL_ZIG_INSTALL_ORACLE")
                .expect("compile pinned compat/install_oracle.zig and set path"),
        )
        .arg(fixture)
        .arg(&root)
        .arg(&store)
        .output()
        .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    };
    let paths = task.join().unwrap();
    (output, paths)
}
#[test]
#[ignore = "requires pinned Zig installer fixture adapter"]
fn cached_download_retries_and_failures_match_zig() {
    for (responses, seed, retries) in [
        (vec![], true, 0),
        (vec![(200, b"abc".to_vec())], false, 0),
        (
            vec![(200, b"bad".to_vec()), (200, b"abc".to_vec())],
            false,
            1,
        ),
        (vec![(200, b"ab".to_vec())], false, 0),
        (vec![(200, b"abcd".to_vec())], false, 0),
        (vec![(200, b"bad".to_vec())], false, 0),
        (vec![(503, b"abc".to_vec())], false, 0),
    ] {
        let a = run(false, responses.clone(), seed, retries);
        let b = run(true, responses.clone(), seed, retries);
        assert_eq!(
            a, b,
            "case seed={seed},retries={retries},responses={responses:?}"
        );
    }
}
