//! Read-only installer ZIP lookup matching the pinned oracle's accepted format.
//! No filesystem paths are materialized. Native extraction has separate policy.
use crate::metadata::Error;
use std::io::Read;
fn error(s: &'static str) -> Error {
    Error::Plan(s)
}
fn bytes(b: &[u8], start: usize, len: usize) -> Result<&[u8], Error> {
    b.get(start..start.checked_add(len).ok_or(error("ZipTruncated"))?)
        .ok_or(error("ZipTruncated"))
}
fn u16at(b: &[u8], at: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(bytes(b, at, 2)?.try_into().unwrap()))
}
fn u32at(b: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(bytes(b, at, 4)?.try_into().unwrap()))
}
pub fn read_entry(b: &[u8], name: &str, max: usize) -> Result<Option<Vec<u8>>, Error> {
    let end = b
        .windows(4)
        .rposition(|v| v == b"PK\x05\x06")
        .ok_or(error("ZipNoEndRecord"))?;
    bytes(b, end, 22)?;
    let count = u16at(b, end + 10)?;
    let size = u32at(b, end + 12)?;
    let offset = u32at(b, end + 16)?;
    if count == u16::MAX || size == u32::MAX || offset == u32::MAX || u16at(b, end + 8)? == u16::MAX
    {
        return Err(error("Zip64Unsupported"));
    }
    bytes(b, offset as usize, size as usize)?;
    let mut at = offset as usize;
    for _ in 0..count {
        let header = bytes(b, at, 46)?;
        if &header[..4] != b"PK\x01\x02" {
            return Err(error("ZipTruncated"));
        }
        if u16at(header, 8)? & 1 != 0 {
            return Err(error("ZipEntryEncrypted"));
        }
        let n = u16at(header, 28)? as usize;
        if n == 0 || n > 4096 {
            return Err(error("ZipUnsafeEntryName"));
        }
        let extra = u16at(header, 30)? as usize;
        let comment = u16at(header, 32)? as usize;
        let entry_name = bytes(b, at + 46, n)?;
        bytes(b, at + 46, n + extra + comment)?;
        if entry_name.contains(&b'\\') {
            return Err(error("ZipUnsafeEntryName"));
        }
        at += 46 + n + extra + comment;
        if entry_name != name.as_bytes() {
            continue;
        }
        let method = u16at(header, 10)?;
        let compressed = u32at(header, 20)? as usize;
        let size = u32at(header, 24)? as usize;
        let local = u32at(header, 42)? as usize;
        if size > max {
            return Err(error("ZipEntryTooLarge"));
        }
        let local_header = bytes(b, local, 30)?;
        if &local_header[..4] != b"PK\x03\x04" {
            return Err(error("ZipTruncated"));
        }
        if u16at(local_header, 6)? & 1 != 0 {
            return Err(error("ZipEntryEncrypted"));
        }
        let start =
            local + 30 + u16at(local_header, 26)? as usize + u16at(local_header, 28)? as usize;
        let compressed = bytes(b, start, compressed)?;
        let out = match method {
            0 => {
                if compressed.len() != size {
                    return Err(error("ZipTruncated"));
                }
                compressed.to_vec()
            }
            8 => {
                let mut out = Vec::new();
                flate2::read::DeflateDecoder::new(compressed)
                    .take(size as u64 + 1)
                    .read_to_end(&mut out)
                    .map_err(|_| error("ZipTruncated"))?;
                if out.len() != size {
                    return Err(error("ZipTruncated"));
                }
                out
            }
            _ => return Err(error("ZipUnsupportedCompression")),
        };
        return Ok(Some(out));
    }
    Ok(None)
}
