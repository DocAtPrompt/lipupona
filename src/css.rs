//! The one look lipupona takes into CSS (SPEC §3.7): the URLs of `@import`
//! and `url(…)`, outside comments and other strings, that refer to another
//! host. CSS is not otherwise read.

use crate::url;

/// The URLs in `css` that refer to another host, in order.
pub(crate) fn external_references(css: &str) -> Vec<String> {
    urls(css).into_iter().filter(|u| is_external(u)).collect()
}

/// Whether a URL refers to another host: it begins with `//`, or has a
/// scheme other than `data:`.
fn is_external(u: &str) -> bool {
    let u = u.trim();
    u.starts_with("//")
        || (url::has_scheme(u)
            && !u
                .split(':')
                .next()
                .is_some_and(|s| s.eq_ignore_ascii_case("data")))
}

/// The URLs of `@import` (a string or `url(…)`) and of every other
/// `url(…)`, skipping comments and other strings.
fn urls(css: &str) -> Vec<String> {
    let b = css.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    // Whether the last token was `@import`, whose string is a URL.
    let mut import = false;
    while i < b.len() {
        let c = b[i];
        if b[i..].starts_with(b"/*") {
            i = css[i + 2..].find("*/").map_or(b.len(), |k| i + 2 + k + 2);
            continue;
        }
        if c == b'"' || c == b'\'' {
            let (text, end) = string(css, i);
            if import {
                out.push(text);
            }
            import = false;
            i = end;
            continue;
        }
        if c == b'@' && starts_with_ignore_case(&b[i..], b"@import") {
            import = true;
            i += 7;
            continue;
        }
        if starts_with_ignore_case(&b[i..], b"url(") && (i == 0 || !is_name_byte(b[i - 1])) {
            let (text, end) = url_function(css, i + 4);
            out.push(text);
            import = false;
            i = end;
            continue;
        }
        if !c.is_ascii_whitespace() {
            import = false;
        }
        i += 1;
    }
    out
}

/// The string starting with the quote at `start`, with its escapes taken,
/// and the index after it. A string ends at its quote, a line feed or the
/// end of the CSS.
fn string(css: &str, start: usize) -> (String, usize) {
    let quote = css.as_bytes()[start] as char;
    let mut text = String::new();
    let mut chars = css[start + 1..].char_indices();
    while let Some((k, c)) = chars.next() {
        match c {
            _ if c == quote => return (text, start + 1 + k + 1),
            '\n' => return (text, start + 1 + k),
            '\\' => {
                if let Some((_, e)) = chars.next() {
                    text.push(e);
                }
            }
            _ => text.push(c),
        }
    }
    (text, css.len())
}

/// The URL of a `url(` whose argument starts at `start`, and the index after
/// its `)`.
fn url_function(css: &str, start: usize) -> (String, usize) {
    let rest = &css[start..];
    let lead = rest.len() - rest.trim_start().len();
    let at = start + lead;
    if let Some(&q) = css.as_bytes().get(at)
        && (q == b'"' || q == b'\'')
    {
        let (text, end) = string(css, at);
        let close = css[end..].find(')').map_or(css.len(), |k| end + k + 1);
        return (text, close);
    }
    let mut text = String::new();
    let mut chars = css[at..].char_indices();
    while let Some((k, c)) = chars.next() {
        match c {
            ')' => return (text.trim_end().to_string(), at + k + 1),
            '\\' => {
                if let Some((_, e)) = chars.next() {
                    text.push(e);
                }
            }
            _ => text.push(c),
        }
    }
    (text.trim_end().to_string(), css.len())
}

fn starts_with_ignore_case(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// A byte that continues a CSS name, so that `myurl(` is not `url(`.
fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'\\' || b >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_what_refers_to_another_host() {
        let css = "@import url(https://a.org/x.css);\n@import \"https://b.org/y.css\";\n\
                   @import '_stil.css';\nh1 { background: url( //c.org/l.png ) }\n\
                   /* url(https://d.org) */ .p::before { content: \"url(https://e.org)\" }\n\
                   .q { background: url(data:image/png;base64,AA==) } .r { x: myurl(https://f.org) }\n\
                   .s { background: URL('https://g.org/a\\'b.png') }";
        assert_eq!(
            external_references(css),
            [
                "https://a.org/x.css",
                "https://b.org/y.css",
                "//c.org/l.png",
                "https://g.org/a'b.png"
            ]
        );
    }

    #[test]
    fn unclosed_parts_end_the_css() {
        for css in [
            "/* x",
            "\"abc",
            "url(",
            "url(\"x",
            "@import",
            "@import \"http://a.org",
            "\\",
        ] {
            let _ = external_references(css);
        }
        assert_eq!(
            external_references("@import \"http://a.org"),
            ["http://a.org"]
        );
    }
}
