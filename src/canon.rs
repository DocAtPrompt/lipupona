//! Canonical serialization (SPEC §4).

use unicode_normalization::UnicodeNormalization;

use crate::diag::{Code, Diagnostic};
use crate::dom::{Attr, Element, Node, is_html_space};
use crate::profile::{Kind, Model, Profile};
use crate::{Output, html, url};

/// Reads HTML and writes it canonically. Blocks outside the profile and
/// islands come back as they were stored, with LF line ends (C2).
pub fn canon(input: &str, profile: &Profile) -> Output {
    let nodes = html::parse(input);
    let mut diagnostics = Vec::new();
    report_comments(&nodes, &mut diagnostics);
    let mut nodes = profile.check(input, nodes, &mut diagnostics);
    normalize(&mut nodes, profile);
    Output {
        text: serialize(&nodes, profile),
        diagnostics,
    }
}

fn report_comments(nodes: &[Node], out: &mut Vec<Diagnostic>) {
    for node in nodes {
        match node {
            Node::Comment(c) => out.push(Diagnostic::new(Code::Comment, c.text.trim().to_string())),
            Node::Element(e) => report_comments(&e.children, out),
            _ => {}
        }
    }
}

// ------------------------------------------------------------------ normalize

/// Brings a tree that is inside the profile into canonical shape: attributes
/// (C3, C10–C12), Unicode NFC (C3) and whitespace (C7).
pub fn normalize(nodes: &mut Vec<Node>, profile: &Profile) {
    for node in nodes.iter_mut() {
        normalize_values(node);
    }
    layout(nodes, profile);
}

fn normalize_values(node: &mut Node) {
    match node {
        Node::Text(t) => t.text = t.text.nfc().collect(),
        Node::Element(e) => {
            normalize_attrs(e);
            for c in e.children.iter_mut() {
                normalize_values(c);
            }
        }
        _ => {}
    }
}

fn normalize_attrs(el: &mut Element) {
    let mut attrs: Vec<Attr> = Vec::with_capacity(el.attrs.len());
    for a in el.attrs.drain(..) {
        let mut value: String = a.value.nfc().collect();
        match a.name.as_str() {
            "class" => {
                let mut classes: Vec<&str> = Vec::new();
                for c in value.split(is_html_space).filter(|c| !c.is_empty()) {
                    if !classes.contains(&c) {
                        classes.push(c);
                    }
                }
                if classes.is_empty() {
                    continue;
                }
                value = classes.join(" ");
            }
            "href" | "src" => value = url::canonical(&value),
            "open" => value.clear(),
            "start" if el.name == "ol" && value.trim().parse::<i64>() == Ok(1) => continue,
            _ => {}
        }
        attrs.push(Attr {
            name: a.name,
            value,
        });
    }
    attrs.sort_by(|a, b| {
        attr_rank(&a.name)
            .cmp(&attr_rank(&b.name))
            .then_with(|| a.name.cmp(&b.name))
    });
    el.attrs = attrs;
}

fn attr_rank(name: &str) -> u8 {
    match name {
        "id" => 0,
        "class" => 1,
        _ => 2,
    }
}

