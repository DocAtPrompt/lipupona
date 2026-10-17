//! The notation (SPEC §5, §6): reading it into a tree and writing the
//! display of a tree.

mod block;
mod inline;
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
    let mut nodes = reader.blocks(&lines, false);
    let mut diagnostics = reader.diags;
    diagnostics.extend(profile.diagnose(&nodes, false));
    canon::normalize(&mut nodes, profile);
    // The document is the HTML: what an HTML parser makes of the tree is
    // what was written. A tree no parser builds — an `li` directly in an
    // `li` — is stored as a parser reads it back.
    let text = canon::serialize(&nodes, profile);
    let (nodes, _) = canon::read(&text, profile);
    Output {
        text: canon::serialize(&nodes, profile),
        diagnostics,
    }
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
