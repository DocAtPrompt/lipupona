//! What the readers record for an editor's spans (SPEC §5.14): which
//! characters of the notation become text of the document (*content*) and
//! which line feeds are line breaks (*break*). Everything else is markup or,
//! for a line feed, a separator.
//!
//! The block reader works on slices of the notation, so a slice tells where
//! it came from; phrasing, which the block reader assembles from lines and
//! may escape (§6, a typed block not taken), carries the offset of each of
//! its characters along in a [`Src`].

/// What a range of the notation was recorded as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    Content,
    Break,
}

/// The record of one reading: byte ranges of the notation (after line ends
/// are normalised, §5.2), in the order they were recorded; a later record
/// wins over an earlier one.
pub(crate) struct Track {
    base: usize,
    text: Box<[u8]>,
    pub marks: Vec<(usize, usize, Mark)>,
}

impl Track {
    /// A record of reading `text`, whose lines the reader gets as slices.
    pub(crate) fn new(text: &str) -> Self {
        Track {
            base: text.as_ptr() as usize,
            text: text.as_bytes().into(),
            marks: Vec::new(),
        }
    }

    /// Where `s` begins in the text, if it is a slice of it.
    pub(crate) fn offset(&self, s: &str) -> Option<usize> {
        let at = (s.as_ptr() as usize).checked_sub(self.base)?;
        (at + s.len() <= self.text.len()).then_some(at)
    }

    /// The offset of the line feed after the line `s`, if `s` reaches it.
    pub(crate) fn line_end(&self, s: &str) -> Option<usize> {
        let end = self.offset(s)? + s.len();
        (self.text.get(end) == Some(&b'\n')).then_some(end)
    }

    pub(crate) fn mark(&mut self, from: usize, to: usize, mark: Mark) {
        if from >= to {
            return;
        }
        match self.marks.last_mut() {
            Some((_, end, m)) if *end == from && *m == mark => *end = to,
            _ => self.marks.push((from, to, mark)),
        }
    }

    /// Marks the slice `s` of the text.
    pub(crate) fn mark_slice(&mut self, s: &str, mark: Mark) {
        if let Some(at) = self.offset(s) {
            self.mark(at, at + s.len(), mark);
        }
    }
}

/// Phrasing as the inline reader reads it and, when spans are recorded, the
/// offset in the notation of each of its characters (`None` for one the
/// block reader added, such as the backslash of an escape).
#[derive(Debug, Clone, Default)]
pub(crate) struct Src {
    pub text: String,
    /// Empty when nothing is recorded.
    pub at: Vec<Option<usize>>,
}

impl Src {
    /// `s` as it is; `track` knows where it came from.
    pub(crate) fn of(s: &str, track: Option<&Track>) -> Src {
        let at = match track {
            None => Vec::new(),
            Some(t) => match t.offset(s) {
                Some(o) => s.char_indices().map(|(b, _)| Some(o + b)).collect(),
                None => vec![None; s.chars().count()],
            },
        };
        Src {
            text: s.to_string(),
            at,
        }
    }

    /// `s` as text the inline reader reads as it is written: a backslash
    /// before every ASCII punctuation character (§5.12).
    pub(crate) fn literal(s: &str, track: Option<&Track>) -> Src {
        let from = track.and_then(|t| t.offset(s));
        let mut out = Src {
            text: String::with_capacity(s.len() * 2),
            at: Vec::new(),
        };
        for (b, ch) in s.char_indices() {
            if ch.is_ascii_punctuation() {
                out.text.push('\\');
                if track.is_some() {
                    out.at.push(None);
                }
            }
            out.text.push(ch);
            if track.is_some() {
                out.at.push(from.map(|o| o + b));
            }
        }
        out
    }

    /// Appends `other`.
    pub(crate) fn push(&mut self, other: Src) {
        self.text.push_str(&other.text);
        self.at.extend(other.at);
    }

    /// The lines joined by line feeds, each with the offset of the line feed
    /// that followed it in the notation.
    pub(crate) fn join(lines: Vec<(Src, Option<usize>)>, track: Option<&Track>) -> Src {
        let mut out = Src::default();
        let n = lines.len();
        for (k, (line, lf)) in lines.into_iter().enumerate() {
            out.push(line);
            if k + 1 < n {
                out.text.push('\n');
                if track.is_some() {
                    out.at.push(lf);
                }
            }
        }
        out
    }

    /// Without the spaces and tabs at either end.
    pub(crate) fn trim(&self) -> Src {
        let sp = |c: char| c == ' ' || c == '\t';
        let chars: Vec<char> = self.text.chars().collect();
        let start = chars.iter().position(|&c| !sp(c)).unwrap_or(chars.len());
        let end = chars.iter().rposition(|&c| !sp(c)).map_or(start, |e| e + 1);
        Src {
            text: chars[start..end].iter().collect(),
            at: self
                .at
                .get(start..end)
                .map(<[_]>::to_vec)
                .unwrap_or_default(),
        }
    }
}
