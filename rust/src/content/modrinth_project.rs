//! Private ordered cursor and Project schema (also reused by Version schemas).
//! Unknown containers use an input-bounded stack;
//! scalar numbers retain their spelling, never passing through binary64.
use super::{Error, MetadataBytes, Result, WireProject};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Token<'a> {
    String(&'a str),
    Number(&'a str),
    Null,
    Bool(bool),
    OpenObject,
    CloseObject,
    OpenArray,
    CloseArray,
    Colon,
    Comma,
    End,
}
pub(crate) struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    pub(crate) fn error<T>(&self) -> Result<T> {
        Err("SyntaxError".into())
    }
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    pub(crate) fn finish(&self) -> Result<()> {
        if self.bytes[self.at..]
            .iter()
            .any(|b| !matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            return self.error();
        }
        Ok(())
    }
    pub(in crate::content) fn array_start(&mut self) -> Result<bool> {
        if self.peek_value()? != b'[' {
            return Err("UnexpectedToken".into());
        }
        self.next()?;
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.at += 1;
        }
        if self.bytes.get(self.at) == Some(&b']') {
            self.next()?;
            Ok(false)
        } else {
            Ok(true)
        }
    }
    fn byte(&mut self) -> Result<u8> {
        let b = *self
            .bytes
            .get(self.at)
            .ok_or_else(|| Error("UnexpectedEndOfInput".into()))?;
        self.at += 1;
        Ok(b)
    }
    fn hex(&mut self) -> Result<u16> {
        let mut n = 0;
        for _ in 0..4 {
            let b = self.byte()?;
            n = n * 16
                + match b {
                    b'0'..=b'9' => u16::from(b - b'0'),
                    b'a'..=b'f' => u16::from(b - b'a' + 10),
                    b'A'..=b'F' => u16::from(b - b'A' + 10),
                    _ => return self.error(),
                };
        }
        Ok(n)
    }
    fn string(&mut self, start: usize) -> Result<Token<'a>> {
        loop {
            match self.byte()? {
                b'"' => {
                    return Ok(Token::String(
                        std::str::from_utf8(&self.bytes[start..self.at])
                            .map_err(|_| Error("SyntaxError".into()))?,
                    ));
                }
                b'\\' => match self.byte()? {
                    b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {}
                    b'u' => {
                        let n = self.hex()?;
                        if (0xd800..=0xdbff).contains(&n) {
                            if self.byte()? != b'\\' || self.byte()? != b'u' {
                                return self.error();
                            }
                            let low = self.hex()?;
                            if !(0xdc00..=0xdfff).contains(&low) {
                                return self.error();
                            }
                        } else if (0xdc00..=0xdfff).contains(&n) {
                            return self.error();
                        }
                    }
                    _ => return self.error(),
                },
                0..=31 => return self.error(),
                b @ 128..=255 => {
                    let len = match b {
                        0xc2..=0xdf => 2,
                        0xe0..=0xef => 3,
                        0xf0..=0xf4 => 4,
                        _ => return self.error(),
                    };
                    let begin = self.at - 1;
                    for i in 1..len {
                        let next = self.byte()?;
                        if !(0x80..=0xbf).contains(&next)
                            || (i == 1
                                && ((b == 0xe0 && next < 0xa0)
                                    || (b == 0xed && next >= 0xa0)
                                    || (b == 0xf0 && next < 0x90)
                                    || (b == 0xf4 && next >= 0x90)))
                        {
                            return self.error();
                        }
                    }
                    debug_assert!(std::str::from_utf8(&self.bytes[begin..self.at]).is_ok());
                }
                _ => {}
            }
        }
    }
    pub(crate) fn next(&mut self) -> Result<Token<'a>> {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.at += 1;
        }
        let start = self.at;
        let Some(&b) = self.bytes.get(self.at) else {
            return Ok(Token::End);
        };
        self.at += 1;
        Ok(match b {
            b'"' => return self.string(start),
            b'{' => Token::OpenObject,
            b'}' => Token::CloseObject,
            b'[' => Token::OpenArray,
            b']' => Token::CloseArray,
            b':' => Token::Colon,
            b',' => Token::Comma,
            b'n' | b't' | b'f' => {
                let tail: &[u8] = match b {
                    b'n' => b"ull",
                    b't' => b"rue",
                    _ => b"alse",
                };
                for &wanted in tail {
                    if self.byte()? != wanted {
                        return self.error();
                    }
                }
                if b == b'n' {
                    Token::Null
                } else {
                    Token::Bool(b == b't')
                }
            }
            b'-' | b'0'..=b'9' => {
                if b == b'-' {
                    if !self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
                        if self.at == self.bytes.len() {
                            return Err("UnexpectedEndOfInput".into());
                        }
                        return self.error();
                    }
                    self.at += 1;
                }
                let first = self.bytes[start + usize::from(b == b'-')];
                if first != b'0' {
                    while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
                        self.at += 1;
                    }
                }
                if self.bytes.get(self.at) == Some(&b'.') {
                    self.at += 1;
                    self.digits()?;
                }
                if self
                    .bytes
                    .get(self.at)
                    .is_some_and(|b| matches!(b, b'e' | b'E'))
                {
                    self.at += 1;
                    if self
                        .bytes
                        .get(self.at)
                        .is_some_and(|b| matches!(b, b'+' | b'-'))
                    {
                        self.at += 1;
                    }
                    self.digits()?;
                }
                Token::Number(
                    std::str::from_utf8(&self.bytes[start..self.at]).expect("number is ASCII"),
                )
            }
            _ => return self.error(),
        })
    }
    fn digits(&mut self) -> Result<()> {
        let start = self.at;
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        if self.at == start {
            if self.at == self.bytes.len() {
                return Err("UnexpectedEndOfInput".into());
            }
            return self.error();
        }
        Ok(())
    }
    // Slice children peek a wrong closing token; numeric children ask the
    // scanner to consume it and receive SyntaxError instead.
    pub(crate) fn separator(&mut self, close: Token<'a>, child_peeks: bool) -> Result<Token<'a>> {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.at += 1;
        }
        match self.bytes.get(self.at) {
            None => Err("UnexpectedEndOfInput".into()),
            Some(b',' | b']' | b'}') => {
                let t = self.next()?;
                if t == Token::Comma || t == close {
                    Ok(t)
                } else if !child_peeks {
                    self.error()
                } else {
                    Err("UnexpectedToken".into())
                }
            }
            _ => self.error(),
        }
    }
    pub(crate) fn expect(&mut self, t: Token<'_>) -> Result<()> {
        debug_assert!(t == Token::Colon);
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.at += 1;
        }
        match self.bytes.get(self.at) {
            Some(b':') => {
                self.at += 1;
                Ok(())
            }
            None => Err("UnexpectedEndOfInput".into()),
            _ => self.error(),
        }
    }
    pub(crate) fn object_key(&mut self, allow_end: bool) -> Result<Token<'a>> {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.at += 1;
        }
        match self.bytes.get(self.at) {
            Some(b'"') => self.next(),
            Some(b'}') if allow_end => self.next(),
            None => Err("UnexpectedEndOfInput".into()),
            _ => self.error(),
        }
    }
    pub(crate) fn peek_value(&mut self) -> Result<u8> {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.at += 1;
        }
        let b = *self
            .bytes
            .get(self.at)
            .ok_or_else(|| Error("UnexpectedEndOfInput".into()))?;
        if !matches!(
            b,
            b'"' | b'[' | b'{' | b'n' | b't' | b'f' | b'-' | b'0'..=b'9'
        ) {
            return self.error();
        }
        Ok(b)
    }
    pub(crate) fn value_bytes(&mut self) -> Result<MetadataBytes> {
        if !matches!(self.peek_value()?, b'"' | b'[') {
            return Err("UnexpectedToken".into());
        }
        let first = self.next()?;
        match first {
            Token::String(s) => decode(s).map(MetadataBytes::from),
            Token::OpenArray => {
                let mut out = vec![];
                let mut n = self.next()?;
                if n == Token::CloseArray {
                    return Ok(out.into());
                }
                loop {
                    let raw = match n {
                        Token::String(s) | Token::Number(s) => s,
                        Token::End => return Err("UnexpectedEndOfInput".into()),
                        Token::CloseArray | Token::CloseObject | Token::Colon | Token::Comma => {
                            return self.error();
                        }
                        _ => return Err("UnexpectedToken".into()),
                    };
                    out.push(
                        crate::json_number::typed_raw(raw, 8).map_err(|e| Error(e.name().into()))?
                            as u8,
                    );
                    match self.separator(Token::CloseArray, false)? {
                        Token::CloseArray => break,
                        Token::Comma => n = self.next()?,
                        Token::End => return Err("UnexpectedEndOfInput".into()),
                        _ => return self.error(),
                    }
                }
                Ok(out.into())
            }
            Token::End => Err("UnexpectedEndOfInput".into()),
            _ => Err("UnexpectedToken".into()),
        }
    }
    pub(crate) fn skip(&mut self) -> Result<()> {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum State {
            ArrayStart,
            ArrayValue,
            ArrayAfter,
            ObjectStart,
            ObjectKey,
            ObjectColon,
            ObjectValue,
            ObjectAfter,
        }
        use State::*;
        // Synthetic root uses the object-value transition. No recursive calls.
        let mut stack = vec![ObjectValue];
        while let Some(state) = stack.last_mut() {
            // Reject punctuation errors before trying to lex a spurious value.
            while self
                .bytes
                .get(self.at)
                .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
            {
                self.at += 1;
            }
            let b = self
                .bytes
                .get(self.at)
                .copied()
                .ok_or_else(|| Error("UnexpectedEndOfInput".into()))?;
            let allowed = match *state {
                ArrayAfter => matches!(b, b',' | b']'),
                ObjectAfter => matches!(b, b',' | b'}'),
                ObjectColon => b == b':',
                ObjectStart => matches!(b, b'"' | b'}'),
                ObjectKey => b == b'"',
                _ => true,
            };
            if !allowed {
                return self.error();
            }
            let token = self.next()?;
            if token == Token::End {
                return Err("UnexpectedEndOfInput".into());
            }
            match *state {
                ArrayStart if token == Token::CloseArray => {
                    stack.pop();
                }
                ArrayStart | ArrayValue | ObjectValue => {
                    *state = if *state == ObjectValue {
                        ObjectAfter
                    } else {
                        ArrayAfter
                    };
                    match token {
                        Token::OpenArray => stack.push(ArrayStart),
                        Token::OpenObject => stack.push(ObjectStart),
                        Token::String(_) | Token::Number(_) | Token::Null | Token::Bool(_) => {}
                        _ => return self.error(),
                    }
                }
                ArrayAfter => match token {
                    Token::CloseArray => {
                        stack.pop();
                    }
                    Token::Comma => *state = ArrayValue,
                    _ => return self.error(),
                },
                ObjectStart if token == Token::CloseObject => {
                    stack.pop();
                }
                ObjectStart | ObjectKey => {
                    if matches!(token, Token::String(_)) {
                        *state = ObjectColon
                    } else {
                        return self.error();
                    }
                }
                ObjectColon => {
                    if token == Token::Colon {
                        *state = ObjectValue
                    } else {
                        return self.error();
                    }
                }
                ObjectAfter => match token {
                    Token::CloseObject => {
                        stack.pop();
                    }
                    Token::Comma => *state = ObjectKey,
                    _ => return self.error(),
                },
            }
            // The synthetic root's value has completed when no child remains.
            if stack.len() == 1 && stack[0] == ObjectAfter {
                return Ok(());
            }
        }
        Ok(())
    }
}
pub(crate) fn decode(raw: &str) -> Result<Vec<u8>> {
    crate::auth::json::Json(raw)
        .string()
        .map(|s| s.as_bytes().to_vec())
        .map_err(|_| Error("SyntaxError".into()))
}

