use std::io::{self, Write};
fn main() {
    match std::env::var("JMCL_TEST_JAVA_MODE").as_deref() {
        Ok("hang") => std::thread::sleep(std::time::Duration::from_secs(2)),
        Ok("hold") => {
            let _ = std::process::Command::new(std::env::current_exe().unwrap())
                .env("JMCL_TEST_JAVA_MODE", "hang")
                .spawn()
                .unwrap();
        }
        Ok("large") => {
            let _ = io::stdout().write_all(&vec![b'x'; 4096]);
        }
        _ => {
            for key in ["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS"] {
                if std::env::var_os(key).is_some() {
                    std::process::exit(3)
                }
            }
            eprintln!("java.specification.version = 17\nos.arch = amd64\nsun.arch.data.model = 64\njava.vendor = Fixture\njava.home = /fixture/java");
        }
    }
}
