//! Mode *article* (`docs/converters.md` §3.3, §3.4): the regions of a web
//! page that are not its content — navigation, sidebars, comments, share
//! bars — taken out before the page is converted, each kept for the report.
//!
//! Step 1 strips what declared meaning marks as not content. Step 2 finds
//! the main content, by `main` or `article` or else by a score after
//! Readability, strips what lies outside it and cleans it.

use std::collections::BTreeMap;

use lipupona::dom::{Element, Node};

/// A region taken out of the page: its label for the report (element and
/// start of its text) and its nodes.
pub struct Stripped {
    pub label: String,
    pub nodes: Vec<Node>,
}

/// The nodes of the page's body that stay, and the regions stripped from
/// it, in the order they were found.
pub fn strip(nodes: Vec<Node>) -> (Vec<Node>, Vec<Stripped>) {
    let mut out = Vec::new();
    let nodes = structural(nodes, false, &mut out);
    let (nodes, found) = main_content(nodes, &mut out);
    let nodes = match found {
        true => clean(nodes, &mut out),
        false => nodes,
    };
    (nodes, out)
}

// ------------------------------------------------------------------ step 1

/// Strips what declared meaning marks as not content (§3.3).
fn structural(nodes: Vec<Node>, in_article: bool, out: &mut Vec<Stripped>) -> Vec<Node> {
    let mut kept = Vec::with_capacity(nodes.len());
    for node in nodes {
        let Node::Element(mut el) = node else {
            kept.push(node);
            continue;
        };
        if let Some(mark) = structural_mark(&el, in_article) {
            take(out, Node::Element(el), &mark);
            continue;
        }
        let in_article = in_article || el.name == "article";
        el.children = structural(std::mem::take(&mut el.children), in_article, out);
        kept.push(Node::Element(el));
    }
    kept
}

/// Why `el` is not content, as the suffix of its label: `""` for its name
/// alone, `[role=…]` or `[hidden]`; None when it may be.
fn structural_mark(el: &Element, in_article: bool) -> Option<String> {
    match el.name.as_str() {
        "nav" | "aside" => return Some(String::new()),
        "header" | "footer" if !in_article => return Some(String::new()),
        _ => {}
    }
    if let Some(role) = el.attr("role") {
        let role = role.trim().to_ascii_lowercase();
        if [
            "navigation",
            "banner",
            "contentinfo",
            "complementary",
            "search",
        ]
        .contains(&role.as_str())
        {
            return Some(format!("[role={role}]"));
        }
    }
    if el.attr("hidden").is_some() {
        return Some("[hidden]".into());
    }
    if el
        .attr("aria-hidden")
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
    {
        return Some("[aria-hidden]".into());
    }
    // Some frameworks wrap the whole page in one form.
    if el.name == "form" && !holds(&el.children, &["main", "article", "h1"]) {
        return Some(String::new());
    }
    None
}

fn holds(nodes: &[Node], names: &[&str]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Element(e) => names.contains(&e.name.as_str()) || holds(&e.children, names),
        _ => false,
    })
}

// ------------------------------------------------------------------ step 2

/// Positive and negative words of a class or id (§3.4).
const POSITIVE: &[&str] = &[
    "article", "body", "content", "entry", "main", "page", "post", "text", "blog", "story",
];
const NEGATIVE: &[&str] = &[
    "ad",
    "banner",
    "comment",
    "comments",
    "footer",
    "menu",
    "nav",
    "newsletter",
    "promo",
    "related",
    "share",
    "sidebar",
    "social",
    "sponsor",
    "widget",
];

/// Elements that hold blocks: a `div` with none of them inside is a
/// paragraph for the score.
const BLOCKS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "details",
    "div",
    "dl",
    "fieldset",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "ul",
];

/// Elements that can be the main content.
const CANDIDATES: &[&str] = &["div", "section", "article", "main", "td", "blockquote"];

/// Elements that group a page rather than carry its text: as a sibling of
/// the main content they stay only by their score.
const CONTAINERS: &[&str] = &[
    "div", "section", "article", "main", "aside", "nav", "header", "footer", "form", "table",
];

