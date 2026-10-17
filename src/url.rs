//! Percent-encoding of `href` and `src` (SPEC C12).

use unicode_normalization::UnicodeNormalization;

/// An RFC 3986 scheme of at least two characters followed by `:`.
pub fn has_scheme(v: &str) -> bool {
    let Some(colon) = v.find(':') else {
        return false;
    };
    let scheme = &v[..colon];
    scheme.len() >= 2
        && scheme.as_bytes()[0].is_ascii_alphabetic()
        && scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

/// Absolute in the sense of the reference profile's `src`: a scheme or a
/// leading `/`.
pub fn is_absolute(v: &str) -> bool {
    has_scheme(v) || v.starts_with('/')
}

const PATH: &[u8] = b"-._~/";
const QUERY: &[u8] = b"-._~/!$&'()*+,;=:@?";

pub fn canonical(v: &str) -> String {
    if has_scheme(v) {
        return v.to_string();
    }
    let (reference, fragment) = match v.find('#') {
        Some(i) => (&v[..i], Some(&v[i + 1..])),
        None => (v, None),
    };
    let (path, query) = match reference.find('?') {
        Some(i) => (&reference[..i], Some(&reference[i + 1..])),
        None => (reference, None),
    };
    let mut out = encode(path, PATH);
    if let Some(q) = query {
        out.push('?');
        out.push_str(&encode(q, QUERY));
    }
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(&encode(f, PATH));
    }
    out
}

/// Decodes `%HH`, normalises to NFC and encodes everything outside the
/// unreserved characters and `keep`.
fn encode(part: &str, keep: &[u8]) -> String {
    let b = part.as_bytes();
    let mut decoded = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && b[i + 1].is_ascii_hexdigit()
            && b[i + 2].is_ascii_hexdigit()
        {
            decoded.push(hex(b[i + 1]) << 4 | hex(b[i + 2]));
            i += 3;
        } else {
            decoded.push(b[i]);
            i += 1;
        }
    }
    let bytes = match String::from_utf8(decoded) {
        Ok(s) => s.nfc().collect::<String>().into_bytes(),
        Err(e) => e.into_bytes(),
    };
    let mut out = String::with_capacity(bytes.len());
    for byte in bytes {
        if byte.is_ascii_alphanumeric() || keep.contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn hex(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => b.to_ascii_uppercase().wrapping_sub(b'A').wrapping_add(10),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c12() {
        assert_eq!(
            canonical("Eisen Mangel/übersicht.html#Dosis Kinder"),
            "Eisen%20Mangel/%C3%BCbersicht.html#Dosis%20Kinder"
        );
        assert_eq!(canonical("a%20b.html"), "a%20b.html");
        assert_eq!(canonical("100%.html"), "100%25.html");
        assert_eq!(canonical("a%zz.html"), "a%25zz.html");
        assert_eq!(canonical("%c3%bc.html"), "%C3%BC.html");
        assert_eq!(canonical("C:/Daten/a.html"), "C%3A/Daten/a.html");
        assert_eq!(canonical("https://x.org/a b"), "https://x.org/a b");
        assert_eq!(
            canonical("suche.html?q=eisen mangel&x=ü"),
            "suche.html?q=eisen%20mangel&x=%C3%BC"
        );
        assert_eq!(canonical("a.html#x#y"), "a.html#x%23y");
        assert_eq!(canonical("ro\u{308}ntgen.png"), "r%C3%B6ntgen.png");
        assert_eq!(canonical("50%"), "50%25");
    }
}
