//! The notation (SPEC §5, §6): reading it into a tree and writing the
//! display of a tree.

mod block;
mod inline;
mod scan;
mod syntax;
mod table;
mod write;

use crate::profile::Profile;
use crate::{Output, canon};

/// Reads notation and returns the canonical HTML it means (`html` of the
/// specification), with what was reported on the way.
pub fn html(notation: &str, profile: &Profile) -> Output {
    let text = crate::html::lf(notation.strip_prefix('\u{feff}').unwrap_or(notation));
    let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let mut reader = block::Reader::new(profile);
    let nodes = reader.blocks(&lines, false);
    let mut out = canon::store(nodes, profile);
    let mut diagnostics = reader.diags;
    let too_deep = crate::html::too_deep();
    if diagnostics.contains(&too_deep) {
        out.diagnostics.retain(|d| *d != too_deep);
    }
    diagnostics.append(&mut out.diagnostics);
    out.diagnostics = diagnostics;
    out
}

/// Reads HTML and writes its display, the one canonical notation
/// (`display` of the specification). Blocks outside the profile and islands
/// are shown as raw blocks.
pub fn display(html: &str, profile: &Profile) -> Output {
    let (nodes, diagnostics) = canon::read(html, profile);
    let writer = write::Writer { profile };
    Output {
        text: writer.display(&nodes),
        diagnostics,
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
        "<ol start=3>",
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
        }
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
