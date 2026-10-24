//! Harmless standalone native test executable; neither downloads nor runs Java.
use std::{fs, io::Write};
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args
        .get(1)
        .is_some_and(|s| s == "-XshowSettings:properties")
    {
        let arch = if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else if cfg!(target_arch = "x86") {
            "x86"
        } else {
            "amd64"
        };
        eprintln!(
            "java.specification.version = 17\nos.arch = {arch}\nsun.arch.data.model = 64\njava.vendor = Fixture\njava.home = /fixture/loader-java"
        );
        return;
    }
    assert_eq!(args[1], "-cp");
    assert_eq!(args[3], "fixture.Main");
    if args[5] == "stdout-large" || args[5] == "stderr-large" {
        let bytes = vec![b'x'; 17 * 1024 * 1024];
        if args[5] == "stdout-large" {
            let _ = std::io::stdout().write_all(&bytes);
        } else {
            let _ = std::io::stderr().write_all(&bytes);
        }
        return;
    }
    if args[5] == "fail" {
        std::process::exit(7)
    }
    if args[5] == "noop" {
        return;
    }
    assert_eq!(args[6], "literal value");
    assert_eq!(fs::read(&args[7]).unwrap(), b"embedded fixture");
    assert!(std::path::Path::new(&args[8]).file_name().unwrap() == "dep-1.jar");
    let cwd = std::env::current_dir().unwrap();
    let output = std::path::Path::new(&args[4]);
    assert!(output.starts_with(&cwd));
    let mut order = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(cwd.join("processor-order.txt"))
        .unwrap();
    writeln!(order, "{}", args[5].to_str().unwrap()).unwrap();
    fs::write(output, args[5].to_str().unwrap().as_bytes()).unwrap();
}