/// Keeps the main content and the siblings that belong to it; everything
/// else is stripped as the largest regions that hold text. Whether there
/// was a main content to keep.
fn main_content(nodes: Vec<Node>, out: &mut Vec<Stripped>) -> (Vec<Node>, bool) {
    let root = Element {
        name: "body".into(),
        attrs: Vec::new(),
        children: nodes,
        span: 0..0,
        implied_end: false,
    };
    // Too little text to tell content from the rest.
    if text_lengths(&root.children, false).0 < 500 {
        return (root.children, false);
    }
    let (top, siblings) = match only(&root, "main").or_else(|| only(&root, "article")) {
        Some(path) => (path, Vec::new()),
        None => {
            let scores = scores(&root);
            let Some(top) = best(&root, &scores) else {
                return (root.children, false);
            };
            let keep = kept_siblings(&root, &top, &scores);
            (top, keep)
        }
    };
    if top.is_empty() {
        return (root.children, true);
    }
    (cut(root, &top, &siblings, out), true)
}

/// Down `path`, keeps the element it leads to and, beside it, the
/// `siblings`; strips everything else on the way.
fn cut(el: Element, path: &[usize], siblings: &[usize], out: &mut Vec<Stripped>) -> Vec<Node> {
    let Some((&index, rest)) = path.split_first() else {
        return vec![Node::Element(el)];
    };
    let mut kept = Vec::new();
    let mut next = None;
    for (i, child) in el.children.into_iter().enumerate() {
        if i == index && rest.is_empty() {
            kept.push(child);
        } else if i == index {
            next = Some(child);
        } else if rest.is_empty() && siblings.contains(&i) {
            kept.push(child);
        } else {
            take(out, child, "");
        }
    }
    match next {
        Some(Node::Element(e)) => cut(e, rest, siblings, out),
        _ => kept,
    }
}

/// The path to the only element named `name`, if there is exactly one.
fn only(root: &Element, name: &str) -> Option<Vec<usize>> {
    fn find(el: &Element, name: &str, path: &mut Vec<usize>, found: &mut Vec<Vec<usize>>) {
        for (i, child) in el.children.iter().enumerate() {
            if let Node::Element(e) = child {
                path.push(i);
                if e.name == name {
                    found.push(path.clone());
                }
                find(e, name, path, found);
                path.pop();
            }
        }
    }
    let mut found = Vec::new();
    find(root, name, &mut Vec::new(), &mut found);
    match found.len() {
        1 => found.pop(),
        _ => None,
    }
}

/// The final score of every candidate that a paragraph gave to, by path
/// (in document order). The body is the candidate with the empty path.
fn scores(root: &Element) -> BTreeMap<Vec<usize>, f64> {
    let mut raw: BTreeMap<Vec<usize>, f64> = BTreeMap::new();
    let mut paragraphs = Vec::new();
    collect_paragraphs(root, &mut Vec::new(), &mut paragraphs);
    for (path, score) in paragraphs {
        let candidates = (0..path.len())
            .rev()
            .map(|n| &path[..n])
            .filter(|p| p.is_empty() || CANDIDATES.contains(&get(root, p).name.as_str()));
        for (at, share) in candidates.zip([1.0, 0.5]) {
            let entry = raw
                .entry(at.to_vec())
                .or_insert_with(|| start_score(get(root, at)));
            *entry += score * share;
        }
    }
    raw.into_iter()
        .map(|(path, s)| {
            let (all, linked) = text_lengths(&get(root, &path).children, false);
            let density = if all == 0 {
                0.0
            } else {
                linked as f64 / all as f64
            };
            (path, s * (1.0 - density))
        })
        .collect()
}

