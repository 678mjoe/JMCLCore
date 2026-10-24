//! Native archive reader matching the pinned std.zip metadata checks. Raw CRC
//! bytes are deliberately not reinterpreted as content verification: source SHA1
//! checks the complete archive against the plan, and old native extraction ignores CRC.
use super::Error;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};
type Result<T> = std::result::Result<T, Error>;
fn err(s: &str) -> Error {
    Error::named(s)
}
fn u16at(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes(b[p..p + 2].try_into().unwrap())
}
fn u32at(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
fn u64at(b: &[u8], p: usize) -> u64 {
    u64::from_le_bytes(b[p..p + 8].try_into().unwrap())
}
fn read_at(f: &mut File, offset: u64, length: usize) -> Result<Vec<u8>> {
    f.seek(SeekFrom::Start(offset))?;
    let mut b = vec![0; length];
    f.read_exact(&mut b).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            err("EndOfStream")
        } else {
            e.into()
        }
    })?;
    Ok(b)
}
#[derive(Debug)]
pub(super) struct Entry {
    pub name: Vec<u8>,
    pub directory: bool,
    pub size: u64,
    pub compressed: u64,
    pub method: u16,
    local: u64,
    central: [u8; 46],
}
fn extents(extra: &[u8], fields: &mut [u64]) -> Result<()> {
    let mut p = 0;
    while p + 4 <= extra.len() {
        let id = u16at(extra, p);
        let len = usize::from(u16at(extra, p + 2));
        let end = p + 4 + len;
        if end > extra.len() {
            return Err(err("ZipBadExtraFieldSize"));
        }
        if id == 1 {
            let mut pos = p + 4;
            for field in fields.iter_mut() {
                if *field == u64::from(u32::MAX) {
                    if pos + 8 > end {
                        return Err(err("ZipBadCd64Size"));
                    }
                    *field = u64at(extra, pos);
                    pos += 8;
                }
            }
        }
        p = end;
    }
    Ok(())
}
pub(super) fn entries(
    f: &mut File,
    mut inspect: impl FnMut(&Entry) -> Result<bool>,
) -> Result<Vec<Entry>> {
    let len = f.metadata()?.len();
    let tail_len = len.min(65557) as usize;
    let tail = read_at(f, len - tail_len as u64, tail_len)?;
    let pos = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&p| {
            tail[p..].starts_with(b"PK\x05\x06")
                && p + 22 + usize::from(u16at(&tail, p + 20)) == tail.len()
        })
        .ok_or_else(|| err("ZipNoEndRecord"))?;
    let end = &tail[pos..pos + 22];
    let disk_count = u16at(end, 8);
    let count = u16at(end, 10);
    if disk_count != u16::MAX && disk_count > count {
        return Err(err("ZipDiskRecordCountTooLarge"));
    }
    if u16at(end, 4) != 0
        || u16at(end, 6) != 0
        || (disk_count != u16::MAX && count != u16::MAX && disk_count != count)
    {
        return Err(err("ZipMultiDiskUnsupported"));
    }
    let mut count = u64::from(count);
    let mut size = u64::from(u32at(end, 12));
    let mut offset = u64::from(u32at(end, 16));
    if count == u64::from(u16::MAX)
        || size == u64::from(u32::MAX)
        || offset == u64::from(u32::MAX)
        || disk_count == u16::MAX
    {
        let end_offset = len - tail_len as u64 + pos as u64;
        let locator = read_at(
            f,
            end_offset
                .checked_sub(20)
                .ok_or_else(|| err("ZipTruncated"))?,
            20,
        )?;
        if &locator[..4] != b"PK\x06\x07" {
            return Err(err("ZipBadLocatorSig"));
        }
        if u32at(&locator, 4) != 0 {
            return Err(err("ZipUnsupportedZip64DiskCount"));
        }
        if u32at(&locator, 16) != 1 {
            return Err(err("ZipMultiDiskUnsupported"));
        }
        let z = read_at(f, u64at(&locator, 8), 56)?;
        if &z[..4] != b"PK\x06\x06" {
            return Err(err("ZipBadEndRecord64Sig"));
        }
        match u64at(&z, 4).cmp(&44) {
            std::cmp::Ordering::Less => return Err(err("ZipEndRecord64SizeTooSmall")),
            std::cmp::Ordering::Greater => return Err(err("ZipEndRecord64UnhandledExtraData")),
            _ => {}
        }
        if u16at(&z, 14) > 45 {
            return Err(err("ZipUnsupportedVersion"));
        }
        if u32at(&z, 16) != 0 || u32at(&z, 20) != 0 || u64at(&z, 24) != u64at(&z, 32) {
            return Err(err("ZipMultiDiskUnsupported"));
        }
        for (field, replacement, sentinel, mismatch) in [
            (
                &mut count,
                u64at(&z, 32),
                u64::from(u16::MAX),
                "Zip64RecordCountTotalMismatch",
            ),
            (
                &mut size,
                u64at(&z, 40),
                u64::from(u32::MAX),
                "Zip64CentralDirectorySizeMismatch",
            ),
            (
                &mut offset,
                u64at(&z, 48),
                u64::from(u32::MAX),
                "Zip64CentralDirectoryOffsetMismatch",
            ),
        ] {
            if *field == sentinel {
                *field = replacement;
            } else if *field != replacement {
                return Err(err(mismatch));
            }
        }
    }
    let mut result = Vec::new();
    let mut consumed = 0u64;
    for index in 0..count {
        let header = read_at(
            f,
            offset
                .checked_add(consumed)
                .ok_or_else(|| err("ZipBadCdOffset"))?,
            46,
        )?;
        if &header[..4] != b"PK\x01\x02" {
            return Err(err("ZipBadCdOffset"));
        }
        if u16at(&header, 8) & 1 != 0 {
            return Err(err("ZipEncryptionUnsupported"));
        }
        if u16at(&header, 34) != 0 {
            return Err(err("ZipMultiDiskUnsupported"));
        }
        let name_len = usize::from(u16at(&header, 28));
        let extra_len = usize::from(u16at(&header, 30));
        let name_offset = offset + consumed + 46;
        let extra = read_at(f, name_offset + name_len as u64, extra_len)?;
        let mut fields = [
            u64::from(u32at(&header, 24)),
            u64::from(u32at(&header, 20)),
            u64::from(u32at(&header, 42)),
        ];
        extents(&extra, &mut fields)?;
        consumed += 46 + name_len as u64 + extra_len as u64 + u64::from(u16at(&header, 32));
        if index >= 4096 {
            return Err(err("TooManyArchiveEntries"));
        }
        if name_len == 0 || name_len > 4096 {
            return Err(err("UnsafeArchivePath"));
        }
        let name = read_at(f, name_offset, name_len).map_err(|e| {
            if e.name() == "EndOfStream" {
                err("ZipTruncated")
            } else {
                e
            }
        })?;
        if name.contains(&b'\\') {
            return Err(err("UnsafeArchivePath"));
        }
        let directory = name.ends_with(b"/");
        let path = if directory {
            &name[..name.len() - 1]
        } else {
            &name
        };
        // Lossy conversion is validation-only: all path rules concern ASCII.
        // Actual output names remain byte-exact on Unix and WTF-8 on Windows.
        crate::plan::validate_relative_path(&String::from_utf8_lossy(path))
            .map_err(|e| Error::named(e.name()))?;
        let attrs = u32at(&header, 38);
        if header[5] == 3 {
            let kind = (attrs >> 16) & 0o170000;
            if ![0, 0o100000, 0o040000].contains(&kind) {
                return Err(err("UnsupportedArchiveEntryType"));
            }
            if (kind == 0o040000) != directory {
                return Err(err("ArchiveEntryTypeMismatch"));
            }
        }
        if attrs & 0x10 != 0 && !directory {
            return Err(err("ArchiveEntryTypeMismatch"));
        }
        let entry = Entry {
            name,
            directory,
            size: fields[0],
            compressed: fields[1],
            local: fields[2],
            method: u16at(&header, 10),
            central: header.try_into().unwrap(),
        };
        if inspect(&entry)? {
            result.push(entry);
        }
    }
    if consumed != size {
        return Err(err(if size > consumed {
            "ZipCdOversized"
        } else {
            "ZipCdUndersized"
        }));
    }
    Ok(result)
}
pub(super) fn extract(f: &mut File, e: &Entry, output: &mut dyn Write) -> Result<()> {
    let h = read_at(f, e.local, 30)?;
    if &h[..4] != b"PK\x03\x04" {
        return Err(err("ZipBadFileOffset"));
    }
    for (local, central, name) in [
        (4, 6, "ZipMismatchVersionNeeded"),
        (10, 12, "ZipMismatchModTime"),
        (12, 14, "ZipMismatchModDate"),
        (6, 8, "ZipMismatchFlags"),
    ] {
        if u16at(&h, local) != u16at(&e.central, central) {
            return Err(err(name));
        }
    }
    if u32at(&h, 14) != 0 && u32at(&h, 14) != u32at(&e.central, 16) {
        return Err(err("ZipMismatchCrc32"));
    }
    let name_len = usize::from(u16at(&h, 26));
    let extra_len = usize::from(u16at(&h, 28));
    let extra = read_at(f, e.local + 30 + name_len as u64, extra_len)?;
    let mut fields = [u64::from(u32at(&h, 22)), u64::from(u32at(&h, 18))];
    extents(&extra, &mut fields)?;
    if fields[1] != 0 && fields[1] != e.compressed {
        return Err(err("ZipMismatchCompLen"));
    }
    if fields[0] != 0 && fields[0] != e.size {
        return Err(err("ZipMismatchUncompLen"));
    }
    if name_len != e.name.len() {
        return Err(err("ZipMismatchFilenameLen"));
    }
    if e.directory {
        return if e.size == 0 {
            Ok(())
        } else {
            Err(err("ZipBadDirectorySize"))
        };
    }
    f.seek(SeekFrom::Start(
        e.local + 30 + name_len as u64 + extra_len as u64,
    ))?;
    // Match the pinned extractor's exact output-length behavior. Source remains
    // verified against the plan; no CRC strengthening is silently introduced.
    let copied = match e.method {
        0 => std::io::copy(&mut f.take(e.size), output)?,
        8 => std::io::copy(
            &mut flate2::read::DeflateDecoder::new(f).take(e.size),
            output,
        )?,
        _ => return Err(err("UnsupportedCompressionMethod")),
    };
    if copied != e.size {
        return Err(err("ZipDecompressTruncated"));
    }
    Ok(())
}
