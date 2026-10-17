//! The grammar shared by the element line (SPEC §5.4.1) and the inline long
//! form (§5.5.1): `.name#id.class[attr="value"](content)`.

use super::scan::Scan;

/// A parsed selector. Positions are indices into the `char` slice it was
/// read from.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Selector {
    pub name: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    /// Attributes from the square bracket, in the order written.
    pub attrs: Vec<(String, String)>,
    pub has_attr_bracket: bool,
    /// The range of the round bracket's content, without the brackets.
    pub content: Option<(usize, usize)>,
    /// The index after the selector.
    pub end: usize,
}

pub(crate) fn is_token_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

pub(crate) fn is_token(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_token_char)
}

/// Where the parts of a selector at `c[i] == '.'` are, up to `end`: found
/// without collecting them, so that a check at every position stays linear.
pub(crate) struct Shape {
    pub name: (usize, usize),
    chain: (usize, usize),
    attrs: Option<(usize, usize)>,
    pub content: Option<(usize, usize)>,
    pub end: usize,
}

impl Shape {
    pub(crate) fn name(&self, c: &[char]) -> String {
        c[self.name.0..self.name.1].iter().collect()
    }

    pub(crate) fn has_attr_bracket(&self) -> bool {
        self.attrs.is_some()
    }
}

/// The shape of a selector at `c[i] == '.'`, up to `end`.
pub(crate) fn shape(scan: &mut Scan, c: &[char], i: usize, end: usize) -> Option<Shape> {
    if c.get(i) != Some(&'.') {
        return None;
    }
    let mut j = i + 1;
    let start = j;
    if !(j < end && c[j].is_ascii_lowercase()) {
        return None;
    }
    while j < end && (c[j].is_ascii_lowercase() || c[j].is_ascii_digit()) {
        j += 1;
    }
    let name = (start, j);
    // `#id` and `.class`, in any order, the id at most once.
    let chain = (j, scan.chain_end(c, j, end));
    j = chain.1;
    let mut attrs = None;
    if j < end && c[j] == '[' {
        let next = scan.attrs_end(c, j, end)?;
        attrs = Some((j, next));
        j = next;
    }
    let mut content = None;
    if j < end && c[j] == '(' {
        let close = scan.paren_close(c, j + 1, end)?;
        content = Some((j + 1, close));
        j = close + 1;
    }
    Some(Shape {
        name,
        chain,
        attrs,
        content,
        end: j,
    })
}

/// The selector of a shape, collected.
pub(crate) fn build(c: &[char], shape: &Shape) -> Selector {
    let mut sel = Selector {
        name: shape.name(c),
        content: shape.content,
        end: shape.end,
        has_attr_bracket: shape.attrs.is_some(),
        ..Selector::default()
    };
    let (mut j, chain_end) = shape.chain;
    while j < chain_end {
        let t = j + 1;
        let mut k = t;
        while k < chain_end && is_token_char(c[k]) {
            k += 1;
        }
        let token: String = c[t..k].iter().collect();
        if c[j] == '#' {
            sel.id = Some(token);
        } else {
            sel.classes.push(token);
        }
        j = k;
    }
    if let Some((a, b)) = shape.attrs {
        sel.attrs = attributes(c, a, b)
            .map(|(attrs, _)| attrs)
            .unwrap_or_default();
    }
    sel
}

/// Reads a selector at `c[i] == '.'`, up to `end`.
pub(crate) fn selector(c: &[char], i: usize, end: usize) -> Option<Selector> {
    let shape = shape(&mut Scan::new(c), c, i, end)?;
    Some(build(c, &shape))
}

/// `[name="value" …]` at `c[i] == '['`. Inside a value `\"` is a quote and
/// `\\` a backslash.
fn attributes(c: &[char], i: usize, end: usize) -> Option<(Vec<(String, String)>, usize)> {
    let mut j = i + 1;
    let mut attrs = Vec::new();
    loop {
        while j < end && c[j] == ' ' {
            j += 1;
        }
        if j >= end {
            return None;
        }
        if c[j] == ']' {
            return Some((attrs, j + 1));
        }
        let n = j;
        while j < end && (c[j].is_ascii_alphanumeric() || matches!(c[j], '-' | '_' | ':')) {
            j += 1;
        }
        if j == n || c.get(j) != Some(&'=') || c.get(j + 1) != Some(&'"') {
            return None;
        }
        let name: String = c[n..j].iter().collect::<String>().to_ascii_lowercase();
        j += 2;
        let mut value = String::new();
        loop {
            if j >= end {
                return None;
            }
            match c[j] {
                '\\' if matches!(c.get(j + 1), Some('"' | '\\')) => {
                    value.push(c[j + 1]);
                    j += 2;
                }
                '"' => {
                    j += 1;
                    break;
                }
                ch => {
                    value.push(ch);
                    j += 1;
                }
            }
        }
        if !attrs.iter().any(|(n, _)| *n == name) {
            attrs.push((name, value));
        }
        if j < end && c[j] != ' ' && c[j] != ']' {
            return None;
        }
    }
}

/// `{#id .class name="value"}` (§6): None for anything else, a bare word
/// included.
pub(crate) fn brace_attributes(s: &str) -> Option<Vec<(String, String)>> {
    let inner = s.trim().strip_prefix('{')?.strip_suffix('}')?;
    let c: Vec<char> = inner.chars().collect();
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut classes: Vec<String> = Vec::new();
    let mut j = 0;
    loop {
        while j < c.len() && c[j] == ' ' {
            j += 1;
        }
        if j >= c.len() {
            break;
        }
        match c[j] {
            '#' | '.' => {
                let t = j + 1;
                let mut k = t;
                while k < c.len() && is_token_char(c[k]) {
                    k += 1;
                }
                if k == t {
                    return None;
                }
                let token: String = c[t..k].iter().collect();
                if c[j] == '#' {
                    attrs.retain(|(n, _)| n != "id");
                    attrs.push(("id".into(), token));
                } else {
                    classes.push(token);
                }
                j = k;
            }
            _ => {
                let mut bracket = vec!['['];
                bracket.extend(&c[j..]);
                bracket.push(']');
                // One name="value" pair at a time.
                let n = j;
                while j < c.len()
                    && (c[j].is_ascii_alphanumeric() || matches!(c[j], '-' | '_' | ':'))
                {
                    j += 1;
                }
                if j == n || c.get(j) != Some(&'=') || c.get(j + 1) != Some(&'"') {
                    return None;
                }
                let name: String = c[n..j].iter().collect::<String>().to_ascii_lowercase();
                j += 2;
                let mut value = String::new();
                loop {
                    match c.get(j) {
                        None => return None,
                        Some('\\') if matches!(c.get(j + 1), Some('"' | '\\')) => {
                            value.push(c[j + 1]);
                            j += 2;
                        }
                        Some('"') => {
                            j += 1;
                            break;
                        }
                        Some(&ch) => {
                            value.push(ch);
                            j += 1;
                        }
                    }
                }
                attrs.push((name, value));
            }
        }
    }
    if !classes.is_empty() {
        attrs.push(("class".into(), classes.join(" ")));
    }
    if attrs.is_empty() {
        return None;
    }
    Some(attrs)
}

/// Writes an attribute value for a square bracket.
pub(crate) fn quote_value(v: &str) -> String {
    let mut s = String::with_capacity(v.len() + 2);
    s.push('"');
    for ch in v.chars() {
        if ch == '"' || ch == '\\' {
            s.push('\\');
        }
        s.push(ch);
    }
    s.push('"');
    s
}
