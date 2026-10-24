//! Bounded, type-preserving NBT codec. Unknown fields survive LevelName edits.
use super::{Error, Result};
#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(u32),
    Double(u64),
    Bytes(Vec<u8>),
    String(Vec<u8>),
    List(u8, Vec<Tag>),
    Compound(Vec<(Vec<u8>, Tag)>),
    Ints(Vec<i32>),
    Longs(Vec<i64>),
}
impl Tag {
    fn id(&self) -> u8 {
        match self {
            Self::End => 0,
            Self::Byte(_) => 1,
            Self::Short(_) => 2,
            Self::Int(_) => 3,
            Self::Long(_) => 4,
            Self::Float(_) => 5,
            Self::Double(_) => 6,
            Self::Bytes(_) => 7,
            Self::String(_) => 8,
            Self::List(..) => 9,
            Self::Compound(_) => 10,
            Self::Ints(_) => 11,
            Self::Longs(_) => 12,
        }
    }
    pub fn get(&self, name: &str) -> Option<&Tag> {
        if let Self::Compound(entries) = self {
            entries
                .iter()
                .find(|(n, _)| n == name.as_bytes())
                .map(|(_, t)| t)
        } else {
            None
        }
    }
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Tag> {
        if let Self::Compound(entries) = self {
            entries
                .iter_mut()
                .find(|(n, _)| n == name.as_bytes())
                .map(|(_, t)| t)
        } else {
            None
        }
    }
    pub fn text(&self) -> Option<&str> {
        if let Self::String(v) = self {
            std::str::from_utf8(v).ok()
        } else {
            None
        }
    }
}
#[derive(Debug, PartialEq)]
pub struct Document {
    pub name: Vec<u8>,
    pub root: Tag,
}
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}
impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.pos.checked_add(n).ok_or(Error::InvalidLevelData)?;
        let v = self
            .data
            .get(self.pos..end)
            .ok_or(Error::InvalidLevelData)?;
        self.pos = end;
        Ok(v)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn n<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }
    fn text(&mut self) -> Result<Vec<u8>> {
        let len = u16::from_be_bytes(self.n()?) as usize;
        Ok(self.take(len)?.to_vec())
    }
    fn count(&mut self) -> Result<usize> {
        let n = i32::from_be_bytes(self.n()?);
        if !(0..=1 << 24).contains(&n) {
            return Err(Error::InvalidLevelData);
        }
        Ok(n as usize)
    }
    fn tag(&mut self, id: u8, depth: usize) -> Result<Tag> {
        if depth > 64 {
            return Err(Error::InvalidLevelData);
        }
        Ok(match id {
            0 => Tag::End,
            1 => Tag::Byte(self.byte()? as i8),
            2 => Tag::Short(i16::from_be_bytes(self.n()?)),
            3 => Tag::Int(i32::from_be_bytes(self.n()?)),
            4 => Tag::Long(i64::from_be_bytes(self.n()?)),
            5 => Tag::Float(u32::from_be_bytes(self.n()?)),
            6 => Tag::Double(u64::from_be_bytes(self.n()?)),
            7 => {
                let n = self.count()?;
                Tag::Bytes(self.take(n)?.to_vec())
            }
            8 => Tag::String(self.text()?),
            9 => {
                let t = self.byte()?;
                let n = self.count()?;
                if n > 0 && (t == 0 || t > 12) {
                    return Err(Error::InvalidLevelData);
                }
                let mut v = Vec::new();
                for _ in 0..n {
                    v.push(self.tag(t, depth + 1)?)
                }
                Tag::List(t, v)
            }
            10 => {
                let mut v = Vec::new();
                loop {
                    let t = self.byte()?;
                    if t == 0 {
                        break;
                    }
                    let name = self.text()?;
                    v.push((name, self.tag(t, depth + 1)?));
                }
                Tag::Compound(v)
            }
            11 => {
                let n = self.count()?;
                let mut v = Vec::new();
                for _ in 0..n {
                    v.push(i32::from_be_bytes(self.n()?))
                }
                Tag::Ints(v)
            }
            12 => {
                let n = self.count()?;
                let mut v = Vec::new();
                for _ in 0..n {
                    v.push(i64::from_be_bytes(self.n()?))
                }
                Tag::Longs(v)
            }
            _ => return Err(Error::InvalidLevelData),
        })
    }
}
pub fn parse(bytes: &[u8]) -> Result<Document> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Error::InvalidLevelData);
    }
    let mut c = Cursor {
        data: bytes,
        pos: 0,
    };
    if c.byte()? != 10 {
        return Err(Error::InvalidLevelData);
    }
    let name = c.text()?;
    let root = c.tag(10, 1)?;
    if c.pos != bytes.len() {
        return Err(Error::InvalidLevelData);
    }
    Ok(Document { name, root })
}
fn text(out: &mut Vec<u8>, s: &[u8]) -> Result<()> {
    if s.len() > 1 << 15 {
        return Err(Error::InvalidLevelData);
    }
    out.extend_from_slice(&(s.len() as u16).to_be_bytes());
    out.extend_from_slice(s);
    Ok(())
}
fn encode(t: &Tag, out: &mut Vec<u8>) -> Result<()> {
    match t {
        Tag::End => {}
        Tag::Byte(v) => out.push(*v as u8),
        Tag::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Float(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Double(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Bytes(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            out.extend_from_slice(v)
        }
        Tag::String(v) => text(out, v)?,
        Tag::List(id, v) => {
            out.push(*id);
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for t in v {
                encode(t, out)?
            }
        }
        Tag::Compound(v) => {
            for (n, t) in v {
                out.push(t.id());
                text(out, n)?;
                encode(t, out)?
            }
            out.push(0)
        }
        Tag::Ints(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for x in v {
                out.extend_from_slice(&x.to_be_bytes())
            }
        }
        Tag::Longs(v) => {
            out.extend_from_slice(&(v.len() as i32).to_be_bytes());
            for x in v {
                out.extend_from_slice(&x.to_be_bytes())
            }
        }
    }
    if out.len() > 16 * 1024 * 1024 {
        return Err(Error::InvalidLevelData);
    }
    Ok(())
}
pub fn serialize(d: &Document) -> Result<Vec<u8>> {
    let mut out = vec![10];
    text(&mut out, &d.name)?;
    encode(&d.root, &mut out)?;
    Ok(out)
}
// Match Zig's numeric equality used for the serialize/reparse guard: NaNs are
// intentionally unequal, while signed zero remains equal.
pub fn equivalent(a: &Tag, b: &Tag) -> bool {
    match (a, b) {
        (Tag::Float(a), Tag::Float(b)) => f32::from_bits(*a) == f32::from_bits(*b),
        (Tag::Double(a), Tag::Double(b)) => f64::from_bits(*a) == f64::from_bits(*b),
        (Tag::Compound(a), Tag::Compound(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|((an, av), (bn, bv))| an == bn && equivalent(av, bv))
        }
        (Tag::List(ai, a), Tag::List(bi, b)) => {
            ai == bi && a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn types_and_unknown_fields_roundtrip() {
        let d = Document {
            name: b"root".to_vec(),
            root: Tag::Compound(vec![
                (
                    b"Data".to_vec(),
                    Tag::Compound(vec![
                        (b"LevelName".to_vec(), Tag::String(b"A".to_vec())),
                        (b"unknown".to_vec(), Tag::Longs(vec![i64::MIN, 3])),
                    ]),
                ),
                (b"float".to_vec(), Tag::Float(0x7fc00042)),
            ]),
        };
        assert_eq!(d, parse(&serialize(&d).unwrap()).unwrap());
    }
    #[test]
    fn malformed_depth_and_trailing_rejected() {
        assert!(parse(&[10, 0, 0, 0, 1]).is_err());
        assert!(parse(&[10, 0, 0, 9, 0, 0, 1, 255, 255, 255, 255]).is_err());
    }
}
