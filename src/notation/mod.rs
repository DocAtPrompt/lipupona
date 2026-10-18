//! The notation (SPEC §5, §6): reading it into a tree and writing the
//! display of a tree.

mod block;
mod front;
mod inline;
mod scan;
mod syntax;
mod table;
mod track;
mod write;

use crate::profile::Profile;
use crate::{Output, canon};

/// Reads notation and returns the canonical HTML it means (`html` of the
/// specification), with what was reported on the way.
pub fn html(notation: &str, profile: &Profile) -> Output {
    let text = crate::html::lf(notation.strip_prefix('\u{feff}').unwrap_or(notation));
    let lines: Vec<&str> = text.split('\n').collect();
    // A front matter makes the notation a document (§5.15).
    let front = front::read(&lines);
    let body = &lines[front.as_ref().map_or(0, |f| f.used)..];
    let mut reader = block::Reader::new(profile);
    let nodes = reader.blocks(body, false);
    let (mut out, mut diagnostics) = match front {
        Some(f) => (canon::store_document(f.head, nodes, profile), f.diags),
        None => (canon::store(nodes, profile), Vec::new()),
    };
    diagnostics.append(&mut reader.diags);
    let too_deep = crate::html::too_deep();
    if diagnostics.contains(&too_deep) {
        out.diagnostics.retain(|d| *d != too_deep);
    }
    diagnostics.append(&mut out.diagnostics);
    out.diagnostics = diagnostics;
    out
}

/// What a span of notation is for an editor (SPEC §5.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// Characters that carry structure: `**`, `## `, indentation, an
    /// element line except its content, a link's path.
    Markup,
    /// Characters that become text of the document.
    Content,
    /// A line feed that is a `<br>`.
    Break,
    /// A line feed that ends a block, or a blank line.
    Separator,
    /// A non-breaking space, soft hyphen, zero-width or private-use
    /// character.
    Invisible,
}

/// A span of notation: its kind and its byte range in the notation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub kind: SpanKind,
    pub range: std::ops::Range<usize>,
}

/// The spans of notation as the reader sees it, for an editor to paint
/// every character once (SPEC §5.14): in order, without gaps or overlaps.
/// Runs of one kind are one span, except that every line feed is a span of
/// its own. A carriage return belongs to the line feed it stands before, a
/// byte order mark is invisible.
pub fn spans(notation: &str, profile: &Profile) -> Vec<Span> {
    let bom = if notation.starts_with('\u{feff}') {
        3
    } else {
        0
    };
    let body = &notation[bom..];
    // The text the reader reads (§5.2), and where each of its bytes was.
    let mut text = String::with_capacity(body.len());
    let mut origin = Vec::with_capacity(body.len() + 1);
    let mut chars = body.char_indices().peekable();
    while let Some((k, ch)) = chars.next() {
        if ch == '\r' {
            chars.next_if(|&(_, c)| c == '\n');
            text.push('\n');
            origin.push(bom + k);
            continue;
        }
        text.push(ch);
        origin.extend((0..ch.len_utf8()).map(|j| bom + k + j));
    }
    origin.push(notation.len());
    let lines: Vec<&str> = text.split('\n').collect();
    let mut reader = block::Reader::new(profile);
    reader.track = Some(track::Track::new(&text));
    let front = front::read(&lines);
    if let (Some(f), Some(t)) = (&front, &mut reader.track) {
        for v in &f.values {
            t.mark_slice(v, track::Mark::Content);
        }
    }
    reader.blocks(&lines[front.map_or(0, |f| f.used)..], false);
    let mut marks: Vec<Option<track::Mark>> = vec![None; text.len()];
    for (from, to, mark) in reader.track.map(|t| t.marks).unwrap_or_default() {
        for m in marks.iter_mut().take(to).skip(from) {
            *m = Some(mark);
        }
    }
    let mut out: Vec<Span> = Vec::new();
    let mut push = |kind: SpanKind, range: std::ops::Range<usize>| {
        let single = matches!(kind, SpanKind::Break | SpanKind::Separator);
        match out.last_mut() {
            Some(last) if last.kind == kind && !single && last.range.end == range.start => {
                last.range.end = range.end;
            }
            _ => out.push(Span { kind, range }),
        }
    };
    if bom > 0 {
        push(SpanKind::Invisible, 0..bom);
    }
    for (b, ch) in text.char_indices() {
        let kind = match (ch, marks[b]) {
            _ if is_invisible(ch) => SpanKind::Invisible,
            ('\n', Some(track::Mark::Break)) => SpanKind::Break,
            ('\n', Some(track::Mark::Content)) => SpanKind::Content,
            ('\n', _) => SpanKind::Separator,
            (_, Some(track::Mark::Content)) => SpanKind::Content,
            _ => SpanKind::Markup,
        };
        push(kind, origin[b]..origin[b + ch.len_utf8()]);
    }
    out
}