pub(super) fn parse(bytes: &[u8]) -> Result<WireProject> {
    if bytes.len() > 1024 * 1024 {
        return Err("ModrinthResponseTooLarge".into());
    }
    let mut c = Cursor { bytes, at: 0 };
    match c.next()? {
        Token::OpenObject => {}
        Token::End => return Err("UnexpectedEndOfInput".into()),
        _ => return Err("UnexpectedToken".into()),
    }
    let (mut id, mut slug, mut kind, mut title, mut loaders) = (None, None, None, None, None);
    let mut seen = 0u8;
    let mut key = c.object_key(true)?;
    if key != Token::CloseObject {
        loop {
            let Token::String(raw) = key else {
                return if key == Token::End {
                    Err("UnexpectedEndOfInput".into())
                } else {
                    Err("SyntaxError".into())
                };
            };
            let name = decode(raw)?;
            let bit = match name.as_slice() {
                b"id" => 1,
                b"slug" => 2,
                b"project_type" => 4,
                b"title" => 8,
                b"loaders" => 16,
                _ => 0,
            };
            if seen & bit != 0 {
                return Err("DuplicateField".into());
            }
            seen |= bit;
            c.expect(Token::Colon)?;
            if bit == 0 {
                c.skip()?
            } else {
                match bit {
                    1 => id = Some(c.value_bytes()?),
                    2 => slug = Some(c.value_bytes()?),
                    4 => kind = Some(c.value_bytes()?),
                    8 => {
                        title = Some(if c.peek_value()? == b'n' {
                            c.next()?;
                            None
                        } else {
                            Some(c.value_bytes()?)
                        })
                    }
                    16 => {
                        if c.peek_value()? != b'[' {
                            return Err("UnexpectedToken".into());
                        }
                        let mut values = vec![];
                        c.next()?;
                        while c
                            .bytes
                            .get(c.at)
                            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
                        {
                            c.at += 1;
                        }
                        if c.bytes.get(c.at) != Some(&b']') {
                            loop {
                                values.push(c.value_bytes()?);
                                match c.separator(Token::CloseArray, true)? {
                                    Token::CloseArray => break,
                                    Token::Comma => {}
                                    Token::End => return Err("UnexpectedEndOfInput".into()),
                                    _ => return c.error(),
                                }
                            }
                        } else {
                            c.next()?;
                        }
                        loaders = Some(values);
                    }
                    _ => unreachable!(),
                }
            }
            match c.separator(Token::CloseObject, false)? {
                Token::CloseObject => break,
                Token::Comma => key = c.object_key(false)?,
                Token::End => return Err("UnexpectedEndOfInput".into()),
                _ => return c.error(),
            }
        }
    }
    let p = WireProject {
        id: id.ok_or("MissingField")?,
        slug: slug.ok_or("MissingField")?,
        project_type: kind.ok_or("MissingField")?,
        title: title.unwrap_or(None),
        loaders: loaders.unwrap_or_default(),
    };
    if c.bytes[c.at..]
        .iter()
        .any(|b| !matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
    {
        return Err("SyntaxError".into());
    }
    Ok(p)
}
