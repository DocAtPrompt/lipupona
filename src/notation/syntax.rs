//! The grammar shared by the element line (SPEC §5.4.1) and the inline long
//! form (§5.5.1): `.name#id.class[attr="value"](content)`.

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

/// Reads a selector at `c[i] == '.'`, up to `end`.
pub(crate) fn selector(c: &[char], i: usize, end: usize) -> Option<Selector> {
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
    let mut sel = Selector {
        name: c[start..j].iter().collect(),
        ..Selector::default()
    };
    // `#id` and `.class`, in any order, the id at most once.
    while j < end && (c[j] == '#' || c[j] == '.') {
        let mark = c[j];
        let t = j + 1;
        let mut k = t;
        while k < end && is_token_char(c[k]) {
            k += 1;
        }
        if k == t || (mark == '#' && sel.id.is_some()) {
            break;
        }
        let token: String = c[t..k].iter().collect();
        if mark == '#' {
            sel.id = Some(token);
        } else {
            sel.classes.push(token);
        }
        j = k;
    }
    if j < end && c[j] == '[' {
        let (attrs, next) = attributes(c, j, end)?;
        sel.attrs = attrs;
        sel.has_attr_bracket = true;
        j = next;
    }
    if j < end && c[j] == '(' {
        let close = closing_paren(c, j + 1, end)?;
        sel.content = Some((j + 1, close));
        j = close + 1;
    }
    sel.end = j;
    Some(sel)
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

/// The index after a comment at `c[i]`, if one begins there and is closed.
pub(crate) fn skip_comment(c: &[char], i: usize, end: usize) -> Option<usize> {
    let open = ['<', '!', '-', '-'];
    if i + 4 > end || c[i..i + 4] != open {
        return None;
    }
    (i + 4..end.saturating_sub(2))
        .find(|&k| c[k..k + 3] == ['-', '-', '>'])
        .map(|k| k + 3)
}

/// The index of the `)` that closes a round bracket whose content begins at
/// `from`: brackets nest, `\(` and `\)` do not count, code spans and
/// comments are skipped.
pub(crate) fn closing_paren(c: &[char], from: usize, end: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut j = from;
    while j < end {
        if let Some(next) = skip_comment(c, j, end) {
            j = next;
            continue;
        }
        match c[j] {
            '\\' if j + 1 < end && c[j + 1].is_ascii_punctuation() => j += 1,
            '`' => {
                if let Some((_, next)) = code_span(c, j, end) {
                    j = next;
                    continue;
                }
                while j + 1 < end && c[j + 1] == '`' {
                    j += 1;
                }
            }
            '(' => depth += 1,
            ')' if depth == 0 => return Some(j),
            ')' => depth -= 1,
            _ => {}
        }
        j += 1;
    }
    None
}

/// A code span at `c[i] == '`'` (§5.5.3): its content and the index after
/// it, or None when no run of the same length closes it.
pub(crate) fn code_span(c: &[char], i: usize, end: usize) -> Option<(String, usize)> {
    let run = |k: usize| c[k..end].iter().take_while(|&&x| x == '`').count();
    let n = run(i);
    let mut j = i + n;
    while j < end {
        if c[j] == '`' {
            let m = run(j);
            if m == n {
                let mut content: String = c[i + n..j].iter().collect();
                if content.len() >= 2
                    && content.starts_with(' ')
                    && content.ends_with(' ')
                    && !content.chars().all(|x| x == ' ')
                {
                    content = content[1..content.len() - 1].to_string();
                }
                return Some((content, j + n));
            }
            j += m;
        } else {
            j += 1;
        }
    }
    None
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