fn is_invisible(ch: char) -> bool {
    matches!(
        ch,
        '\u{a0}'
            | '\u{202f}'
            | '\u{ad}'
            | '\u{200b}'..='\u{200d}'
            | '\u{2060}'
            | '\u{feff}'
            | '\u{e000}'..='\u{f8ff}'
            | '\u{f0000}'..='\u{ffffd}'
            | '\u{100000}'..='\u{10fffd}'
    )
}

/// Reads HTML and writes its display, the one canonical notation
/// (`display` of the specification). Blocks outside the profile and islands
/// are shown as raw blocks.
pub fn display(html: &str, profile: &Profile) -> Output {
    let writer = write::Writer { profile };
    if crate::head::is_document(html) {
        let (head, nodes, diagnostics) = canon::read_document(html, profile);
        let body = writer.display(&nodes);
        let mut text = front::write(&head);
        if !body.is_empty() {
            text.push('\n');
            text.push_str(&body);
        }
        return Output { text, diagnostics };
    }
    let (nodes, diagnostics) = canon::read(html, profile);
    let mut text = writer.display(&nodes);
    // W2: an hr that would begin a front matter is written `.hr` (§5.15).
    if text.starts_with("---\n") && front::read(&text.split('\n').collect::<Vec<_>>()).is_some() {
        text.replace_range(..3, ".hr");
    }
    Output { text, diagnostics }
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
        "<ol start=3>",
        "<dl>",
        "<dt>",
        "<dd>",
        "</dl>",
        "<table>",
        "<tr>",
        "<td>",
        "<th>",
        "</table>",
        "<caption>",
        "<div class=x>",
        "</div>",
        "<aside>",
        "<pre>",
        "</pre>",
        "<h2>",
        "</h2>",
        "<details>",
        "<summary>",
        "<blockquote>",
        "<figure>",
        "<figcaption>",
        "<img alt=a src=b>",
        "<span class=\"a\">",
        "</span>",
        "<em>",
        "</em>",
        "<strong>",
        "</strong>",
        "<code>",
        "</code>",
        "<sup>",
        "</sup>",
        "<sub>",
        "<mark>",
        "<s>",
        "<kbd>",
        "<br>",
        "<a href=\"x y\">",
        "</a>",
        "<div class=\"callout warning\" role=note>",
        "<p class=title>",
        "<hr>",
        "<!--c-->",
        "&lt;",
        "&amp;",
        "&nbsp;",
        "\n",
        " ",
        "  ",
        "*",
        "**",
        "_",
        "=",
        "==",
        "~",
        "^",
        "`",
        "``",
        "[",
        "]",
        "(",
        ")",
        "\\",
        "#",
        "# ",
        "- ",
        "1. ",
        "> ",
        "|",
        ".p",
        ".span(x)",
        "{#a}",
        ":::",
        "---",
        "<",
        ">",
        "!",
        "text",
        "Eisen",
        "x",
        "1",
        "ü",
        "[!note]",
        "\u{a0}",
        "\t",
    ];

    fn inputs(n: usize, seed: u64) -> impl Iterator<Item = String> {
        let mut state = seed;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        (0..n).map(move |_| {
            let len = (next() % 20) as usize;
            (0..len)
                .map(|_| FRAGMENTS[(next() % FRAGMENTS.len() as u64) as usize])
                .collect()
        })
    }

    /// L1 for every canonical document the generator reaches, and the
    /// display read back gives the same display (L2 in effect).
    #[test]
    fn display_reads_back() {
        let profile = Profile::reference("test");
        let n: usize = option_env!("LIPUPONA_L1_CASES")
            .and_then(|s| s.parse().ok())
            .unwrap_or(3_000);
        let mut failures = Vec::new();
        for input in inputs(n, 0x5555_aaaa_3333_cccc) {
            let h = canon::canon(&input, &profile).text;
            let d = display(&h, &profile).text;
            let back = super::html(&d, &profile).text;
            if back != h {
                failures.push(format!(
                    "input {input:?}\n  canon   {h:?}\n  display {d:?}\n  html    {back:?}"
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {n} fail L1; first ones:\n{}",
            failures.len(),
            failures
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    const NOTATION: &[&str] = &[
        "---\n",
        "title: Eisen\n",
        "tags: [a, \"b, c\"]\n",
        "lang: de\n",
        "  - x\n",
        "<link rel=\"x\">\n",
        "<body class=\"y\">\n",
        "\n",
        "\n",
        "\n\n",
        "  ",
        "    ",
        "\t",
        " ",
        "- ",
        "1. ",
        "3. ",
        ": ",
        ":",
        "> ",
        ">",
        "> [!note]",
        "> [!tip]- T",
        "> [!WARNING]+",
        "> [!dosage]",
        "# ",
        "## ",
        "##### ",
        "---",
        "```",
        "```rust",
        "~~~",
        "```{=html}",
        ":::",
        "::: div.x",
        "::::",
        "{#a .b}",
        "{lang=\"en\"}",
        "{x}",
        "|",
        "| a |",
        "|---|",
        "|:--|",
        "|=== 2",
        "|===",
        "|---",
        "2+|",
        ".3+|",
        "a|",
        "= ",
        "^",
        "<",
        ".p",
        ".p.lead",
        ".div.x",
        ".ul",
        ".li(x)",
        ".td(a)",
        ".table",
        ".tbody",
        ".tr",
        ".figure",
        ".img[alt=\"a\" src=\"b\"]",
        ".figcaption(c)",
        ".span(x)",
        ".span#i(x)",
        ".br()",
        ".hr(x)",
        ".P",
        "**",
        "*",
        "__",
        "_",
        "==",
        "~~",
        "~",
        "^",
        "`",
        "``",
        "[",
        "](",
        ")",
        "(",
        "\\",
        "\\*",
        "<!--",
        "-->",
        "<p>",
        "</p>",
        "<strong>",
        "</strong>",
        "<blink>",
        "&nbsp;",
        "&",
        "\u{a0}",
        "text",
        "Eisen",
        "x",
        "1",
        "ü",
        "[a](b c)",
        "https://x.org/a",
        "{",
        "}",
        "#",
        "=",
        "|",
        "\\|",
    ];

    fn notation(n: usize, seed: u64) -> impl Iterator<Item = String> {
        let mut state = seed;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        (0..n).map(move |_| {
            let len = (next() % 24) as usize;
            (0..len)
                .map(|_| NOTATION[(next() % NOTATION.len() as u64) as usize])
                .collect()
        })
    }

    /// Notation nested beyond `html::MAX_DEPTH` in every way the reader
    /// nests: it is reported, keeps its text and reads back from its display.
    #[test]
    fn deep_notation_is_bounded() {
        let profile = Profile::reference("test");
        let deep = 300;
        let inputs = [
            format!("{}Eisen\n", "> ".repeat(2_000)),
            format!("{}Eisen\n", ">".repeat(2_000)),
            (0..deep)
                .map(|k| format!("{}- Eisen\n", "  ".repeat(k)))
                .collect(),
            (0..deep)
                .map(|k| format!("{}.div\n", "  ".repeat(k)))
                .chain(std::iter::once(format!("{}Eisen\n", "  ".repeat(deep))))
                .collect(),
            (0..deep)
                .map(|k| format!("{} div\n", ":".repeat(3 + deep - k)))
                .chain(std::iter::once("Eisen\n".to_string()))
                .chain((0..deep).map(|k| format!("{}\n", ":".repeat(4 + k))))
                .collect(),
            format!("{}Eisen{}\n", ".span(".repeat(2_000), ")".repeat(2_000)),
            format!("{}Eisen{}\n", "[".repeat(2_000), "](x)".repeat(2_000)),
            format!("{}Eisen{}\n", ".em([".repeat(1_000), "](x))".repeat(1_000)),
            // Beyond the cap only through typed HTML, seen on reading back.
            format!(
                "{}{}Eisen{}\n",
                "> ".repeat(200),
                "<span>".repeat(100),
                "</span>".repeat(100)
            ),
        ];
        for input in inputs {
            let head: String = input.chars().take(12).collect();
            let h = super::html(&input, &profile);
            assert!(
                h.diagnostics
                    .iter()
                    .any(|d| d.code == crate::Code::LimitExceeded),
                "{head:?}: {:?}",
                h.diagnostics.iter().take(3).collect::<Vec<_>>()
            );
            assert!(h.text.contains("Eisen"), "{head:?}");
            let d = display(&h.text, &profile).text;
            assert_eq!(super::html(&d, &profile).text, h.text, "{head:?}");
            assert_eq!(check_spans(&input, &profile), None, "{head:?}");
        }
    }

    /// Many openers without a partner, in every construct whose closer is
    /// searched for, and chains of typed blocks that are not taken: each was
    /// quadratic once (seconds to minutes at this size), and each reads back
    /// from its display.
    #[test]
    fn unclosed_openers_stay_linear() {
        let profile = Profile::reference("test");
        let n = 8_000;
        let ticks: String = (1..200).map(|k| "`".repeat(k) + "a").collect();
        let inputs = [
            "**a ".repeat(n),
            "==a ".repeat(n),
            "^a ".repeat(n),
            "[a ".repeat(n),
            "[a](b ".repeat(n),
            ".em(a ".repeat(n),
            ".a".repeat(5 * n),
            ".span.a".repeat(n),
            format!("{}</b>", "<b>a ".repeat(n)),
            ticks,
            "<div>\n".repeat(n),
            "<!-- a\n".repeat(n),
            format!("{}x\n{}", "<div>\n".repeat(n), "</div>\n".repeat(n)),
            format!("{}<section>\n{}", "<p>\n".repeat(n), "</p>\n".repeat(n)),
            format!("{}<section>\n{}", "<li>\n".repeat(n), "</li>\n".repeat(n)),
        ];
        for input in inputs {
            let head: String = input.chars().take(12).collect();
            let h = super::html(&input, &profile).text;
            let d = display(&h, &profile).text;
            assert_eq!(super::html(&d, &profile).text, h, "{head:?}");
            assert_eq!(check_spans(&input, &profile), None, "{head:?}");
        }
    }

    /// The text of a tree as the reader built it: its text nodes in order,
    /// a `br` as a line feed.
    fn tree_text(nodes: &[crate::dom::Node], out: &mut String) {
        for node in nodes {
            match node {
                crate::dom::Node::Text(t) => out.push_str(&t.text),
                crate::dom::Node::Element(e) if e.name == "br" => out.push('\n'),
                crate::dom::Node::Element(e) => tree_text(&e.children, out),
                _ => {}
            }
        }
    }

    /// Checks the spans of `input`: they tile it, and their content and
    /// breaks are the text of what the reader reads (`None`), or why not.
    fn check_spans(input: &str, profile: &Profile) -> Option<String> {
        let spans = super::spans(input, profile);
        let mut at = 0;
        for s in &spans {
            if s.range.start != at || s.range.end <= s.range.start {
                return Some(format!("gap or overlap at {at}: {s:?}"));
            }
            at = s.range.end;
        }
        if at != input.len() {
            return Some(format!("spans end at {at} of {}", input.len()));
        }
        // Text that is not in the notation as it stands: a character
        // reference in typed HTML, the default title of a callout.
        if input.contains('&') || input.contains("[!") {
            return None;
        }
        let text = crate::html::lf(input.strip_prefix('\u{feff}').unwrap_or(input));
        let lines: Vec<&str> = text.split('\n').collect();
        // A front matter's values are content but no text of the tree: the
        // body, after the front matter's line feeds, is compared.
        let used = front::read(&lines).map_or(0, |f| f.used);
        let mut tree = String::new();
        tree_text(
            &block::Reader::new(profile).blocks(&lines[used..], false),
            &mut tree,
        );
        let mut feeds = 0;
        let content: String = spans
            .iter()
            .filter(|s| {
                let before = feeds;
                feeds += input[s.range.clone()].matches('\n').count();
                before >= used && s.kind == SpanKind::Content
            })
            .map(|s| &input[s.range.clone()])
            .collect();
        // Line feeds and invisible characters aside: a `br` may be written
        // `.br()`, and an invisible character may stand in markup.
        let plain = |s: &str| -> String {
            s.chars()
                .filter(|&c| !matches!(c, '\n' | '\r') && !is_invisible(c))
                .collect()
        };
        (plain(&content) != plain(&tree))
            .then(|| format!("content {content:?}\n  tree    {tree:?}"))
    }

    /// The spans of generated notation tile it and hold its text.
    #[test]
    fn spans_tile_and_hold_the_text() {
        let profile = Profile::reference("test");
        let n: usize = option_env!("LIPUPONA_L2_CASES")
            .and_then(|s| s.parse().ok())
            .unwrap_or(3_000);
        let mut failures = Vec::new();
        for input in notation(n, 0x5eed_0f5a_a175_0001) {
            for input in [
                input.clone(),
                input.replace('\n', "\r\n"),
                format!("\u{feff}{input}"),
            ] {
                if let Some(why) = check_spans(&input, &profile) {
                    failures.push(format!("input {input:?}\n  {why}"));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {n} fail; first ones:\n{}",
            failures.len(),
            failures
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// L2/L3 for generated notation, valid or not: what it reads as is
    /// what its display reads as.
    #[test]
    fn notation_reads_back() {
        let profile = Profile::reference("test");
        let n: usize = option_env!("LIPUPONA_L2_CASES")
            .and_then(|s| s.parse().ok())
            .unwrap_or(3_000);
        let mut failures = Vec::new();
        for input in notation(n, 0x9159_015a_3070_dd17) {
            if option_env!("LIPUPONA_TRACE").is_some() {
                eprintln!("{input:?}");
            }
            let h = super::html(&input, &profile).text;
            let d = display(&h, &profile).text;
            let back = super::html(&d, &profile).text;
            if back != h {
                failures.push(format!(
                    "input {input:?}\n  html    {h:?}\n  display {d:?}\n  html    {back:?}"
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {n} fail L2; first ones:\n{}",
            failures.len(),
            failures
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
