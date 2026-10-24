use jmcl_core_rust::{VERSION, protocol};
use std::io::{self, Write};
fn run(args: &[String], stdout: &mut impl Write, stderr: &mut impl Write) -> io::Result<u8> {
    match args.first().map(String::as_str) {
        None | Some("help" | "--help") => {
            stdout.write_all(include_bytes!("help.txt"))?;
            Ok(0)
        }
        Some("version") if args.len() == 1 => {
            writeln!(stdout, "{VERSION}")?;
            Ok(0)
        }
        Some("version") if args.len() == 2 && args[1] == "--json" => {
            protocol::version_document(stdout)?;
            Ok(0)
        }
        Some("version") => {
            writeln!(stderr, "usage: jmcl-core version [--json]")?;
            Ok(2)
        }
        Some("rpc") if args.len() == 1 => {
            protocol::run(&mut io::stdin().lock(), stdout)?;
            Ok(0)
        }
        Some("rpc") => {
            writeln!(stderr, "usage: jmcl-core rpc")?;
            Ok(2)
        }
        Some("instance") => {
            if let Some(result) = jmcl_core_rust::instances::cli(&args[1..], stdout, stderr) {
                result
            } else {
                writeln!(
                    stderr,
                    "Rust rewrite instance command is not implemented; do not use as a replacement Core"
                )?;
                Ok(2)
            }
        }
        Some(command) => {
            writeln!(
                stderr,
                "unknown command: {command}\nrun `jmcl-core --help` for usage"
            )?;
            Ok(2)
        }
    }
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let code = match run(&args, &mut stdout, &mut stderr) {
        Ok(c) => c,
        Err(e) => {
            let _ = if e.kind() == io::ErrorKind::UnexpectedEof {
                writeln!(stderr, "error: EndOfStream")
            } else {
                writeln!(stderr, "Core I/O failure")
            };
            1
        }
    };
    let _ = stdout.flush();
    let _ = stderr.flush();
    std::process::exit(code.into());
}
