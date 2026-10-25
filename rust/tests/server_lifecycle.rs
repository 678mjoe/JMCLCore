#![cfg(target_os = "linux")]
use jmcl_core_rust::servers::lifecycle;
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::Write,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn rpc(method: &str, params: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_jmcl-core-rust"))
        .arg("rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        json!({"protocol":1,"id":"test","method":method,"params":params})
    )
    .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
struct Fixture {
    tmp: tempfile::TempDir,
    dir: File,
}
impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let dirpath = tmp.path().join("test");
        fs::create_dir_all(dirpath.join(".jmcl")).unwrap();
        let java = tmp.path().join("java-fixture");
        fs::write(&java,b"#!/bin/sh\nif [ \"$1\" = \"-XshowSettings:properties\" ]; then\n echo '    java.specification.version = 21' >&2\n echo '    java.version = 21.0.1' >&2\n echo '    os.arch = amd64' >&2\n exit 0\nfi\necho synthetic-start\necho synthetic-stderr >&2\nwhile IFS= read -r command; do\n echo \"received:$command\"\n [ \"$command\" = stop ] && exit 0\ndone\n").unwrap();
        fs::set_permissions(&java, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            dirpath.join("server.json"),
            serde_json::to_vec(
                &json!({"id":"test","name":"Test","version_id":"synthetic","java_path":java}),
            )
            .unwrap(),
        )
        .unwrap();
        fs::write(dirpath.join("server.jar"), b"benign").unwrap();
        fs::write(dirpath.join(".jmcl/install.json"),serde_json::to_vec(&json!({"schema_version":3,"version_id":"synthetic","server_sha1":"0".repeat(40),"server_size":6,"java_major_version":21,"launch_mode":"jar","launch_file":"server.jar","launch_sha1":"0".repeat(40),"launch_size":6})).unwrap()).unwrap();
        fs::write(
            dirpath.join("server.properties"),
            b"level-name=world\nrcon.password=secret\n",
        )
        .unwrap();
        let dir = File::open(dirpath).unwrap();
        Self { tmp, dir }
    }
    fn params(&self) -> Value {
        json!({"directory":self.tmp.path(),"id":"test"})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = lifecycle::stop(&self.dir, 100);
    }
}
#[test]
fn detached_start_command_status_restart_stop() {
    let f = Fixture::new();
    let start = rpc("server.start", f.params());
    assert_eq!(start["result"]["running"], true, "{start}");
    let pid = start["result"]["pid"].as_u64().unwrap();
    // The RPC process is already gone: this status observes a detached child.
    let status = rpc("server.status", f.params());
    assert_eq!(status["result"]["pid"], pid);
    assert_eq!(status["result"]["running"], true);
    let again = rpc("server.start", f.params());
    assert_eq!(again["error"]["code"], "SERVER_ALREADY_RUNNING");
    let mut command = f.params();
    command["command"] = "say isolated-test".into();
    assert_eq!(rpc("server.command", command)["result"]["accepted"], true);
    let mut set = f.params();
    set["properties"] = json!({"motd":"blocked"});
    assert_eq!(
        rpc("server.properties.set", set)["error"]["code"],
        "SERVER_ALREADY_RUNNING"
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let logs = rpc("server.logs", f.params());
        if logs["result"]["text"]
            .as_str()
            .unwrap_or("")
            .contains("received:say isolated-test")
        {
            assert!(
                logs["result"]["text"]
                    .as_str()
                    .unwrap()
                    .contains("synthetic-stderr")
            );
            break;
        }
        assert!(Instant::now() < deadline, "{logs}");
        std::thread::sleep(Duration::from_millis(20));
    }
    let restart = rpc("server.restart", f.params());
    assert_eq!(restart["result"]["running"], true, "{restart}");
    assert_ne!(restart["result"]["pid"], pid);
    let stop = rpc("server.stop", f.params());
    assert_eq!(stop["result"]["termination"], "graceful", "{stop}");
    assert_eq!(rpc("server.status", f.params())["result"]["running"], false);
    assert!(!f.tmp.path().join("test/.jmcl/server.pid").exists());
}
#[test]
fn offline_properties_and_unknown_parameters() {
    let f = Fixture::new();
    let get = rpc("server.properties.get", f.params());
    assert_eq!(get["result"]["properties"]["rcon.password"], "<redacted>");
    let mut set = f.params();
    set["properties"] = json!({"motd":"hello","rcon.password":"new-secret"});
    let result = rpc("server.properties.set", set);
    assert_eq!(result["result"]["properties"]["motd"], "hello");
    assert!(!result.to_string().contains("new-secret"));
    let mut status = f.params();
    status["grace_ms"] = json!({"ignored":true});
    assert_eq!(rpc("server.status", status)["result"]["running"], false);
    let stop = rpc("server.stop", f.params());
    assert_eq!(stop["error"]["code"], "SERVER_NOT_RUNNING");
}