/// The main content by score: the best candidate; the nearest ancestor
/// holding three others that score at least three quarters of it; and up
/// while it is the only element in a candidate.
fn best(root: &Element, scores: &BTreeMap<Vec<usize>, f64>) -> Option<Vec<usize>> {
    let mut best: Option<(&Vec<usize>, f64)> = None;
    for (path, &score) in scores {
        if best.is_none_or(|(_, b)| score > b) {
            best = Some((path, score));
        }
    }
    let (top, score) = best?;
    let close: Vec<&Vec<usize>> = scores
        .iter()
        .filter(|(p, s)| *p != top && **s >= score * 0.75)
        .map(|(p, _)| p)
        .collect();
    let mut top = top.clone();
    if close.len() >= 3 {
        for n in (0..top.len()).rev() {
            if close.iter().filter(|p| p.starts_with(&top[..n])).count() >= 3 {
                top.truncate(n);
                break;
            }
        }
    }
    while let Some((_, parent)) = top.split_last() {
        let el = get(root, parent);
        let alone = el.children.iter().filter(|n| !n.is_blank_text()).count() == 1;
        if !alone || !(parent.is_empty() || CANDIDATES.contains(&el.name.as_str())) {
            break;
        }
        top.pop();
    }
    Some(top)
}

/// Paragraph-like blocks with at least 25 characters, and their scores.
fn collect_paragraphs(el: &Element, path: &mut Vec<usize>, out: &mut Vec<(Vec<usize>, f64)>) {
    for (i, child) in el.children.iter().enumerate() {
        let Node::Element(e) = child else {
            continue;
        };
        path.push(i);
        if is_paragraph(e) {
            let text = text_of(&e.children);
            let len = text.chars().count();
            if len >= 25 {
                let commas = text.matches([',', '，']).count() as f64;
                out.push((
                    path.clone(),
                    1.0 + commas + (len as f64 / 100.0).floor().min(3.0),
                ));
            }
        } else {
            collect_paragraphs(e, path, out);
        }
        path.pop();
    }
}

fn is_paragraph(e: &Element) -> bool {
    match e.name.as_str() {
        "p" | "pre" => true,
        "td" | "blockquote" | "div" => !holds(&e.children, BLOCKS),
        _ => false,
    }
}

fn start_score(el: &Element) -> f64 {
    let kind = match el.name.as_str() {
        "div" => 5.0,
        "td" | "blockquote" => 3.0,
        _ => 0.0,
    };
    kind + weight(el)
}

/// +25 for a positive class or id, −25 for a negative one.
fn weight(el: &Element) -> f64 {
    let (positive, negative) = names(el);
    f64::from(i8::from(positive) - i8::from(negative)) * 25.0
}

/// Whether a word of the class or id is positive, and whether one is
/// negative.
fn names(el: &Element) -> (bool, bool) {
    let mut words = String::new();
    for name in ["class", "id"] {
        if let Some(v) = el.attr(name) {
            words.push(' ');
            words.push_str(&v.to_ascii_lowercase());
        }
    }
    let words: Vec<&str> = words
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter(|w| !w.is_empty())
        .collect();
    (
        words.iter().any(|w| POSITIVE.contains(w)),
        words.iter().any(|w| NEGATIVE.contains(w)),
    )
}

/// The indices of the siblings of the main content that belong to it: a
/// container by its score, anything else unless it is marked negative or
/// made of links.
fn kept_siblings(root: &Element, top: &[usize], scores: &BTreeMap<Vec<usize>, f64>) -> Vec<usize> {
    let Some((&index, parent_path)) = top.split_last() else {
        return Vec::new();
    };
    let best = scores.get(top).copied().unwrap_or(0.0);
    let parent = get(root, parent_path);
    let mut keep = Vec::new();
    for (i, child) in parent.children.iter().enumerate() {
        let Node::Element(e) = child else {
            continue;
        };
        if i == index {
            continue;
        }
        let keeps = if CONTAINERS.contains(&e.name.as_str()) {
            let mut path = parent_path.to_vec();
            path.push(i);
            scores
                .get(&path)
                .is_some_and(|&s| s >= (best * 0.2).max(10.0))
        } else {
            let (_, negative) = names(e);
            let (all, linked) = text_lengths(&e.children, e.name == "a");
            !negative && linked * 2 < all.max(1)
        };
        if keeps {
            keep.push(i);
        }
    }
    keep
}

