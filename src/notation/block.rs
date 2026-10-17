//! Reading notation: the line model (SPEC §5.2) and every block construct
//! of §5 and §6.

use super::inline::{self, Inline, element, from_selector, text};
use super::syntax;
use super::table;
use crate::diag::{Code, Diagnostic};
use crate::dom::{Attr, Element, Node};
use crate::html;
use crate::profile::{Kind, Model, Profile, Weight};

pub(crate) struct Reader<'p> {
    pub profile: &'p Profile,
    pub diags: Vec<Diagnostic>,
}

/// What a line opens (§5.2), seen without its indentation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Opener {
    Fence {
        ch: char,
        len: usize,
        info: String,
    },
    Comment,
    TypedBlock,
    Container {
        len: usize,
        spec: String,
    },
    AttrLine(Vec<(String, String)>),
    Heading {
        level: usize,
        text: String,
    },
    Hr,
    Quote,
    Grid,
    CellTable,
    Item {
        number: Option<u64>,
        content: String,
    },
    ElementLine(syntax::Selector),
}

/// The whitespace of indentation and of line ends in notation. Other
/// whitespace — a no-break space — is content.
pub(crate) const SP: [char; 2] = [' ', '\t'];

/// Width of the indentation, a tab counting as one level (two spaces).
pub(crate) fn indent_width(line: &str) -> usize {
    let mut w = 0;
    for ch in line.chars() {
        match ch {
            ' ' => w += 1,
            '\t' => w += 2,
            _ => break,
        }
    }
    w
}

/// Removes up to `width` columns of indentation.
pub(crate) fn dedent(line: &str, width: usize) -> String {
    let mut w = 0;
    let mut cut = 0;
    for (i, ch) in line.char_indices() {
        if w >= width {
            break;
        }
        match ch {
            ' ' => w += 1,
            '\t' => w += 2,
            _ => break,
        }
        cut = i + ch.len_utf8();
    }
    line[cut..].to_string()
}

pub(crate) fn is_blank(line: &str) -> bool {
    line.trim_matches(SP).is_empty()
}