/// C7 for the children of one element (or of `body`).
fn layout(children: &mut Vec<Node>, profile: &Profile) {
    let block_layout = children.iter().any(|n| profile.is_block(n));
    let mut start = 0;
    while start < children.len() {
        if profile.is_block(&children[start]) {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < children.len() && !profile.is_block(&children[end]) {
            end += 1;
        }
        collapse_run(&mut children[start..end]);
        start = end;
    }
    if block_layout {
        children.retain(|n| !matches!(n, Node::Text(t) if t.text.is_empty()));
    }
    for child in children.iter_mut() {
        if let Node::Element(e) = child
            && profile.kind(&e.name) == Some(Kind::Block)
            && profile.model(&e.name) != Some(Model::Text)
        {
            layout(&mut e.children, profile);
        }
    }
}

/// Collapses whitespace in a run of phrasing, across the boundaries of inline
/// elements, and removes it at both ends of the run and next to `<br>`.
fn collapse_run(run: &mut [Node]) {
    let mut space = true;
    collapse_forward(run, &mut space);
    let mut trim = true;
    trim_backward(run, &mut trim);
}

fn collapse_forward(nodes: &mut [Node], space: &mut bool) {
    for node in nodes {
        match node {
            Node::Text(t) => {
                let mut out = String::with_capacity(t.text.len());
                for c in t.text.chars() {
                    if is_html_space(c) {
                        if !*space {
                            out.push(' ');
                            *space = true;
                        }
                    } else {
                        out.push(c);
                        *space = false;
                    }
                }
                t.text = out;
            }
            Node::Element(e) if e.name == "br" => *space = true,
            Node::Element(e) if e.children.is_empty() && is_void(&e.name) => *space = false,
            Node::Element(e) => collapse_forward(&mut e.children, space),
            Node::Comment(_) => {}
            Node::Verbatim(_) => *space = false,
        }
    }
}

fn trim_backward(nodes: &mut [Node], trim: &mut bool) {
    for node in nodes.iter_mut().rev() {
        match node {
            Node::Text(t) => {
                if *trim && t.text.ends_with(' ') {
                    t.text.pop();
                }
                if !t.text.is_empty() {
                    *trim = false;
                }
            }
            Node::Element(e) if e.name == "br" => *trim = true,
            Node::Element(e) if e.children.is_empty() && is_void(&e.name) => *trim = false,
            Node::Element(e) => trim_backward(&mut e.children, trim),
            Node::Comment(_) => {}
            Node::Verbatim(_) => *trim = false,
        }
    }
}

fn is_void(name: &str) -> bool {
    matches!(name, "br" | "img" | "hr")
}

// ------------------------------------------------------------------ serialize

pub fn serialize(nodes: &[Node], profile: &Profile) -> String {
    let mut lines = Vec::new();
    write_blocks(nodes, 0, &mut lines, profile);
    if lines.is_empty() {
        return String::new();
    }
    let mut out = lines.join("\n");
    out.push('\n');
    // A U+FEFF at the very start would read as a byte order mark (C1); it
    // can only stand there as text passed through, where a reference means
    // the same.
    if let Some(rest) = out.strip_prefix('\u{feff}') {
        out = format!("&#xFEFF;{rest}");
    }
    out
}

/// Children in block layout: every block, run of phrasing and comment
/// between blocks on a line of its own (C6), a blank line before a heading
/// that is not the first child (C9).
fn write_blocks(children: &[Node], level: usize, lines: &mut Vec<String>, profile: &Profile) {
    let indent = "  ".repeat(level);
    let mut first = true;
    let mut i = 0;
    while i < children.len() {
        let node = &children[i];
        if profile.is_block(node) {
            match node {
                Node::Element(e) => {
                    if !first && matches!(e.name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
                    {
                        lines.push(String::new());
                    }
                    write_block(e, level, lines, profile);
                }
                Node::Verbatim(v) => lines.push(format!("{indent}{}", v.text)),
                _ => {}
            }
            first = false;
            i += 1;
            continue;
        }
        let start = i;
        while i < children.len() && !profile.is_block(&children[i]) {
            i += 1;
        }
        let run = &children[start..i];
        let only_comments = run
            .iter()
            .all(|n| matches!(n, Node::Comment(_)) || n.is_blank_text());
        if only_comments {
            for n in run {
                if let Node::Comment(c) = n {
                    lines.push(format!("{indent}<!--{}-->", c.text));
                    first = false;
                }
            }
        } else {
            let mut line = indent.clone();
            write_phrasing(run, &mut line);
            lines.push(line);
            first = false;
        }
    }
}

fn write_block(el: &Element, level: usize, lines: &mut Vec<String>, profile: &Profile) {
    let indent = "  ".repeat(level);
    let mut line = format!("{indent}{}", start_tag(el));
    match profile.model(&el.name) {
        Some(Model::Void) => lines.push(line),
        Some(Model::Text) => {
            let mut text = String::new();
            for c in &el.children {
                if let Node::Text(t) = c {
                    text.push_str(&t.text);
                }
            }
            if text.starts_with('\n') {
                line.push('\n');
            }
            escape_text(&text, &mut line);
            line.push_str(&format!("</{}>", el.name));
            lines.push(line);
        }
        _ if el.children.iter().any(|n| profile.is_block(n)) => {
            lines.push(line);
            write_blocks(&el.children, level + 1, lines, profile);
            lines.push(format!("{indent}</{}>", el.name));
        }
        _ => {
            write_phrasing(&el.children, &mut line);
            line.push_str(&format!("</{}>", el.name));
            lines.push(line);
        }
    }
}

fn write_phrasing(nodes: &[Node], out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(t) => escape_text(&t.text, out),
            Node::Comment(c) => {
                out.push_str("<!--");
                out.push_str(&c.text);
                out.push_str("-->");
            }
            Node::Verbatim(v) => out.push_str(&v.text),
            Node::Element(e) => {
                out.push_str(&start_tag(e));
                if !is_void(&e.name) {
                    write_phrasing(&e.children, out);
                    out.push_str(&format!("</{}>", e.name));
                }
            }
        }
    }
}

