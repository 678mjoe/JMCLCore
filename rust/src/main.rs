use jmcl_core_rust::{VERSION, protocol};
use std::io::{self, Write};
fn run(args: &[String], stdout: &mut impl Write, stderr: &mut impl Write) -> io::Result<u8> {
    if let Some(result) = jmcl_core_rust::launch::cli::run(args, stdout, stderr) {
        return result;
    }
    if let Some(result) = jmcl_core_rust::install::cli::cli(args, stdout, stderr) {
        return result;
    }
    if let Some(result) = jmcl_core_rust::version::cli(args, stdout, stderr) {
        return result;
    }
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
        Some("store") => jmcl_core_rust::store::cli(&args[1..], stdout, stderr),
        Some("server") => jmcl_core_rust::servers::cli(&args[1..], stdout, stderr),
        Some("java") => jmcl_core_rust::java::cli(&args[1..], stdout, stderr),
        Some("instance") => {
            if args.get(1).is_some_and(|a| a == "modpack") {
                return jmcl_core_rust::modpack::cli::run(&args[2..], stdout, stderr);
            }
            if args.get(1).is_some_and(|a| a == "worlds")
                && let Some(result) = jmcl_core_rust::worlds::cli(&args[2..], stdout, stderr)
            {
                return result;
            }
            if let Some(result) = jmcl_core_rust::content::cli(&args[1..], stdout, stderr) {
                return result;
            }
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
    let os_args: Vec<_> = std::env::args_os().skip(1).collect();
    let raw_result =
        jmcl_core_rust::content::local_cli_os(&os_args, &mut io::stdout(), &mut io::stderr())
            .or_else(|| {
                jmcl_core_rust::worlds::read_cli_os(&os_args, &mut io::stdout(), &mut io::stderr())
            });
    let raw_result = raw_result.or_else(|| {
        jmcl_core_rust::version::read_cli_os(&os_args, &mut io::stdout(), &mut io::stderr())
    });
    let raw_result = raw_result.or_else(|| {
        jmcl_core_rust::version::plan_cli_os(&os_args, &mut io::stdout(), &mut io::stderr())
    });
    let args = if raw_result.is_none() {
        Some(std::env::args().skip(1).collect::<Vec<_>>())
    } else {
        None
    };
    #[cfg(target_os = "linux")]
    if let Some(args) = args.as_ref()
        && let Some(code) = jmcl_core_rust::servers::lifecycle::dispatch_hidden(args)
    {
        std::process::exit(code);
    }
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let result = match raw_result {
        Some(result) => result,
        None => run(args.as_ref().unwrap(), &mut stdout, &mut stderr),
    };
    let code = match result {
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
