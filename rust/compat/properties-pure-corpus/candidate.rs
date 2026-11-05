#[allow(dead_code)] mod properties;
fn main() {
 for arg in std::env::args().skip(1) {
  let bytes: Vec<u8> = arg.as_bytes().chunks(2).map(|s|u8::from_str_radix(std::str::from_utf8(s).unwrap(),16).unwrap()).collect();
  match properties::parse(&bytes) {
   Err(e)=>println!("error:{}",e.0),
   Ok(d)=>{print!("ok:");for e in d.entries {for b in e.key.as_bytes(){print!("{b:02x}");}print!(":");for b in e.value.as_bytes(){print!("{b:02x}");}print!(";");}println!();}
  }
 }
}
