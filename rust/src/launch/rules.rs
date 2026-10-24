//! The oracle's small byte-pattern language, deliberately not a general regex.
use super::Error;
pub fn regex_matches(p: &str, t: &str) -> Result<bool, Error> {
    if p.len() > 256 || t.len() > 1024 {
        return Err("RulePatternTooLong");
    }
    fn here(p: &[u8], t: &[u8]) -> Result<bool, Error> {
        if p.is_empty() {
            return Ok(true);
        }
        if p == b"$" {
            return Ok(t.is_empty());
        }
        let (kind, c, n) = match p[0] {
            b'.' => (1, 0, 1),
            b'\\' => {
                if p.len() == 1 {
                    return Err("UnsupportedRulePattern");
                };
                (if p[1] == b'd' { 2 } else { 0 }, p[1], 2)
            }
            c if b"[](){}+?|^".contains(&c) => return Err("UnsupportedRulePattern"),
            c => (0, c, 1),
        };
        let matches =
            |b: u8| kind == 1 || (kind == 2 && b.is_ascii_digit()) || (kind == 0 && b == c);
        if p.get(n) == Some(&b'*') {
            let end = t.iter().take_while(|&&b| matches(b)).count();
            for i in (0..=end).rev() {
                if here(&p[n + 1..], &t[i..])? {
                    return Ok(true);
                }
            }
            Ok(false)
        } else if t.first().is_some_and(|&b| matches(b)) {
            here(&p[n..], &t[1..])
        } else {
            Ok(false)
        }
    }
    if let Some(p) = p.strip_prefix('^') {
        here(p.as_bytes(), t.as_bytes())
    } else {
        for i in 0..=t.len() {
            if here(p.as_bytes(), &t.as_bytes()[i..])? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
pub fn compare_versions(a: &str, b: &str) -> Result<std::cmp::Ordering, Error> {
    fn part(s: Option<&str>) -> Result<u64, Error> {
        let Some(s) = s else { return Ok(0) };
        // Zig parseInt permits one leading sign and underscores between digits.
        let negative = s.starts_with('-');
        let s = s
            .strip_prefix('+')
            .or_else(|| s.strip_prefix('-'))
            .unwrap_or(s);
        if s.is_empty()
            || s.starts_with('_')
            || s.ends_with('_')
            || !s.bytes().all(|b| b.is_ascii_digit() || b == b'_')
        {
            return Err("UnsupportedVersionRange");
        }
        let n: u64 = s
            .replace('_', "")
            .parse()
            .map_err(|_| "UnsupportedVersionRange")?;
        if negative && n != 0 {
            Err("UnsupportedVersionRange")
        } else {
            Ok(n)
        }
    }
    let (mut a, mut b) = (a.split('.'), b.split('.'));
    loop {
        let (x, y) = (a.next(), b.next());
        if x.is_none() && y.is_none() {
            return Ok(std::cmp::Ordering::Equal);
        }
        let o = part(x)?.cmp(&part(y)?);
        if !o.is_eq() {
            return Ok(o);
        }
    }
}
pub fn tokenize(input: &str) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    let mut token = Vec::new();
    let (mut state, mut started, mut i) = (0, false, 0);
    let b = input.as_bytes();
    while i < b.len() {
        let c = b[i];
        match (state, c) {
            (0, b' ' | b'\t' | b'\r' | b'\n') => {
                if started {
                    out.push(String::from_utf8(std::mem::take(&mut token)).unwrap());
                    started = false
                }
            }
            (0, b'\'') => {
                state = 1;
                started = true
            }
            (0, b'"') => {
                state = 2;
                started = true
            }
            (1, b'\'') | (2, b'"') => state = 0,
            (0, b'\\') | (2, b'\\') => {
                i += 1;
                if i == b.len() {
                    return Err("InvalidLegacyArguments");
                };
                token.push(b[i]);
                started = true
            }
            _ => {
                token.push(c);
                started = true
            }
        };
        i += 1
    }
    if state != 0 {
        return Err("InvalidLegacyArguments");
    }
    if started {
        out.push(String::from_utf8(token).unwrap())
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patterns() {
        assert_eq!(regex_matches(r"^10\.\d*", "10.44"), Ok(true));
        assert_eq!(regex_matches("a+", "a"), Err("UnsupportedRulePattern"));
        assert_eq!(regex_matches("z[", "abc"), Ok(false));
        assert_eq!(regex_matches("*", "*"), Ok(true));
    }
    #[test]
    fn legacy() {
        assert_eq!(
            tokenize("a '' \"x y\" 'a\\b' c\\ d").unwrap(),
            vec!["a", "", "x y", "a\\b", "c d"]
        );
        assert_eq!(tokenize("a\\"), Err("InvalidLegacyArguments"));
    }
}