/// What `rest` (a line without indentation) opens; None for a text line.
/// `figure` admits an `img` element line (§5.4.4).
pub(crate) fn opener(profile: &Profile, rest: &str, figure: bool) -> Option<Opener> {
    let c: Vec<char> = rest.chars().collect();
    let first = *c.first()?;
    let run = |ch: char| c.iter().take_while(|&&x| x == ch).count();
    match first {
        '`' | '~' if run(first) >= 3 => {
            let len = run(first);
            let info: String = c[len..]
                .iter()
                .collect::<String>()
                .trim_matches(SP)
                .to_string();
            if first == '`' && info.contains('`') {
                return None;
            }
            Some(Opener::Fence {
                ch: first,
                len,
                info,
            })
        }
        // A comment is a block when nothing follows it on its last line.
        '<' if rest.starts_with("<!--") => match rest[4..].find("-->") {
            Some(k) => rest[4 + k + 3..]
                .trim_matches(SP)
                .is_empty()
                .then_some(Opener::Comment),
            None => Some(Opener::Comment),
        },
        '<' => {
            let name: String = c[1..]
                .iter()
                .take_while(|x| x.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            let after = c.get(1 + name.len()).copied();
            let boundary = after.is_none_or(|x| x == '>' || x == '/' || x.is_whitespace());
            (!name.is_empty() && boundary && profile.kind(&name) == Some(Kind::Block))
                .then_some(Opener::TypedBlock)
        }
        ':' if run(':') >= 3 => {
            let len = run(':');
            Some(Opener::Container {
                len,
                spec: c[len..]
                    .iter()
                    .collect::<String>()
                    .trim_matches(SP)
                    .to_string(),
            })
        }
        '{' => syntax::brace_attributes(rest).map(Opener::AttrLine),
        '#' => {
            let level = run('#');
            match c.get(level) {
                Some(' ') => Some(Opener::Heading {
                    level,
                    text: c[level + 1..]
                        .iter()
                        .collect::<String>()
                        .trim_matches(SP)
                        .to_string(),
                }),
                _ => None,
            }
        }
        '-' if rest.trim_end_matches(SP) == "---" => Some(Opener::Hr),
        '-' if c.len() == 1 || c.get(1) == Some(&' ') => Some(Opener::Item {
            number: None,
            content: c.get(2..).map(|r| r.iter().collect()).unwrap_or_default(),
        }),
        '>' => Some(Opener::Quote),
        '|' if rest.starts_with("|===") => {
            let count = rest[4..].trim_matches(SP);
            (!count.is_empty() && count.bytes().all(|b| b.is_ascii_digit()))
                .then_some(Opener::CellTable)
        }
        '|' => Some(Opener::Grid),
        '0'..='9' => {
            let digits = c.iter().take_while(|x| x.is_ascii_digit()).count();
            if c.get(digits) != Some(&'.') {
                return None;
            }
            match c.get(digits + 1) {
                None | Some(' ') => {
                    let number: String = c[..digits].iter().collect();
                    Some(Opener::Item {
                        number: Some(number.parse().unwrap_or(u64::MAX)),
                        content: c
                            .get(digits + 2..)
                            .map(|r| r.iter().collect())
                            .unwrap_or_default(),
                    })
                }
                _ => None,
            }
        }
        '.' => {
            let sel = syntax::selector(&c, 0, c.len())?;
            let tail: String = c[sel.end..].iter().collect();
            if !tail.trim_matches(SP).is_empty() {
                return None;
            }
            let (kind, model) = profile.element(&sel.name)?;
            if model == Model::Void && sel.content.is_some_and(|(a, b)| a != b) {
                return None;
            }
            match kind {
                Kind::Block => Some(Opener::ElementLine(sel)),
                Kind::Inline if figure && sel.name == "img" => Some(Opener::ElementLine(sel)),
                Kind::Inline => None,
            }
        }
        _ => None,
    }
}

impl<'p> Reader<'p> {
    pub(crate) fn new(profile: &'p Profile) -> Self {
        Reader {
            profile,
            diags: Vec::new(),
        }
    }

    fn phrasing(&mut self, s: &str) -> Vec<Node> {
        let mut r = Inline::new(self.profile);
        let nodes = inline::unwrap_fragments(r.read(s));
        self.diags.append(&mut r.diags);
        nodes
    }

    fn paragraph(&mut self, lines: &[String]) -> Node {
        let text: Vec<&str> = lines.iter().map(|l| l.trim_start_matches(SP)).collect();
        element("p", Vec::new(), self.phrasing(&text.join("\n")))
    }

    /// Reads the lines of one context, already without its indentation.
    pub(crate) fn blocks(&mut self, input: &[String], figure: bool) -> Vec<Node> {
        let mut lines: Vec<String> = input.to_vec();
        let mut out: Vec<Node> = Vec::new();
        let mut para: Vec<String> = Vec::new();
        let mut pending: Option<Vec<(String, String)>> = None;
        let mut i = 0;
        while i < lines.len() {
            if is_blank(&lines[i]) {
                self.flush(&mut para, &mut out, &mut pending);
                i += 1;
                continue;
            }
            let rest = lines[i].trim_start_matches(SP).to_string();
            let op = opener(self.profile, &rest, figure);
            let Some(op) = op else {
                para.push(rest);
                i += 1;
                continue;
            };
            if let Opener::AttrLine(attrs) = &op {
                let next_is_block = lines.get(i + 1).is_some_and(|l| !is_blank(l));
                if para.is_empty() && next_is_block {
                    let mut merged = pending.take().unwrap_or_default();
                    merged.extend(attrs.iter().cloned());
                    pending = Some(merged);
                    i += 1;
                    continue;
                }
                para.push(rest);
                i += 1;
                continue;
            }
            // What only looks like a comment or typed HTML block is text.
            let taken = match &op {
                Opener::Comment => self.block_comment(&lines, i).is_some(),
                Opener::TypedBlock => self.typed_block(&lines, i).is_some(),
                Opener::Container { spec, .. } => self.container(spec).is_some(),
                _ => true,
            };
            if !taken {
                para.push(rest);
                i += 1;
                continue;
            }
            let caption = if matches!(op, Opener::Grid | Opener::CellTable) && para.len() == 1 {
                para.pop()
            } else {
                self.flush(&mut para, &mut out, &mut pending);
                None
            };
            let before = out.len();
            let line_before = i;
            match op {
                Opener::Fence { ch, len, info } => {
                    let close = (i + 1..lines.len()).find(|&j| {
                        let t = lines[j].trim_matches(SP);
                        t.chars().count() >= len && t.chars().all(|x| x == ch)
                    });
                    let end = close.unwrap_or(lines.len());
                    let content = lines[i + 1..end].join("\n");
                    i = close.map_or(end, |j| j + 1);
                    if info == "{=html}" || info == "=html" {
                        let nodes = html::parse(&content);
                        let nodes = self.profile.check(&content, nodes, &mut self.diags);
                        out.extend(nodes.into_iter().filter(|n| !n.is_blank_text()));
                    } else {
                        let mut attrs = Vec::new();
                        if !info.is_empty() {
                            attrs.push(Attr {
                                name: "class".into(),
                                value: info.split_whitespace().collect::<Vec<_>>().join(" "),
                            });
                        }
                        let children = if content.is_empty() {
                            Vec::new()
                        } else {
                            vec![text(content)]
                        };
                        out.push(element("pre", attrs, children));
                    }
                }
                Opener::Comment => {
                    if let Some((inner, used)) = self.block_comment(&lines, i) {
                        self.diags.push(Diagnostic::new(
                            Code::Comment,
                            inner.trim_matches(SP).to_string(),
                        ));
                        out.push(Node::Comment(crate::dom::Comment {
                            text: inner,
                            span: 0..0,
                        }));
                        i += used;
                    }
                }
                Opener::TypedBlock => {
                    if let Some((nodes, used, remainder)) = self.typed_block(&lines, i) {
                        out.extend(nodes);
                        i += used;
                        match remainder {
                            Some(r) => lines[i] = r,
                            None => i += 1,
                        }
                    }
                }
                Opener::Container { len, spec } => {
                    let mut stack: Vec<usize> = Vec::new();
                    let mut close = None;
                    for (j, line) in lines.iter().enumerate().skip(i + 1) {
                        let t = line.trim_matches(SP);
                        let m = t.chars().take_while(|&x| x == ':').count();
                        if m < 3 {
                            continue;
                        }
                        if t[m..].trim_matches(SP).is_empty() {
                            if stack.last() == Some(&m) {
                                stack.pop();
                            } else if stack.is_empty() && m == len {
                                close = Some(j);
                                break;
                            }
                        } else {
                            stack.push(m);
                        }
                    }
                    let end = close.unwrap_or(lines.len());
                    let content: Vec<String> = lines[i + 1..end].to_vec();
                    i = close.map_or(end, |j| j + 1);
                    if let Some((name, attrs)) = self.container(&spec) {
                        let children = self.content(&name, None, &content);
                        out.push(element(&name, attrs, children));
                    }
                }
                Opener::AttrLine(_) => {
                    para.push(rest);
                    i += 1;
                }
                Opener::Heading { level, text: t } => {
                    let (t, attrs) = heading_braces(&t);
                    let level = if level > 4 {
                        self.diags.push(Diagnostic::new(
                            Code::HeadingLevelReduced,
                            format!("h{level} reduced to h4"),
                        ));
                        4
                    } else {
                        level
                    };
                    let children = self.phrasing(&t);
                    out.push(element(&format!("h{level}"), attrs, children));
                    i += 1;
                }
                Opener::Hr => {
                    out.push(element("hr", Vec::new(), Vec::new()));
                    i += 1;
                }
                Opener::Quote => {
                    let mut inner = Vec::new();
                    while i < lines.len() && lines[i].trim_start_matches(SP).starts_with('>') {
                        let l = &lines[i].trim_start_matches(SP)[1..];
                        inner.push(l.strip_prefix(' ').unwrap_or(l).to_string());
                        i += 1;
                    }
                    out.push(self.quote(&inner));
                }
                Opener::Grid => {
                    let (node, next) = table::grid(self, &lines, i, caption);
                    out.push(node);
                    i = next;
                }
                Opener::CellTable => {
                    let (node, next) = table::cells(self, &lines, i, caption);
                    out.push(node);
                    i = next;
                }
                Opener::Item { number, .. } => {
                    let (node, next) = self.list(&lines, i, number.is_some());
                    out.push(node);
                    i = next;
                }
                Opener::ElementLine(sel) => {
                    let mut j = i + 1;
                    while j < lines.len() && (is_blank(&lines[j]) || indent_width(&lines[j]) >= 2) {
                        j += 1;
                    }
                    while j > i + 1 && is_blank(&lines[j - 1]) {
                        j -= 1;
                    }
                    let content: Vec<String> =
                        lines[i + 1..j].iter().map(|l| dedent(l, 2)).collect();
                    i = j;
                    let (name, attrs) = from_selector(&sel);
                    let bracket: Option<String> = sel
                        .content
                        .map(|(a, b)| rest.chars().skip(a).take(b - a).collect());
                    let children = self.content(&name, bracket, &content);
                    out.push(element(&name, attrs, children));
                }
            }
            if let Some(attrs) = pending.take()
                && let Some(Node::Element(e)) = out.get_mut(before)
            {
                self.merge(e, attrs);
            }
            // Every line is read once: a construct that took nothing leaves
            // its line as text.
            if i == line_before {
                out.truncate(before);
                para.push(lines[i].trim_start_matches(SP).to_string());
                i += 1;
            }
        }
        self.flush(&mut para, &mut out, &mut pending);
        out
    }

    /// A comment block at `lines[i]`: its text and the lines it takes, when
    /// it is closed and nothing follows it on its last line.
    fn block_comment(&self, lines: &[String], i: usize) -> Option<(String, usize)> {
        let joined = lines[i..].join("\n");
        let body = joined.trim_start_matches(SP).strip_prefix("<!--")?;
        let k = body.find("-->")?;
        let tail = body[k + 3..].split('\n').next().unwrap_or("");
        if !tail.trim_matches(SP).is_empty() {
            return None;
        }
        let inner = body[..k].to_string();
        let used = inner.matches('\n').count() + 1;
        Some((inner, used))
    }

    /// A typed HTML block at `lines[i]` that is taken (§6): its nodes, the
    /// lines it takes up to its last, and what follows it on that line.
    fn typed_block(
        &self,
        lines: &[String],
        i: usize,
    ) -> Option<(Vec<Node>, usize, Option<String>)> {
        let joined = std::iter::once(lines[i].trim_start_matches(SP).to_string())
            .chain(lines[i + 1..].iter().cloned())
            .collect::<Vec<_>>()
            .join("\n");
        let len = html::fragment_len(&joined)?;
        let fragment = &joined[..len];
        let nodes = html::parse(fragment);
        if self
            .profile
            .diagnose(&nodes, false)
            .iter()
            .any(|d| d.code.is_blocking())
        {
            return None;
        }
        let nodes: Vec<Node> = nodes.into_iter().filter(|n| !n.is_blank_text()).collect();
        if nodes.iter().any(|n| matches!(n, Node::Text(_))) {
            return None;
        }
        let used = fragment.matches('\n').count();
        let remainder = joined[len..].split('\n').next().unwrap_or("").to_string();
        let remainder = (!remainder.trim_matches(SP).is_empty()).then_some(remainder);
        Some((nodes, used, remainder))
    }

    fn flush(
        &mut self,
        para: &mut Vec<String>,
        out: &mut Vec<Node>,
        pending: &mut Option<Vec<(String, String)>>,
    ) {
        if para.is_empty() {
            return;
        }
        let mut p = self.paragraph(para);
        para.clear();
        if let (Some(attrs), Node::Element(e)) = (pending.take(), &mut p) {
            self.merge(e, attrs);
        }
        out.push(p);
    }

    /// Attributes from braces merged into an element (§6): classes united,
    /// braces first; on a conflict the element line wins.
    fn merge(&mut self, e: &mut Element, braces: Vec<(String, String)>) {
        for (name, value) in braces {
            match e.attrs.iter_mut().find(|a| a.name == name) {
                Some(a) if name == "class" => {
                    a.value = format!("{value} {}", a.value);
                }
                Some(a) if a.value != value => {
                    self.diags.push(Diagnostic::new(
                        Code::AttributeConflict,
                        format!(
                            "{name}: \"{value}\" in braces, \"{}\" on the element",
                            a.value
                        ),
                    ));
                }
                Some(_) => {}
                None => e.attrs.push(Attr { name, value }),
            }
        }
    }

    /// The element of a fenced container's name: a block element of the
    /// profile, or else a class of a `div`, as Pandoc reads it.
    fn container(&self, spec: &str) -> Option<(String, Vec<Attr>)> {
        if spec.is_empty() {
            return Some(("div".into(), Vec::new()));
        }
        let c: Vec<char> = format!(".{spec}").chars().collect();
        let sel = syntax::selector(&c, 0, c.len())?;
        if sel.end != c.len() || sel.content.is_some() {
            return None;
        }
        let (name, mut attrs) = from_selector(&sel);
        if self.profile.kind(&name) == Some(Kind::Block) {
            return Some((name, attrs));
        }
        let mut classes = vec![name];
        if let Some(i) = attrs.iter().position(|a| a.name == "class") {
            classes.push(attrs.remove(i).value);
        }
        attrs.push(Attr {
            name: "class".into(),
            value: classes.join(" "),
        });
        Some(("div".into(), attrs))
    }

    /// The children of an element by its content model (§5.4.2, §5.4.3).
    fn content(&mut self, name: &str, bracket: Option<String>, lines: &[String]) -> Vec<Node> {
        let model = self.profile.model(name).unwrap_or(Model::Blocks);
        match model {
            Model::Void => Vec::new(),
            Model::Phrasing => match bracket {
                Some(b) => self.phrasing(&b),
                // Only the level's indentation is removed; the rest is
                // whitespace C7 collapses, or the text of a comment.
                None => self.phrasing(&lines.join("\n")),
            },
            Model::Text => {
                if lines.is_empty() {
                    Vec::new()
                } else {
                    vec![text(lines.join("\n"))]
                }
            }
            Model::Either => match bracket {
                Some(b) => self.phrasing(&b),
                None => self.blocks(lines, false),
            },
            Model::Lead => {
                let mut children = match bracket {
                    Some(b) => self.phrasing(&b),
                    None => Vec::new(),
                };
                children.extend(self.blocks(lines, false));
                children
            }
            _ => self.blocks(lines, name == "figure"),
        }
    }

    /// §5.7 and §7.3.
    fn quote(&mut self, inner: &[String]) -> Node {
        if let Some(first) = inner.first()
            && let Some((t, fold, title)) = callout_line(first)
        {
            return self.callout(&t, fold, title, &inner[1..]);
        }
        element("blockquote", Vec::new(), self.blocks(inner, false))
    }

    fn callout(
        &mut self,
        t: &str,
        fold: Option<char>,
        title: Option<String>,
        rest: &[String],
    ) -> Node {
        let weight = self.profile.weight(t);
        if weight.is_none() {
            self.diags.push(Diagnostic::new(
                Code::UnregisteredCallout,
                format!("callout type {t}"),
            ));
        }
        let class = Attr {
            name: "class".into(),
            value: format!("callout {t}"),
        };
        let mut children = Vec::new();
        let name = match fold {
            Some(_) => {
                let title = title
                    .filter(|s| !s.trim_matches(SP).is_empty())
                    .unwrap_or_else(|| capitalize(t));
                children.push(element("summary", Vec::new(), self.phrasing(&title)));
                "details"
            }
            None => {
                if let Some(title) = title.filter(|s| !s.trim_matches(SP).is_empty()) {
                    let title_class = Attr {
                        name: "class".into(),
                        value: "title".into(),
                    };
                    children.push(element("p", vec![title_class], self.phrasing(&title)));
                }
                if weight == Some(Weight::Core) {
                    "div"
                } else {
                    "aside"
                }
            }
        };
        children.extend(self.blocks(rest, false));
        let mut attrs = vec![class];
        if name == "div" {
            attrs.push(Attr {
                name: "role".into(),
                value: "note".into(),
            });
        }
        if fold == Some('+') {
            attrs.push(Attr {
                name: "open".into(),
                value: String::new(),
            });
        }
        element(name, attrs, children)
    }

    /// §5.6: a list at `lines[i]`.
    fn list(&mut self, lines: &[String], start: usize, ordered: bool) -> (Node, usize) {
        // Levels count from the list's own first line.
        let base = indent_width(&lines[start]);
        let item_at = |line: &str| -> Option<(Option<u64>, String)> {
            if indent_width(line) >= base + 2 {
                return None;
            }
            match opener(self.profile, line.trim_start_matches(SP), false) {
                Some(Opener::Item { number, content }) if number.is_some() == ordered => {
                    Some((number, content))
                }
                _ => None,
            }
        };
        let mut items: Vec<Vec<String>> = Vec::new();
        let mut first_number = None;
        let mut i = start;
        while let Some((number, content)) = lines.get(i).and_then(|l| item_at(l)) {
            if first_number.is_none() {
                first_number = number;
            }
            let mut body = vec![content];
            i += 1;
            while i < lines.len() {
                if is_blank(&lines[i]) {
                    let next = (i..lines.len()).find(|&k| !is_blank(&lines[k]));
                    match next {
                        Some(k) if indent_width(&lines[k]) >= base + 2 => {
                            body.extend(std::iter::repeat_n(String::new(), k - i));
                            i = k;
                        }
                        Some(k) if item_at(&lines[k]).is_some() => {
                            i = k;
                            break;
                        }
                        _ => break,
                    }
                } else if indent_width(&lines[i]) >= base + 2 {
                    body.push(dedent(&lines[i], base + 2));
                    i += 1;
                } else {
                    break;
                }
            }
            items.push(body);
        }
        let mut children = Vec::new();
        for body in items {
            let lead = !body[0].trim_matches(SP).is_empty()
                && opener(self.profile, body[0].trim_start_matches(SP), false).is_none();
            let mut nodes = self.blocks(&body, false);
            let leading_p = matches!(nodes.first(), Some(Node::Element(p)) if p.name == "p" && p.attrs.is_empty());
            if lead
                && leading_p
                && let Node::Element(p) = nodes.remove(0)
            {
                let mut lead_nodes = p.children;
                lead_nodes.extend(nodes);
                nodes = lead_nodes;
            }
            children.push(element("li", Vec::new(), nodes));
        }
        let mut attrs = Vec::new();
        if ordered && let Some(n) = first_number.filter(|&n| n != 1) {
            attrs.push(Attr {
                name: "start".into(),
                value: n.to_string(),
            });
        }
        // Blank lines after the last item belong to the context.
        while i > start && is_blank(&lines[i - 1]) {
            i -= 1;
        }
        (
            element(if ordered { "ol" } else { "ul" }, attrs, children),
            i,
        )
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// `[!type]`, `[!type]-`, `[!type]+`, then optionally a space and the title.
pub(crate) fn callout_line(line: &str) -> Option<(String, Option<char>, Option<String>)> {
    let rest = line.strip_prefix("[!")?;
    let close = rest.find(']')?;
    let t = &rest[..close];
    let mut chars = t.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return None;
    }
    let mut after = &rest[close + 1..];
    let fold = match after.chars().next() {
        Some(c @ ('-' | '+')) => {
            after = &after[1..];
            Some(c)
        }
        _ => None,
    };
    let title = match after {
        "" => None,
        s if s.starts_with(' ') => Some(s[1..].to_string()),
        _ => return None,
    };
    Some((t.to_ascii_lowercase(), fold, title))
}

/// `Titel {#anker .klasse}` at the end of a heading (§6).
fn heading_braces(t: &str) -> (String, Vec<Attr>) {
    if t.ends_with('}')
        && let Some(open) = t.rfind(" {")
        && let Some(attrs) = syntax::brace_attributes(&t[open + 1..])
    {
        let attrs = attrs
            .into_iter()
            .map(|(name, value)| Attr { name, value })
            .collect();
        return (t[..open].trim_end_matches(SP).to_string(), attrs);
    }
    (t.to_string(), Vec::new())
}

/// W2 at the start of a line: if the line would open a block, the index of
/// the character to put a backslash before.
pub(crate) fn opener_escape(profile: &Profile, line: &[char]) -> Option<usize> {
    let s: String = line.iter().collect();
    opener(profile, &s, false)?;
    line.iter().position(|c| c.is_ascii_punctuation())
}