fn start_tag(el: &Element) -> String {
    let mut s = format!("<{}", el.name);
    for a in &el.attrs {
        s.push(' ');
        s.push_str(&a.name);
        s.push_str("=\"");
        escape_attr(&a.value, &mut s);
        s.push('"');
    }
    s.push('>');
    s
}

/// C4: invisible characters as references.
fn invisible(c: char) -> Option<String> {
    match c {
        '\u{a0}' => Some("&nbsp;".into()),
        '\u{ad}' => Some("&shy;".into()),
        '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}' => {
            Some(format!("&#x{:04X};", c as u32))
        }
        _ => None,
    }
}

/// C5 in text: `&` and `<`.
pub fn escape_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            c => match invisible(c) {
                Some(r) => out.push_str(&r),
                None => out.push(c),
            },
        }
    }
}

/// C5 in attribute values: `&` and `"`.
pub fn escape_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            c => match invisible(c) {
                Some(r) => out.push_str(&r),
                None => out.push(c),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAGMENTS: &[&str] = &[
        "<p>",
        "</p>",
        "<ul>",
        "<li>",
        "</ul>",
        "<ol start=1>",
        "<table>",
        "<tr>",
        "<td>",
        "</table>",
        "<caption>",
        "<div>",
        "</div>",
        "<aside>",
        "<pre>",
        "</pre>",
        "<h2>",
        "</h2>",
        "<h5>",
        "<details>",
        "<summary>",
        "<blockquote>",
        "<figure>",
        "<figcaption>",
        "<template>",
        "<span class=\"a  a\">",
        "</span>",
        "<em>",
        "</strong>",
        "<code>",
        "<br>",
        "</br>",
        "<P>",
        "<img src=\"a b\" alt>",
        "<a href=\"%zz?x y#a#b\">",
        "</a>",
        "<svg data-test-rendered=x>",
        "</svg>",
        "<script>",
        "</script>",
        "<style>",
        "<!--",
        "-->",
        "<!doctype html>",
        "<head><title>t</title></head>",
        "<body>",
        "<ü>",
        "</ü>",
        "<",
        ">",
        "</",
        "&",
        "&amp;",
        "&#x",
        "&#0;",
        "&uuml",
        ";",
        "=",
        "'",
        "\"",
        "\n",
        "\r\n",
        "\r",
        "\t",
        " ",
        "\u{feff}",
        "\u{a0}",
        "u\u{308}",
        "ü",
        "text",
        "x",
        "<p class=",
    ];

    fn inputs() -> impl Iterator<Item = String> {
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        (0..5_000).map(move |_| {
            let len = (next() % 24) as usize;
            (0..len)
                .map(|_| FRAGMENTS[(next() % FRAGMENTS.len() as u64) as usize])
                .collect()
        })
    }

    #[test]
    fn never_panics_and_is_idempotent() {
        let profile = Profile::reference("test");
        for input in inputs() {
            let once = canon(&input, &profile).text;
            let twice = canon(&once, &profile).text;
            assert_eq!(once, twice, "L4 broken for input {input:?}");
        }
    }

    #[test]
    fn deep_nesting_is_passed_through() {
        let profile = Profile::reference("test");
        for tag in ["div", "span", "blockquote", "table", "li"] {
            let input = format!("<{tag}>").repeat(10_000);
            let out = canon(&input, &profile);
            assert!(
                out.diagnostics.iter().any(|d| d.code.is_blocking()),
                "{tag}"
            );
            assert_eq!(canon(&out.text, &profile).text, out.text, "{tag}");
            if tag != "li" {
                // One element inside the next: one block, passed through whole.
                assert_eq!(out.text, format!("{input}\n"), "{tag}");
            }
        }
    }

    #[test]
    fn deep_input_stays_linear() {
        let profile = Profile::reference("test");
        let input = format!("{}{}", "<div>".repeat(300), "x<div>".repeat(100_000));
        let out = canon(&input, &profile);
        assert_eq!(out.text, format!("{input}\n"));
    }

    #[test]
    fn spans_refer_to_the_input() {
        let input = "\u{feff}<p>a</p>\r\n<ul>\r\n<li>b\r\n</ul>";
        let nodes = html::parse(input);
        let ul = nodes.iter().filter_map(Node::element).nth(1).unwrap();
        assert_eq!(ul.source(input), "<ul>\r\n<li>b\r\n</ul>");
    }

    #[test]
    fn passed_through_blocks_get_lf() {
        let profile = Profile::reference("test");
        let out = canon("<section>\r\n<p>a</p>\r\n</section>\r\n", &profile);
        assert_eq!(out.text, "<section>\n<p>a</p>\n</section>\n");
    }
}
