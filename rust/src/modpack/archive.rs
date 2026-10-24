//! Pinned Zig read-only ZIP semantics, including first-match lookup and no CRC check.
use super::Error;
use std::io::Read;
type Result<T> = std::result::Result<T, Error>;
fn bytes(b: &[u8], at: usize, n: usize) -> Result<&[u8]> {
    b.get(at..at.checked_add(n).ok_or(Error::from("ZipTruncated"))?)
        .ok_or("ZipTruncated".into())
}
fn u16at(b: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(bytes(b, at, 2)?.try_into().unwrap()))
}
fn u32at(b: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes(b, at, 4)?.try_into().unwrap()))
}
#[derive(Debug, Clone)]
pub struct Entry<'a> {
    pub name: &'a [u8],
    method: u16,
    compressed: usize,
    pub size: usize,
    local: usize,
}
pub struct Entries<'a> {
    b: &'a [u8],
    at: usize,
    remaining: u16,
}
impl<'a> Entries<'a> {
    pub fn new(b: &'a [u8]) -> Result<Self> {
        let end = b
            .windows(4)
            .rposition(|v| v == b"PK\x05\x06")
            .ok_or(Error::from("ZipNoEndRecord"))?;
        bytes(b, end, 22)?;
        let count = u16at(b, end + 10)?;
        let size = u32at(b, end + 12)?;
        let offset = u32at(b, end + 16)?;
        if count == u16::MAX
            || size == u32::MAX
            || offset == u32::MAX
            || u16at(b, end + 8)? == u16::MAX
        {
            return Err("Zip64Unsupported".into());
        }
        bytes(b, offset as usize, size as usize)?;
        Ok(Self {
            b,
            at: offset as usize,
            remaining: count,
        })
    }
    pub fn next_entry(&mut self) -> Result<Option<Entry<'a>>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        let h = bytes(self.b, self.at, 46)?;
        if &h[..4] != b"PK\x01\x02" {
            return Err("ZipTruncated".into());
        }
        if u16at(h, 8)? & 1 != 0 {
            return Err("ZipEntryEncrypted".into());
        }
        let n = u16at(h, 28)? as usize;
        if n == 0 || n > 4096 {
            return Err("ZipUnsafeEntryName".into());
        }
        let extra = u16at(h, 30)? as usize;
        let comment = u16at(h, 32)? as usize;
        let name = bytes(self.b, self.at + 46, n)?;
        bytes(self.b, self.at + 46, n + extra + comment)?;
        if name.contains(&b'\\') {
            return Err("ZipUnsafeEntryName".into());
        }
        self.at += 46 + n + extra + comment;
        self.remaining -= 1;
        Ok(Some(Entry {
            name,
            method: u16at(h, 10)?,
            compressed: u32at(h, 20)? as usize,
            size: u32at(h, 24)? as usize,
            local: u32at(h, 42)? as usize,
        }))
    }
}
pub fn find<'a>(b: &'a [u8], name: &str) -> Result<Option<Entry<'a>>> {
    let mut it = Entries::new(b)?;
    while let Some(e) = it.next_entry()? {
        if e.name == name.as_bytes() {
            return Ok(Some(e));
        }
    }
    Ok(None)
}
pub fn read(b: &[u8], e: &Entry<'_>, max: usize) -> Result<Vec<u8>> {
    if e.size > max {
        return Err("ZipEntryTooLarge".into());
    }
    let h = bytes(b, e.local, 30)?;
    if &h[..4] != b"PK\x03\x04" {
        return Err("ZipTruncated".into());
    }
    if u16at(h, 6)? & 1 != 0 {
        return Err("ZipEntryEncrypted".into());
    }
    let start = e.local + 30 + u16at(h, 26)? as usize + u16at(h, 28)? as usize;
    let compressed = bytes(b, start, e.compressed)?;
    match e.method {
        0 => {
            if compressed.len() != e.size {
                return Err("ZipTruncated".into());
            }
            Ok(compressed.to_vec())
        }
        8 => {
            let mut decoder = flate2::read::DeflateDecoder::new(compressed);
            let mut out = vec![0; e.size];
            decoder
                .read_exact(&mut out)
                .map_err(|_| Error::from("ZipTruncated"))?;
            // The original reader checks one extra byte, treating an extra zero as EOF.
            let mut extra = [0];
            if decoder
                .read(&mut extra)
                .map_err(|_| Error::from("ZipTruncated"))?
                != 0
                && extra[0] != 0
            {
                return Err("ZipTruncated".into());
            }
            Ok(out)
        }
        _ => Err("ZipUnsupportedCompression".into()),
    }
}
pub fn read_named(b: &[u8], name: &str, max: usize) -> Result<Option<Vec<u8>>> {
    find(b, name)?.map(|e| read(b, &e, max)).transpose()
}
