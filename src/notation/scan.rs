//! The searches for what closes a construct in phrasing (SPEC §5.5): code
//! spans, comments, markers, link brackets and paths, round brackets,
//! attribute brackets and the `#id.class` chain of a selector.
//!
//! Every search is a walk from a start position whose next step depends only
//! on the position it is at, so walks from different starts meet and then go
//! on together. What a walk found is kept for every position it passed, with
//! the end of the range it was read in. Many openers without a partner thus
//! cost linear time, not quadratic: each position is walked once.
//!
//! A partner found before a shorter end is found there too: every step of
//! the walk lay before it. Nested content, read with ever shorter ends, so
//! reuses what the walk of the outer level found. That no partner was found
//! holds only for the same end: a code span whose partner lies beyond a
//! shorter end is text there, and the walk goes on inside it.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// A hasher for small integer keys (no protection against chosen keys is
/// needed: the keys are positions).
#[derive(Default)]
struct Fast(u64);

impl Hasher for Fast {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn write_usize(&mut self, n: usize) {
        self.write_u64(n as u64);
    }
}

type Map<K, V> = HashMap<K, V, BuildHasherDefault<Fast>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Walk {
    Marker(&'static str),
    Bracket,
    Path,
    Paren,
    Attrs,
    Chain { id: bool },
}

/// One step of a walk.
enum Step {
    To(usize),
    Found(usize),
    Fail,
}

/// One step of a walk over nesting brackets.
enum Nest {
    To(usize),
    Open,
    Close,
    Fail,
}

/// The searches over one text. The text itself is passed to every call; it
/// must be the one the scan was made for.
pub(crate) struct Scan {
    /// Maximal runs of backticks: start and length, in order.
    runs: Vec<(usize, usize)>,
    /// The starts of those runs per length, in order.
    by_len: HashMap<usize, Vec<usize>>,
    /// Where `-->` begins, in order.
    dashes: Vec<usize>,
    /// Where whitespace is, in order.
    spaces: Vec<usize>,
    /// Per walk and start position: the end it was read with, and what it
    /// found.
    found: Map<(Walk, usize), (usize, Option<usize>)>,
}

impl Scan {
    pub(crate) fn new(c: &[char]) -> Self {
        let mut runs = Vec::new();
        let mut by_len: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut i = 0;
        while i < c.len() {
            if c[i] == '`' {
                let n = c[i..].iter().take_while(|&&x| x == '`').count();
                runs.push((i, n));
                by_len.entry(n).or_default().push(i);
                i += n;
            } else {
                i += 1;
            }
        }
        let dashes = (0..c.len().saturating_sub(2))
            .filter(|&k| c[k..k + 3] == ['-', '-', '>'])
            .collect();
        let spaces = (0..c.len()).filter(|&k| c[k].is_whitespace()).collect();
        Scan {
            runs,
            by_len,
            dashes,
            spaces,
            found: Map::default(),
        }
    }

    /// What the walk `kind` from `at` finds before `end`, if known.
    fn known(&self, kind: Walk, at: usize, end: usize) -> Option<Option<usize>> {
        let &(read, result) = self.found.get(&(kind, at))?;
        if read == end {
            return Some(result);
        }
        let x = result?;
        let fits = match kind {
            Walk::Marker(m) => x + m.len() <= end,
            Walk::Bracket | Walk::Path | Walk::Paren => x < end,
            Walk::Attrs | Walk::Chain { .. } => x <= end,
        };
        (read > end && fits && !self.cuts_run(end)).then_some(Some(x))
    }

    fn keep(&mut self, kind: Walk, at: usize, end: usize, result: Option<usize>) {
        self.found.insert((kind, at), (end, result));
    }

    /// Whether `end` falls inside a run of backticks, which a shorter end
    /// reads as a shorter run.
    fn cuts_run(&self, end: usize) -> bool {
        let k = self.runs.partition_point(|&(s, _)| s < end);
        k.checked_sub(1)
            .is_some_and(|k| self.runs[k].0 + self.runs[k].1 > end)
    }

    /// Whether `c[a..b]` holds whitespace.
    pub(crate) fn space_in(&self, a: usize, b: usize) -> bool {
        let k = self.spaces.partition_point(|&x| x < a);
        self.spaces.get(k).is_some_and(|&x| x < b)
    }

    /// The length of the backtick run at `i`, counted up to `end`.
    fn run_len(&self, i: usize, end: usize) -> usize {
        let k = self.runs.partition_point(|&(s, _)| s <= i);
        match k.checked_sub(1).map(|k| self.runs[k]) {
            Some((s, n)) if i < s + n => (s + n).min(end).saturating_sub(i),
            _ => 0,
        }
    }

    /// The index after a code span at `c[i] == '`'` (§5.5.3): the first later
    /// run of the same length closes it. A run cut by `end` counts with the
    /// part before it.
    pub(crate) fn code_span_end(&self, i: usize, end: usize) -> Option<usize> {
        let n = self.run_len(i, end);
        if n == 0 || i + n >= end {
            return None;
        }
        let from = i + n;
        if let Some(starts) = self.by_len.get(&n) {
            let k = starts.partition_point(|&x| x < from);
            if let Some(&x) = starts.get(k)
                && x + n <= end
            {
                return Some(x + n);
            }
        }
        let k = self.runs.partition_point(|&(s, _)| s < end);
        match k.checked_sub(1).map(|k| self.runs[k]) {
            Some((s, len)) if s >= from && s + len > end && end - s == n => Some(end),
            _ => None,
        }
    }

    /// A code span at `c[i] == '`'`: its content and the index after it.
    pub(crate) fn code_span(&self, c: &[char], i: usize, end: usize) -> Option<(String, usize)> {
        let next = self.code_span_end(i, end)?;
        let n = self.run_len(i, end);
        let mut content: String = c[i + n..next - n].iter().collect();
        if content.len() >= 2
            && content.starts_with(' ')
            && content.ends_with(' ')
            && !content.chars().all(|x| x == ' ')
        {
            content = content[1..content.len() - 1].to_string();
        }
        Some((content, next))
    }

    /// The index after a comment at `c[i]`, if one begins there and is
    /// closed before `end`.
    pub(crate) fn comment_end(&self, c: &[char], i: usize, end: usize) -> Option<usize> {
        if i + 4 > end || c[i..i + 4] != ['<', '!', '-', '-'] {
            return None;
        }
        let k = self.dashes.partition_point(|&x| x < i + 4);
        let &x = self.dashes.get(k)?;
        (x + 3 <= end).then_some(x + 3)
    }

    /// The first `m` that closes a marker on the walk from `from` (§5.5.2):
    /// escapes and code spans are skipped; a closer follows no whitespace,
    /// and an intraword marker is followed by no letter or digit.
    pub(crate) fn marker_close(
        &mut self,
        c: &[char],
        m: &'static str,
        from: usize,
        end: usize,
    ) -> Option<usize> {
        self.walk(Walk::Marker(m), from, end, |scan, j| {
            if j >= end {
                Step::Fail
            } else if c[j] == '\\' && j + 1 < end && c[j + 1].is_ascii_punctuation() {
                Step::To(j + 2)
            } else if let Some(next) = (c[j] == '`').then(|| scan.code_span_end(j, end)).flatten() {
                Step::To(next)
            } else if super::inline::closes_at(c, m, j, end) {
                Step::Found(j)
            } else {
                Step::To(j + 1)
            }
        })
    }

    /// The `]` that closes a link's `[` at `i` (§5.5.4): brackets nest,
    /// escapes, code spans and comments are skipped, a line feed ends the
    /// search.
    pub(crate) fn bracket_close(&mut self, c: &[char], i: usize, end: usize) -> Option<usize> {
        self.nested(Walk::Bracket, i + 1, end, |scan, j| {
            if let Some(next) = scan.comment_end(c, j, end) {
                return Nest::To(next);
            }
            match c[j] {
                '\\' if j + 1 < end && c[j + 1].is_ascii_punctuation() => Nest::To(j + 2),
                '`' => Nest::To(scan.code_span_end(j, end).unwrap_or(j + 1)),
                '[' => Nest::Open,
                ']' => Nest::Close,
                '\n' => Nest::Fail,
                _ => Nest::To(j + 1),
            }
        })
    }

    /// The `)` that closes a link's path whose first character is at `from`:
    /// brackets nest, `\)` and `\\` are escapes, a line feed ends the search.
    pub(crate) fn path_close(&mut self, c: &[char], from: usize, end: usize) -> Option<usize> {
        self.nested(Walk::Path, from, end, |_, k| match c[k] {
            '\\' if k + 1 < end && matches!(c[k + 1], ')' | '\\') => Nest::To(k + 2),
            '\n' => Nest::Fail,
            '(' => Nest::Open,
            ')' => Nest::Close,
            _ => Nest::To(k + 1),
        })
    }

    /// The `)` that closes a round bracket whose content begins at `from`:
    /// brackets nest, `\(` and `\)` do not count, code spans and comments are
    /// skipped.
    pub(crate) fn paren_close(&mut self, c: &[char], from: usize, end: usize) -> Option<usize> {
        self.nested(Walk::Paren, from, end, |scan, j| {
            if let Some(next) = scan.comment_end(c, j, end) {
                return Nest::To(next);
            }
            match c[j] {
                '\\' if j + 1 < end && c[j + 1].is_ascii_punctuation() => Nest::To(j + 2),
                '`' => Nest::To(
                    scan.code_span_end(j, end)
                        .unwrap_or_else(|| j + scan.run_len(j, end).max(1)),
                ),
                '(' => Nest::Open,
                ')' => Nest::Close,
                _ => Nest::To(j + 1),
            }
        })
    }

    /// The index after an attribute bracket `[name="value" …]` at
    /// `c[i] == '['`.
    pub(crate) fn attrs_end(&mut self, c: &[char], i: usize, end: usize) -> Option<usize> {
        self.walk(Walk::Attrs, i + 1, end, |_, mut j| {
            while j < end && c[j] == ' ' {
                j += 1;
            }
            if j >= end {
                return Step::Fail;
            }
            if c[j] == ']' {
                return Step::Found(j + 1);
            }
            let n = j;
            while j < end && (c[j].is_ascii_alphanumeric() || matches!(c[j], '-' | '_' | ':')) {
                j += 1;
            }
            if j == n || c.get(j) != Some(&'=') || c.get(j + 1) != Some(&'"') {
                return Step::Fail;
            }
            j += 2;
            loop {
                if j >= end {
                    return Step::Fail;
                }
                match c[j] {
                    '\\' if matches!(c.get(j + 1), Some('"' | '\\')) => j += 2,
                    '"' => {
                        j += 1;
                        break;
                    }
                    _ => j += 1,
                }
            }
            if j < end && c[j] != ' ' && c[j] != ']' {
                return Step::Fail;
            }
            Step::To(j)
        })
    }

    /// The end of a selector's `#id` and `.class` chain that begins at
    /// `from`: tokens in any order, the id at most once.
    pub(crate) fn chain_end(&mut self, c: &[char], from: usize, end: usize) -> usize {
        let mut path = Vec::new();
        let (mut j, mut id) = (from, false);
        let result = loop {
            if let Some(Some(r)) = self.known(Walk::Chain { id }, j, end) {
                break r;
            }
            path.push((Walk::Chain { id }, j));
            if !(j < end && (c[j] == '#' || c[j] == '.')) {
                break j;
            }
            let t = j + 1;
            let mut k = t;
            while k < end && super::syntax::is_token_char(c[k]) {
                k += 1;
            }
            if k == t || (c[j] == '#' && id) {
                break j;
            }
            id |= c[j] == '#';
            j = k;
        };
        for (walk, k) in path {
            self.keep(walk, k, end, Some(result));
        }
        result
    }

    fn walk(
        &mut self,
        kind: Walk,
        from: usize,
        end: usize,
        step: impl Fn(&Self, usize) -> Step,
    ) -> Option<usize> {
        let mut path = Vec::new();
        let mut j = from;
        let result = loop {
            if let Some(r) = self.known(kind, j, end) {
                break r;
            }
            path.push(j);
            match step(self, j) {
                Step::To(k) => j = k,
                Step::Found(k) => break Some(k),
                Step::Fail => break None,
            }
        };
        for k in path {
            self.keep(kind, k, end, result);
        }
        result
    }

    /// A walk over brackets that nest: an inner bracket is the same walk
    /// from its first character, kept under that position.
    fn nested(
        &mut self,
        kind: Walk,
        from: usize,
        end: usize,
        step: impl Fn(&Self, usize) -> Nest,
    ) -> Option<usize> {
        if let Some(r) = self.known(kind, from, end) {
            return r;
        }
        let mut open = vec![from];
        let mut j = from;
        while j < end {
            match step(self, j) {
                Nest::To(k) => j = k,
                Nest::Open => match self.known(kind, j + 1, end) {
                    Some(Some(close)) => j = close + 1,
                    Some(None) => break,
                    None => {
                        open.push(j + 1);
                        j += 1;
                    }
                },
                Nest::Close => {
                    if let Some(start) = open.pop() {
                        self.keep(kind, start, end, Some(j));
                    }
                    if open.is_empty() {
                        return Some(j);
                    }
                    j += 1;
                }
                Nest::Fail => break,
            }
        }
        for start in open {
            self.keep(kind, start, end, None);
        }
        None
    }
}
