use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub type Environment = HashMap<std::ffi::OsString, std::ffi::OsString>;
pub type Error = &'static str;
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSource {
    Explicit,
    JavaHome,
    Platform,
    Path,
    Managed,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Runtime {
    pub executable: String,
    pub source: CandidateSource,
    pub major_version: u16,
    pub specification_version: String,
    pub version: Option<String>,
    pub runtime_version: Option<String>,
    pub vendor: Option<String>,
    pub java_home: Option<String>,
    pub vm_name: Option<String>,
    pub os_arch: String,
    pub os_version: Option<String>,
    pub architecture: Option<String>,
    pub data_model: Option<u8>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    pub executable: String,
    pub source: CandidateSource,
    pub error_name: String,
}
#[derive(Debug, Serialize)]
pub struct Detection {
    pub runtimes: Vec<Runtime>,
    pub failures: Vec<Failure>,
}
pub fn parse_major(version: &str) -> Result<u16, Error> {
    let v = version.trim_matches([' ', '\t', '\r', '\n', '"']);
    let v = v.strip_prefix("1.").unwrap_or(v);
    let digits: String = v.chars().take_while(char::is_ascii_digit).collect();
    let n: u16 = digits.parse().map_err(|_| "InvalidJavaVersion")?;
    if n == 0 || n > 1000 {
        Err("InvalidJavaVersion")
    } else {
        Ok(n)
    }
}
pub fn architecture(value: &str) -> Option<&'static str> {
    match value {
        "x86" => Some("x86"),
        "x86_64" | "amd64" => Some("x86_64"),
        "arm" => Some("arm"),
        "arm64" | "aarch64" => Some("arm64"),
        _ => None,
    }
}
fn property<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output.split('\n').find_map(|line| {
        let (k, v) = line.trim_matches([' ', '\t', '\r']).split_once('=')?;
        (k.trim_matches([' ', '\t', '\r']) == key).then(|| v.trim_matches([' ', '\t', '\r']))
    })
}
pub fn parse_output(
    executable: &str,
    source: CandidateSource,
    stderr: &str,
    stdout: &str,
) -> Result<Runtime, Error> {
    let get = |key| property(stderr, key).or_else(|| property(stdout, key));
    let spec = get("java.specification.version")
        .or_else(|| get("java.version"))
        .ok_or("MissingJavaVersion")?;
    let arch = get("os.arch").ok_or("MissingJavaArchitecture")?;
    let architecture = architecture(arch);
    let model = get("sun.arch.data.model")
        .map(|v| v.parse::<u8>().map_err(|_| "InvalidJavaDataModel"))
        .transpose()?;
    if let Some(n) = model {
        if n != 32 && n != 64 {
            return Err("InvalidJavaDataModel");
        };
        if let Some(a) = architecture
            && n != if a == "x86" || a == "arm" { 32 } else { 64 }
        {
            return Err("JavaArchitectureConflict");
        }
    }
    let own = |k| get(k).map(str::to_owned);
    Ok(Runtime {
        executable: executable.into(),
        source,
        major_version: parse_major(spec)?,
        specification_version: spec.into(),
        version: own("java.version"),
        runtime_version: own("java.runtime.version"),
        vendor: own("java.vendor"),
        java_home: own("java.home"),
        vm_name: own("java.vm.name"),
        os_arch: arch.into(),
        os_version: own("os.version"),
        architecture: architecture.map(str::to_owned),
        data_model: model,
    })
}
pub fn select_compatible(detection: &Detection, major: u16, arch: &str) -> Result<Runtime, Error> {
    detection
        .runtimes
        .iter()
        .find(|r| r.major_version == major && r.architecture.as_deref() == Some(arch))
        .cloned()
        .ok_or("NoCompatibleJava")
}
fn io_name(e: &io::Error) -> Error {
    #[cfg(unix)]
    if e.raw_os_error() == Some(8) {
        return "InvalidExe";
    }
    #[cfg(windows)]
    if e.raw_os_error() == Some(193) {
        return "InvalidExe";
    }
    match e.kind() {
        io::ErrorKind::NotFound => "FileNotFound",
        io::ErrorKind::PermissionDenied => "AccessDenied",
        io::ErrorKind::InvalidInput => "InvalidArgument",
        _ => "Unexpected",
    }
}
#[cfg(unix)]
fn nonblocking<T: std::os::fd::AsFd>(pipe: &T) -> Result<(), Error> {
    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
    let flags = fcntl_getfl(pipe).map_err(|_| "Unexpected")?;
    fcntl_setfl(pipe, flags | OFlags::NONBLOCK).map_err(|_| "Unexpected")
}
#[cfg(windows)]
fn nonblocking<T>(_: &T) -> Result<(), Error> {
    Ok(())
}
#[cfg(unix)]
fn read_pipe<T: Read>(
    pipe: &mut T,
    bytes: &mut Vec<u8>,
    done: &mut bool,
    limit: usize,
) -> Result<(), Error> {
    let mut buf = [0; 8192];
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => {
                *done = true;
                return Ok(());
            }
            Ok(n) => {
                if bytes.len() + n > limit {
                    return Err("StreamTooLong");
                };
                bytes.extend_from_slice(&buf[..n])
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err("Unexpected"),
        }
    }
}
#[cfg(windows)]
fn read_pipe<T: Read + std::os::windows::io::AsRawHandle>(
    pipe: &mut T,
    bytes: &mut Vec<u8>,
    done: &mut bool,
    limit: usize,
) -> Result<(), Error> {
    use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, GetLastError};
    unsafe extern "system" {
        fn PeekNamedPipe(
            h: *mut std::ffi::c_void,
            buffer: *mut std::ffi::c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            left: *mut u32,
        ) -> i32;
    }
    loop {
        let mut available = 0u32;
        let success = unsafe {
            PeekNamedPipe(
                pipe.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if success == 0 {
            if unsafe { GetLastError() } == ERROR_BROKEN_PIPE {
                *done = true;
                return Ok(());
            }
            return Err("Unexpected");
        };
        if available == 0 {
            return Ok(());
        };
        let mut buf = [0; 8192];
        let want = available.min(buf.len() as u32) as usize;
        match pipe.read(&mut buf[..want]) {
            Ok(0) => {
                *done = true;
                return Ok(());
            }
            Ok(n) => {
                if bytes.len() + n > limit {
                    return Err("StreamTooLong");
                };
                bytes.extend_from_slice(&buf[..n])
            }
            Err(_) => return Err("Unexpected"),
        }
    }
}
/// Bounded concurrent pipe collection; never joins a blocking reader thread.
pub fn run_bounded(
    executable: &str,
    args: &[&str],
    env: &Environment,
    limit: usize,
    timeout: Duration,
) -> Result<(Vec<u8>, Vec<u8>), Error> {
    let mut cmd = Command::new(executable);
    cmd.args(args)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    #[cfg(target_os = "macos")]
    super::direct_exec::configure(&mut cmd, executable, args, env).map_err(|e| io_name(&e))?;
    let mut child = cmd.spawn().map_err(|e| io_name(&e))?;
    let mut out = child.stdout.take().ok_or("Unexpected")?;
    let mut err = child.stderr.take().ok_or("Unexpected")?;
    let result = (|| {
        nonblocking(&out)?;
        nonblocking(&err)?;
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let (mut od, mut ed) = (false, false);
        let start = Instant::now();
        loop {
            read_pipe(&mut out, &mut stdout, &mut od, limit)?;
            read_pipe(&mut err, &mut stderr, &mut ed, limit)?;
            if let Some(status) = child.try_wait().map_err(|e| io_name(&e))?
                && od
                && ed
            {
                if status.code().is_none() {
                    return Err("JavaAbnormalTermination");
                };
                if !status.success() {
                    return Err("JavaExitedNonZero");
                };
                return Ok((stdout, stderr));
            }
            if start.elapsed() >= timeout {
                return Err("Timeout");
            };
            std::thread::sleep(Duration::from_millis(2));
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}
pub fn probe(
    executable: &str,
    source: CandidateSource,
    env: &Environment,
) -> Result<Runtime, Error> {
    let mut clean = env.clone();
    for key in ["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS"] {
        clean.remove(std::ffi::OsStr::new(key));
    }
    let (out, err) = run_bounded(
        executable,
        &["-XshowSettings:properties", "-version"],
        &clean,
        256 * 1024,
        Duration::from_secs(10),
    )?;
    parse_output(
        executable,
        source,
        std::str::from_utf8(&err).map_err(|_| "InvalidUtf8")?,
        std::str::from_utf8(&out).map_err(|_| "InvalidUtf8")?,
    )
}
fn executable(path: &Path) -> bool {
    let Ok(m) = path.metadata() else { return false };
    if !m.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        rustix::fs::access(path, rustix::fs::Access::EXEC_OK).is_ok()
    }
    #[cfg(windows)]
    {
        true
    }
}
pub fn discover(
    env: &Environment,
    explicit: &[String],
) -> Result<Vec<(String, CandidateSource)>, Error> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |p: PathBuf, s: CandidateSource, required: bool| {
        let Some(v) = p.to_str() else { return };
        if !v.is_empty() && !seen.contains(v) && (!required || executable(&p)) {
            seen.insert(v.to_owned());
            out.push((v.to_owned(), s));
        }
    };
    for p in explicit {
        add(p.into(), CandidateSource::Explicit, false)
    }
    let name = if cfg!(windows) { "java.exe" } else { "java" };
    if let Some(home) = env.get(std::ffi::OsStr::new("JAVA_HOME")) {
        add(
            PathBuf::from(home).join("bin").join(name),
            CandidateSource::JavaHome,
            true,
        )
    }
    #[cfg(target_os = "linux")]
    for base in ["/usr/lib/jvm", "/usr/java"] {
        if let Ok(items) = std::fs::read_dir(base) {
            for item in items.flatten() {
                add(
                    item.path().join("bin/java"),
                    CandidateSource::Platform,
                    true,
                )
            }
        }
    }
    #[cfg(target_os = "macos")]
    if let Ok((_, err)) = run_bounded(
        "/usr/libexec/java_home",
        &["-V"],
        env,
        64 * 1024,
        Duration::from_secs(10),
    ) {
        for line in String::from_utf8_lossy(&err).lines() {
            let line = line.trim_matches([' ', '\t', '\r']);
            let home = line.rsplit_once(' ').map(|(_, h)| h).unwrap_or(line);
            if Path::new(home).is_absolute() {
                add(
                    Path::new(home).join("bin/java"),
                    CandidateSource::Platform,
                    true,
                )
            }
        }
    }
    if let Some(paths) = env.get(std::ffi::OsStr::new("PATH")) {
        for p in std::env::split_paths(paths) {
            let p = if p.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                p
            };
            add(p.join(name), CandidateSource::Path, true)
        }
    }
    if out.is_empty() {
        Err("JavaNotFound")
    } else {
        Ok(out)
    }
}
pub fn detect(env: &Environment, explicit: &[String]) -> Result<Detection, Error> {
    let candidates = discover(env, explicit)?;
    let mut out = Detection {
        runtimes: Vec::new(),
        failures: Vec::new(),
    };
    let mut homes = HashSet::new();
    for (executable, source) in candidates {
        match probe(&executable, source, env) {
            Ok(r) => {
                if r.java_home
                    .as_ref()
                    .is_some_and(|h| !homes.insert(h.clone()))
                {
                    continue;
                }
                out.runtimes.push(r)
            }
            Err(e) => out.failures.push(Failure {
                executable,
                source,
                error_name: e.into(),
            }),
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_and_arch() {
        for (v, n) in [("1.8.0_402", 8), ("  \"21-ea\" ", 21), ("17.0.12+7", 17)] {
            assert_eq!(parse_major(v), Ok(n))
        }
        for v in ["", "0", "1001", "not-java"] {
            assert!(parse_major(v).is_err())
        }
        assert_eq!(architecture("aarch64"), Some("arm64"));
    }
    #[test]
    fn properties_selection() {
        let r=parse_output("java",CandidateSource::Explicit," java.specification.version = 21\n os.arch = amd64\n sun.arch.data.model = 64\n java.vendor = First\n","java.vendor = Second\n").unwrap();
        assert_eq!(r.vendor.as_deref(), Some("First"));
        assert!(
            parse_output(
                "java",
                CandidateSource::Explicit,
                "java.version = 17\nos.arch = x86\nsun.arch.data.model = 64",
                ""
            )
            .is_err()
        );
        let d = Detection {
            runtimes: vec![r.clone()],
            failures: vec![],
        };
        assert_eq!(select_compatible(&d, 21, "x86_64").unwrap(), r);
        assert!(select_compatible(&d, 17, "x86_64").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn bounded_probe_and_clean_options() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("java");
        std::fs::write(&path,"#!/bin/sh\nif [ -n \"$JAVA_TOOL_OPTIONS$_JAVA_OPTIONS$JDK_JAVA_OPTIONS\" ]; then exit 2; fi\nprintf 'java.specification.version = 17\\nos.arch = amd64\\nsun.arch.data.model = 64\\n' >&2\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut env = Environment::new();
        env.insert("JAVA_TOOL_OPTIONS".into(), "bad".into());
        assert_eq!(
            probe(path.to_str().unwrap(), CandidateSource::Explicit, &env)
                .unwrap()
                .major_version,
            17
        );
    }
    #[cfg(unix)]
    #[test]
    fn bounded_timeout_and_output() {
        let env = Environment::new();
        assert_eq!(
            run_bounded(
                "/bin/sh",
                &["-c", "while :; do :; done"],
                &env,
                100,
                Duration::from_millis(30)
            ),
            Err("Timeout")
        );
        assert_eq!(
            run_bounded(
                "/bin/sh",
                &["-c", "printf 123456789"],
                &env,
                4,
                Duration::from_secs(1)
            ),
            Err("StreamTooLong")
        );
    }
}

#[cfg(all(test, unix))]
mod pipe_tests {
    use super::*;
    #[test]
    fn descendant_held_pipe_is_bounded() {
        let start = Instant::now();
        assert_eq!(
            run_bounded(
                "/bin/sh",
                &["-c", "/bin/sleep 1 & exit 0"],
                &Environment::new(),
                100,
                Duration::from_millis(40)
            ),
            Err("Timeout")
        );
        assert!(start.elapsed() < Duration::from_millis(500));
    }
}