#[test]
fn stop_escalates_only_verified_synthetic_children() {
    for (ignore_term, expected) in [(false, "terminated"), (true, "killed")] {
        let f = Fixture::new();
        let java = f.tmp.path().join("java-fixture");
        let script = fs::read_to_string(&java)
            .unwrap()
            .replace("[ \"$command\" = stop ] && exit 0", ":");
        let script = if ignore_term {
            script.replace("echo synthetic-start", "trap '' TERM\necho synthetic-start")
        } else {
            script
        };
        fs::write(java, script).unwrap();
        let start = rpc("server.start", f.params());
        assert_eq!(start["result"]["running"], true, "{start}");
        // The production acknowledgement means "spawned", not "application
        // initialized". Wait for this fixture's marker after installing its
        // TERM trap before asserting which escalation stage must be required.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let logs = rpc("server.logs", f.params());
            if logs["result"]["text"]
                .as_str()
                .unwrap_or("")
                .contains("synthetic-start")
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "synthetic fixture did not become ready: {logs}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut stop = f.params();
        stop["grace_ms"] = 0.into();
        let result = rpc("server.stop", stop);
        assert_eq!(result["result"]["termination"], expected, "{result}");
        assert!(!lifecycle::status(&f.dir).unwrap().running);
    }
}

#[test]
fn server_worlds_keep_base_dimensions_and_properties_coupled() {
    let f = Fixture::new();
    let root = f.tmp.path().join("test");
    for name in ["world", "world_nether", "world_the_end", "unrelated"] {
        fs::create_dir(root.join(name)).unwrap();
        fs::write(root.join(name).join("data"), name).unwrap();
    }
    let list = rpc("server.worlds.list", f.params());
    assert_eq!(
        list["result"]["worlds"].as_array().unwrap().len(),
        3,
        "{list}"
    );
    let mut rename = f.params();
    rename["world"] = "world".into();
    rename["new_name"] = "renamed".into();
    let result = rpc("server.worlds.rename", rename);
    assert_eq!(
        result["result"]["renamed"]["new_name"], "renamed",
        "{result}"
    );
    for suffix in ["", "_nether", "_the_end"] {
        assert!(root.join(format!("renamed{suffix}")).is_dir());
        assert!(!root.join(format!("world{suffix}")).exists());
    }
    assert!(
        fs::read_to_string(root.join("server.properties"))
            .unwrap()
            .contains("level-name=renamed")
    );
    assert!(root.join("unrelated").is_dir());
    let mut delete = f.params();
    delete["world"] = "renamed".into();
    assert_eq!(
        rpc("server.worlds.delete", delete)["result"]["deleted"],
        "renamed"
    );
    for suffix in ["", "_nether", "_the_end"] {
        assert!(!root.join(format!("renamed{suffix}")).exists());
    }
    assert!(root.join("unrelated").is_dir());
}

#[test]
fn coupled_backup_restore_and_stopped_guard() {
    let f = Fixture::new();
    let root = f.tmp.path().join("test");
    for name in ["world", "world_nether", "world_the_end"] {
        fs::create_dir(root.join(name)).unwrap();
        fs::write(root.join(name).join("data"), name).unwrap();
    }
    let backup = rpc("server.worlds.backup", f.params());
    let filename = backup["result"]["backup"]
        .as_str()
        .unwrap_or_else(|| panic!("{backup}"));
    let mut restore = f.params();
    restore["backup"] = filename.into();
    assert_eq!(
        rpc("server.worlds.restore", restore.clone())["error"]["code"],
        "WORLD_EXISTS"
    );
    for name in ["world", "world_nether", "world_the_end"] {
        fs::remove_dir_all(root.join(name)).unwrap();
    }
    assert_eq!(
        rpc("server.worlds.restore", restore)["result"]["restored"],
        filename
    );
    for name in ["world", "world_nether", "world_the_end"] {
        assert_eq!(
            fs::read(root.join(name).join("data")).unwrap(),
            name.as_bytes()
        );
    }
    assert_eq!(rpc("server.start", f.params())["result"]["running"], true);
    assert_eq!(
        rpc("server.worlds.backup", f.params())["error"]["code"],
        "SERVER_ALREADY_RUNNING"
    );
    let mut rename = f.params();
    rename["world"] = "world".into();
    rename["new_name"] = "blocked".into();
    assert_eq!(
        rpc("server.worlds.rename", rename)["error"]["code"],
        "SERVER_ALREADY_RUNNING"
    );
    let mut delete = f.params();
    delete["backup"] = filename.into();
    assert_eq!(
        rpc("server.worlds.backups.delete", delete)["result"]["deleted"],
        filename
    );
}

