//! Benign native executable for all host CI targets. No accounts or network.
use std::io::{self, Write};
fn main() {
 let args:Vec<_>=std::env::args().collect();
 if args.iter().any(|a|a=="-XshowSettings:properties") {
  eprintln!("java.specification.version = 17\nos.arch = amd64\nsun.arch.data.model = 64\njava.vendor = Fixture");return;
 }
 match args.get(1).map(String::as_str) {
  Some("emit")=>{io::stdout().write_all(b"out fake-").unwrap();io::stdout().flush().unwrap();std::thread::sleep(std::time::Duration::from_millis(10));io::stdout().write_all(b"secret\r\n\n").unwrap();io::stderr().write_all(b"binary\0\xff\r").unwrap();std::process::exit(7)},
  Some("args")=>for a in &args[2..]{println!("{a}")},
  Some("env")=>{println!("HOME={}",std::env::var("HOME").unwrap());println!("TOKEN={}",std::env::var_os("FAKE_TOKEN").is_some());println!("JAVA={}",std::env::var_os("JAVA_TOOL_OPTIONS").is_some());},
  Some("hold")=>std::thread::sleep(std::time::Duration::from_millis(600)),
  Some("descendant")=>{std::process::Command::new(&args[0]).arg("hold").spawn().unwrap();println!("parent done");},
  _=>std::process::exit(2)
 }
}
