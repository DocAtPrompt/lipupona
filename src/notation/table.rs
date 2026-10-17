//! Reading tables (SPEC §5.9): the pipe grid and the cell-per-line form.

use super::block::{Reader, SP, dedent, indent_width, is_blank};
use super::inline::element;
use super::syntax;
use crate::diag::{Code, Diagnostic};
use crate::dom::{Attr, Node};

struct Cell {
    row: usize,
    col: usize,
    header: bool,
    colspan: usize,
    rowspan: usize,
    attrs: Vec<Attr>,
    children: Vec<Node>,
}

/// Splits a grid row at the `|` that are not escaped; the cells keep their
/// escapes for the inline reader.
fn split_row(line: &str) -> Vec<String> {
    let s = line.trim_matches(SP);
    let s = s.strip_prefix('|').unwrap_or(s);
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                cur.push(c);
                if let Some(&n) = chars.peek() {
                    cur.push(n);
                    chars.next();
                }
            }
            '|' => cells.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !cur.trim_matches(SP).is_empty() {
        cells.push(cur);
    }
    cells
        .into_iter()
        .map(|c| c.trim_matches(SP).to_string())
        .collect()
}

fn is_separator(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|c| {
            let inner = c.trim_start_matches(':').trim_end_matches(':');
            !inner.is_empty() && inner.chars().all(|x| x == '-')
        })
}

/// The pipe grid at `lines[i]`, with the caption line above it if any.
pub(crate) fn grid(
    r: &mut Reader,
    lines: &[String],
    i: usize,
    caption: Option<String>,
) -> (Node, usize) {
    let mut j = i;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut header_rows = None;
    while j < lines.len()
        && lines[j].trim_start_matches(SP).starts_with('|')
        && !lines[j].trim_start_matches(SP).starts_with("|===")
    {
        let cells = split_row(&lines[j]);
        if header_rows.is_none() && is_separator(&cells) {
            if cells.iter().any(|c| c.contains(':')) {
                r.diags.push(Diagnostic::new(
                    Code::AlignmentDropped,
                    "colons in the separator line",
                ));
            }
            header_rows = Some(rows.len());
        } else {
            rows.push(cells);
        }
        j += 1;
    }
    let header_rows = header_rows.unwrap_or(0);
    let mut cells: Vec<Cell> = Vec::new();
    let mut owner: Vec<Vec<Option<usize>>> = Vec::new();
    for (row, raw) in rows.iter().enumerate() {
        let mut here: Vec<Option<usize>> = Vec::new();
        for (col, text) in raw.iter().enumerate() {
            let up = (text == "^")
                .then(|| {
                    owner
                        .get(row.wrapping_sub(1))
                        .and_then(|o| o.get(col))
                        .copied()
                        .flatten()
                })
                .flatten();
            let left = (text == "<")
                .then(|| {
                    col.checked_sub(1)
                        .and_then(|c| here.get(c))
                        .copied()
                        .flatten()
                })
                .flatten();
            if let Some(c) = up {
                if cells[c].col == col && cells[c].row + cells[c].rowspan == row {
                    cells[c].rowspan += 1;
                }
                here.push(Some(c));
            } else if let Some(c) = left.filter(|&c| cells[c].row == row) {
                cells[c].colspan += 1;
                here.push(Some(c));
            } else {
                let (header, content) = match text.strip_prefix("= ") {
                    Some(rest) => (true, rest),
                    None if text == "=" => (true, ""),
                    None => (row < header_rows, text.as_str()),
                };
                let children = r.phrasing(content);
                cells.push(Cell {
                    row,
                    col,
                    header: header || row < header_rows,
                    colspan: 1,
                    rowspan: 1,
                    attrs: Vec::new(),
                    children,
                });
                here.push(Some(cells.len() - 1));
            }
        }
        owner.push(here);
    }
    (assemble(r, cells, rows.len(), header_rows, caption), j)
}