#[test]
fn lifecycle_numeric_parameter_compatibility() {
    let f = Fixture::new();
    let mut logs = f.params();
    logs["max_bytes"] = serde_json::json!(4294967296u64);
    let result = rpc("server.logs", logs);
    assert_eq!(
        result["error"]["message"],
        "server max_bytes is out of range"
    );
    let mut logs = f.params();
    logs["cursor"] = "1e1".into();
    logs["max_bytes"] = "64.0".into();
    assert_eq!(
        rpc("server.logs", logs)["error"]["code"],
        "SERVER_LOGS_NOT_FOUND"
    );
}

#[test]
fn supervisor_handshake_failure_kills_child_and_cleans_state() {
    let f = Fixture::new();
    rustix::fs::mknodat(
        &f.dir,
        ".jmcl/console.fifo",
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::from_raw_mode(0o600),
        0,
    )
    .unwrap();
    let full = fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_jmcl-core-rust"))
        .arg("__server_supervisor")
        .arg(f.tmp.path().join("test"))
        .arg(f.tmp.path().join("java-fixture"))
        .arg("0123456789abcdef0123456789abcdef")
        .stdin(Stdio::null())
        .stdout(full)
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success());
    assert!(!f.tmp.path().join("test/.jmcl/server.pid").exists());
}

#[test]
fn managed_rcon_and_query_use_property_derived_loopback_ports() {
    use std::io::Read;
    use std::net::{Ipv4Addr, TcpListener, UdpSocket};
    fn frame(stream: &mut std::net::TcpStream) -> (i32, i32, Vec<u8>) {
        let mut p = [0; 4];
        stream.read_exact(&mut p).unwrap();
        let n = i32::from_le_bytes(p);
        assert!((10..=65536).contains(&n));
        let mut b = vec![0; n as usize];
        stream.read_exact(&mut b).unwrap();
        (
            i32::from_le_bytes(b[..4].try_into().unwrap()),
            i32::from_le_bytes(b[4..8].try_into().unwrap()),
            b[8..b.len() - 2].to_vec(),
        )
    }
    fn send(stream: &mut std::net::TcpStream, id: i32, kind: i32, body: &[u8]) {
        let mut bytes = vec![];
        bytes.extend_from_slice(&((body.len() + 10) as i32).to_le_bytes());
        bytes.extend_from_slice(&id.to_le_bytes());
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(body);
        bytes.extend_from_slice(&[0, 0]);
        stream.write_all(&bytes).unwrap();
    }
    let f = Fixture::new();
    let tcp = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let udp = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    udp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    fs::write(f.tmp.path().join("test/server.properties"),format!("enable-rcon=true\nrcon.port={}\nenable-query=true\nquery.port={}\nserver-ip=not-a-host-to-dial.invalid\n",tcp.local_addr().unwrap().port(),udp.local_addr().unwrap().port())).unwrap();
    let t = std::thread::spawn(move || {
        let (mut stream, _) = tcp.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let (auth, kind, password) = frame(&mut stream);
        assert_eq!(kind, 3);
        assert_eq!(password, b"synthetic-secret");
        send(&mut stream, auth, 2, b"");
        let (cmd, _, body) = frame(&mut stream);
        assert_eq!(body, b"list");
        send(&mut stream, cmd, 0, b"part-one");
        let (probe, _, body) = frame(&mut stream);
        assert!(body.is_empty());
        send(&mut stream, cmd, 0, b"+part-two");
        send(&mut stream, probe, 0, b"");
    });
    let u = std::thread::spawn(move || {
        let mut b = [0; 64];
        let (n, peer) = udp.recv_from(&mut b).unwrap();
        assert_eq!(n, 7);
        let session = b[3..7].to_vec();
        let mut response = vec![9];
        response.extend_from_slice(&session);
        response.extend_from_slice(b"42\0");
        udp.send_to(&response, peer).unwrap();
        let (n, _) = udp.recv_from(&mut b).unwrap();
        assert_eq!(n, 11);
        let mut response = vec![0];
        response.extend_from_slice(&session);
        response.extend_from_slice(b"fixture\0SMP\0world\x000\x0020\0\xdd\x63127.0.0.1\0");
        udp.send_to(&response, peer).unwrap();
    });
    assert_eq!(rpc("server.start", f.params())["result"]["running"], true);
    let mut p = f.params();
    p["password"] = "synthetic-secret".into();
    p["command"] = "list".into();
    let r = rpc("server.rcon.command", p);
    assert_eq!(r["result"]["text"], "part-one+part-two", "{r}");
    assert!(!r.to_string().contains("synthetic-secret"));
    let mut p = f.params();
    p["mode"] = "basic".into();
    let r = rpc("server.query", p);
    assert_eq!(r["result"]["hostname"], "fixture", "{r}");
    assert_eq!(r["result"]["host_port"], 25565);
    t.join().unwrap();
    u.join().unwrap();
}