fn get<'a>(root: &'a Element, path: &[usize]) -> &'a Element {
    let mut el = root;
    for &i in path {
        match el.children.get(i) {
            Some(Node::Element(e)) => el = e,
            _ => break,
        }
    }
    el
}

// ---------------------------------------------------------------- cleaning

/// Strips, inside the main content, containers that are marked or made of
/// links; tables and paragraphs stay.
fn clean(nodes: Vec<Node>, out: &mut Vec<Stripped>) -> Vec<Node> {
    let mut kept = Vec::with_capacity(nodes.len());
    for node in nodes {
        let Node::Element(mut el) = node else {
            kept.push(node);
            continue;
        };
        if matches!(el.name.as_str(), "div" | "section" | "ul" | "ol" | "form") {
            let (positive, negative) = names(&el);
            let (all, linked) = text_lengths(&el.children, false);
            let links = count(&el.children, "a");
            if (negative && !positive) || (links >= 3 && linked * 2 > all) {
                take(out, Node::Element(el), "");
                continue;
            }
        }
        el.children = clean(std::mem::take(&mut el.children), out);
        kept.push(Node::Element(el));
    }
    kept
}

fn count(nodes: &[Node], name: &str) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            Node::Element(e) => usize::from(e.name == name) + count(&e.children, name),
            _ => 0,
        })
        .sum()
}

// ------------------------------------------------------------------- text

/// Strips `node` into `out`, labelled by its element and the start of its
/// text, with `mark` after the element. A region without text or image
/// goes without an entry.
fn take(out: &mut Vec<Stripped>, node: Node, mark: &str) {
    let text = text_of(std::slice::from_ref(&node));
    let label = match &node {
        Node::Element(el) => {
            if text.is_empty() && count(&el.children, "img") == 0 && el.name != "img" {
                return;
            }
            selector(el) + mark
        }
        Node::Text(_) if !text.is_empty() => "text".to_string(),
        _ => return,
    };
    let preview: String = text.chars().take(60).collect();
    let label = match (preview.is_empty(), preview.len() < text.len()) {
        (true, _) => label,
        (false, false) => format!("{label}: {preview}"),
        (false, true) => format!("{label}: {preview}…"),
    };
    out.push(Stripped {
        label,
        nodes: vec![node],
    });
}

/// `div#comments`, `div.share`, `nav`.
fn selector(el: &Element) -> String {
    if let Some(id) = el.attr("id").map(str::trim).filter(|v| !v.is_empty()) {
        return format!("{}#{id}", el.name);
    }
    match el.classes().next() {
        Some(class) => format!("{}.{class}", el.name),
        None => el.name.clone(),
    }
}

/// The text of `nodes` with runs of whitespace as one space, trimmed;
/// what is never content left out.
fn text_of(nodes: &[Node]) -> String {
    let mut raw = String::new();
    gather(nodes, &mut raw);
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn gather(nodes: &[Node], out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(&t.text),
            Node::Element(e) if !never(e) => {
                out.push(' ');
                gather(&e.children, out);
                out.push(' ');
            }
            _ => {}
        }
    }
}

fn never(e: &Element) -> bool {
    matches!(
        e.name.as_str(),
        "script" | "style" | "noscript" | "template"
    )
}

/// Characters of text that is not whitespace, in all and inside links.
fn text_lengths(nodes: &[Node], in_link: bool) -> (usize, usize) {
    let mut all = 0;
    let mut linked = 0;
    for node in nodes {
        match node {
            Node::Text(t) => {
                let n = t.text.chars().filter(|c| !c.is_whitespace()).count();
                all += n;
                if in_link {
                    linked += n;
                }
            }
            Node::Element(e) if !never(e) => {
                let (a, l) = text_lengths(&e.children, in_link || e.name == "a");
                all += a;
                linked += l;
            }
            _ => {}
        }
    }
    (all, linked)
}