/// The cell-per-line form at `lines[i]` (`|=== n` … `|===`).
pub(crate) fn cells(
    r: &mut Reader,
    lines: &[String],
    i: usize,
    caption: Option<String>,
) -> (Node, usize) {
    let n: usize = lines[i].trim_matches(SP)[4..]
        .trim_matches(SP)
        .parse()
        .unwrap_or(1)
        .max(1);
    let mut j = i + 1;
    let mut cells: Vec<Cell> = Vec::new();
    let mut occupied: Vec<Vec<bool>> = Vec::new();
    let (mut row, mut col) = (0usize, 0usize);
    let mut header_rows = None;
    while j < lines.len() {
        let line = lines[j].trim_matches(SP);
        if line == "|===" {
            j += 1;
            break;
        }
        if line == "|---" {
            if col > 0 {
                row += 1;
                col = 0;
            }
            header_rows = Some(row);
            j += 1;
            continue;
        }
        let Some((colspan, rowspan, attrs, blocks, content)) = cell_line(line) else {
            j += 1;
            continue;
        };
        j += 1;
        // The next free position.
        loop {
            if col >= n {
                row += 1;
                col = 0;
            }
            let taken = occupied
                .get(row)
                .and_then(|r| r.get(col))
                .copied()
                .unwrap_or(false);
            if !taken {
                break;
            }
            col += 1;
        }
        for rr in row..row + rowspan {
            while occupied.len() <= rr {
                occupied.push(vec![false; n]);
            }
            for slot in occupied[rr].iter_mut().skip(col).take(colspan) {
                *slot = true;
            }
        }
        let (header, children) = if blocks {
            let mut k = j;
            while k < lines.len() && (is_blank(&lines[k]) || indent_width(&lines[k]) >= 2) {
                k += 1;
            }
            let body: Vec<String> = lines[j..k].iter().map(|l| dedent(l, 2)).collect();
            j = k;
            (false, r.blocks(&body, false))
        } else {
            match content.strip_prefix("= ") {
                Some(rest) => (true, r.phrasing(rest)),
                None if content == "=" => (true, Vec::new()),
                None => (false, r.phrasing(&content)),
            }
        };
        cells.push(Cell {
            row,
            col,
            header,
            colspan,
            rowspan,
            attrs,
            children,
        });
        col += colspan;
    }
    let rows = occupied
        .len()
        .max(cells.iter().map(|c| c.row + 1).max().unwrap_or(0));
    let header_rows = header_rows.unwrap_or(0);
    for c in cells.iter_mut() {
        if c.row < header_rows {
            c.header = true;
        }
    }
    (assemble(r, cells, rows, header_rows, caption), j)
}

/// A cell line: `2+`, `.3+`, `2.3+`, `[attrs]`, `a`, then `|` and content.
fn cell_line(line: &str) -> Option<(usize, usize, Vec<Attr>, bool, String)> {
    let bar = line.find('|')?;
    let (prefix, rest) = (&line[..bar], &line[bar + 1..]);
    let content = rest.strip_prefix(' ').unwrap_or(rest).to_string();
    let mut p = prefix;
    let (mut colspan, mut rowspan) = (1, 1);
    if let Some(plus) = p.find('+') {
        let spans = &p[..plus];
        if !spans.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return None;
        }
        let (c, r) = spans.split_once('.').unwrap_or((spans, ""));
        colspan = if c.is_empty() { 1 } else { c.parse().ok()? };
        rowspan = if r.is_empty() { 1 } else { r.parse().ok()? };
        p = &p[plus + 1..];
    }
    let mut attrs = Vec::new();
    if p.starts_with('[') {
        let close = p.rfind(']')?;
        let c: Vec<char> = format!(".td{}", &p[..=close]).chars().collect();
        let sel = syntax::selector(&c, 0, c.len())?;
        attrs = sel
            .attrs
            .into_iter()
            .map(|(name, value)| Attr { name, value })
            .collect();
        p = &p[close + 1..];
    }
    let blocks = match p {
        "" => false,
        "a" => true,
        _ => return None,
    };
    Some((colspan.max(1), rowspan.max(1), attrs, blocks, content))
}

fn assemble(
    r: &mut Reader,
    cells: Vec<Cell>,
    rows: usize,
    header_rows: usize,
    caption: Option<String>,
) -> Node {
    let mut by_row: Vec<Vec<Node>> = (0..rows).map(|_| Vec::new()).collect();
    for c in cells {
        let mut attrs = c.attrs;
        if c.colspan > 1 {
            attrs.push(Attr {
                name: "colspan".into(),
                value: c.colspan.to_string(),
            });
        }
        if c.rowspan > 1 {
            attrs.push(Attr {
                name: "rowspan".into(),
                value: c.rowspan.to_string(),
            });
        }
        if let Some(row) = by_row.get_mut(c.row) {
            row.push(element(
                if c.header { "th" } else { "td" },
                attrs,
                c.children,
            ));
        }
    }
    let mut parts = Vec::new();
    if let Some(caption) = caption {
        parts.push(element(
            "caption",
            Vec::new(),
            r.phrasing(caption.trim_matches(SP)),
        ));
    }
    let rows: Vec<Node> = by_row
        .into_iter()
        .map(|cells| element("tr", Vec::new(), cells))
        .collect();
    let split = header_rows.min(rows.len());
    let mut rows = rows.into_iter();
    if split > 0 {
        parts.push(element(
            "thead",
            Vec::new(),
            rows.by_ref().take(split).collect(),
        ));
    }
    parts.push(element("tbody", Vec::new(), rows.collect()));
    element("table", Vec::new(), parts)
}
