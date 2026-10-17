# lipupona — plain-text notation for profiled HTML

**Version 0.1.0 — draft for public review — 2026-10-17**

*lipu* is a document, a page, a sheet; *pona* is good, simple — and repaired
(toki pona). A lipupona document is an HTML document that is good to read for
people and machines alike, and simple to write.

---

## 0 · Status and conventions

This is a draft. Everything in it is open to review; the rationale for each
decision is given in place or in [Appendix B](#appendix-b--rationale-and-prior-art)
so that a reviewer can disagree with the reason, not only with the rule.

The key words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are to be read as
described in RFC 2119 and RFC 8174 when, and only when, they appear in
capitals. Sections marked *informative* contain no requirements.

Examples show notation in plain code blocks and HTML in blocks marked `html`.
HTML examples show the `body` content only. `⏎` in running text marks a line
break inside notation.

---

## 1 · Motivation and scope

Markdown's lasting strength is a symmetry: people read and write it without
tools, and so do machines. Its weaknesses are as well documented: it started
without a specification, its dialects diverge, its emphasis and list rules
are ambiguous, it has no general way to attach attributes, and anything beyond
its dozen constructs falls back to raw HTML inside the Markdown — with
complicated rules of its own (Appendix B).

HTML has the opposite profile. It can say what a document means — a figure, a
warning, a caption, a column — and every system can display it. Machines read
it unambiguously. What it costs is on the human side: typing tags and reading
past them.

lipupona keeps HTML as the document and moves the convenience into a view:

- **The document is HTML** — a declared subset (a *profile*, §3), written in
  exactly one way (the *canonical serialization*, §4). This is what is stored,
  synchronised, versioned, and read by a browser, a static site generator or a
  language model.
- **The notation is a projection** (§5): a plain-text view of the document
  tree, as easy to type as Markdown, that maps to the tree and back without
  loss. It is generated when a part of the document is opened for editing and
  it is **never stored**.
- **Input is tolerant** (§6): familiar forms from Markdown, Obsidian, Pandoc
  and plain HTML are accepted and normalised. What is displayed afterwards is
  always the one canonical form.

### 1.1 Non-goals

- lipupona is **not a general HTML editor.** Foreign HTML is outside the
  format; a profile decides what is inside.
- It is **not a layout or application language.** No inline styles, no
  scripts in documents. Appearance belongs to stylesheets.
- The notation is **not a file format.** Nothing in this specification
  requires or permits storing notation instead of HTML.

### 1.2 Principles

- **P1 — HTML is the document; the notation is a view.** Switching between
  views never translates the document, so it cannot lose anything.
- **P2 — Accept generously, display canonically.** Several inputs may produce
  the same tree. Exactly one notation is displayed for it.
- **P3 — One generic mechanism reaches every element.** The element line
  (§5.4) and the inline long form (§5.5.1) can express any element of the
  profile. Short forms and markers are a convenience on top, never a
  precondition.
- **P4 — Meaning in the document, appearance in the stylesheet.** Classes name
  what something *is* (`warning`, `dosage`), not how it looks (`red`).
- **P5 — A document is complete without script.** Removing the stylesheet MAY
  remove appearance; it MUST NOT remove meaning. Extensions that store only a
  recipe for content (§11) are the declared exception.
- **P6 — One escaping rule.** A backslash before ASCII punctuation takes the
  meaning from that character, everywhere (§5.12).
- **P7 — Personal input stays personal.** Users MAY configure input aliases
  (§8.6). Nothing that leaves an editor — saved file, clipboard, export — is
  ever in an alias form.

---

## 2 · Terminology

- **Tree** — the parsed document: elements, attributes, text, comments.
- **Profile** — the declaration of which elements and attributes a document
  may contain and how each is classified (§3).
- **Block** — an element that forms a line structure of its own (`p`, `ul`,
  `table`). **Inline** — phrasing content inside a block (`em`, `a`).
- **Phrasing** — text and inline elements.
- **Structure** — a block with an inner grammar that counts as one block
  (`ul`, `table`, `blockquote`).
- **Container** — an author grouping that holds blocks (`div`, `aside`).
- **Canonical** — written according to §4 and conforming to the profile.
- **Notation** — any text accepted by a notation reader (§5, §6).
- **Display** — the one canonical notation of a tree, produced by a notation
  writer.
- **Reader**, **writer** — the two directions: notation → tree, tree →
  display.
- **Unit** — the part of a document opened for editing at a time (§8.1).
- **Island** — a subtree the profile declares opaque; passed through unread
  (§3.6).
- **Implementation** — software that reads, writes or edits lipupona documents.

---

## 3 · Profiles

### 3.1 What a profile declares

A profile MUST declare:

1. **Elements**, each with exactly one *kind*: block or inline. Elements of
   the document head are owned by the implementation and outside this
   specification.
2. For every block its **content model**, exactly one of:
   - *phrasing* — phrasing only (`p`, `h1`);
   - *blocks* — blocks only (`blockquote`, `div`);
   - *either* — phrasing **or** blocks, not both (`td`); the notation says
     which (§5.4.3);
   - *lead* — blocks, or a run of phrasing followed by zero or more blocks
     (`li`: `<li>a<ul>…</ul></li>`);
   - *text* — character data only (`pre`);
   - *void* — no content (`hr`).
   For inline elements the content model is phrasing or void (`br`, `img`).
3. Which blocks are **structures** and which are **containers**, and for each
   structure its inner grammar (which children in which order).
4. **Limits**: container depth, list depth, the deepest heading level and the
   maximum column count of the pipe grid (§5.9). The last is a limit of the
   display, not of the document: a wider table is valid and is displayed in
   the cell-per-line form.
5. **Attributes**: which are allowed, on which elements, and — where
   restricted — with which values.
6. The **callout registry** (§7).
7. **Islands**, if any, and the attribute that marks them.

Appendix A (`profiles/reference.md`) is the reference profile. Other projects
MAY declare their own; the notation grammar adapts to the declaration and does
not change otherwise.

### 3.2 Constraints the notation relies on

Every profile MUST satisfy these, because the notation is only unambiguous
under them:

- **A container holds blocks, never text directly.** `<div>Text</div>` is not
  in any profile.
- **Structures contain no containers.** A `div` inside an `li` or a `td` is not
  in any profile.
- **Every element has one content model.** The reader never guesses whether
  indented content is a paragraph or phrasing; the element says it, or the
  notation says it (§5.4.3).
- **Indentation means either container depth or list depth, never both at
  once.**

### 3.3 What is not in the profile

An element or attribute outside the profile is not part of the format. An
implementation:

- MUST NOT drop it silently — that would be data loss;
- MUST pass it through verbatim and report it (`out-of-profile`);
- MUST NOT rewrite a document merely because it was read. Canonicalisation
  happens on saving.

Reading never converts: an `h5` in HTML is passed through and reported, not
reduced to `h4`. Reducing is a conversion — done by a reader of notation
(§6) or by a converter, never on reading HTML.

### 3.4 Limits and constraints that are broken

A reader that is given notation producing a tree beyond a limit (a fourth
list level, a third container level) or against a constraint (a callout in a
list item) builds the tree anyway, so that nothing typed is lost, and reports
`limit-exceeded` or `constraint-violated`. Such a block is outside the
profile in the sense of §3.3: it is saved in canonical form — there are no
stored bytes yet — and displayed as a raw block (§5.11) holding that HTML
until it is fixed. A block read from HTML that breaks a limit or a constraint
is passed through as it is.

Each cause is reported with one code: an element or attribute the profile
does not know is `out-of-profile`; elements the profile knows, arranged
against a constraint (text directly in a container, a `summary` that is not
the first child, a callout in a list item), are `constraint-violated`; depth
beyond a limit is `limit-exceeded`.

### 3.5 Attributes in general

`class` and `lang` SHOULD be allowed on every body element, `id` on every
block. `style` and event attributes (`on…`) MUST NOT be part of any profile.

### 3.6 Islands

A profile MAY declare islands — for example a rendered SVG next to its diagram
source. An island is marked by an attribute the profile names, is passed
through unread, is not canonicalised and has no notation of its own; it is
displayed as a raw block (§5.11). The profile SHOULD give islands a whitelist
of their own (no scripts, no event attributes, no external references). An
island that breaks its whitelist is passed through all the same and reported
(`island-rejected`); whether and how it is rendered is the implementation's
decision.

---

## 4 · Canonical serialization

A canonical document has exactly one byte sequence. The rules apply to the
`body`; the head is the implementation's.

*File and characters*

- **C1** UTF-8 without byte order mark. Readers MUST accept a BOM.
- **C2** Line ends are LF; a file ends with exactly one LF. Readers MUST accept
  CRLF and CR.
- **C3** Text, attribute values and paths are in Unicode NFC.
- **C4** Invisible characters are written as references, everything else
  literally: U+00A0 as `&nbsp;`, U+00AD as `&shy;`, U+200B, U+200C, U+200D,
  U+2060 and U+FEFF as `&#xHHHH;` (four upper-case hex digits). This holds
  everywhere: in text, in attribute values and inside `pre`.
  *Reason: a non-breaking space in "z. B." cannot otherwise be told from a
  space in the source.*
- **C5** Minimal escapes: `&` and `<` in text, as `&amp;` and `&lt;`; `&` and
  `"` in attribute values, as `&amp;` and `&quot;`. `>` stays literal.

*Line breaks and indentation*

- **C6** Line breaks and indentation only between blocks: two spaces per
  level, no tabs. **The phrasing content of a block stands on exactly one
  line**, however long. *Reason: whitespace between inline elements carries
  meaning; breaking there would change the text.* An element without block
  children stands on one line with its content. An element with at least one
  block child puts every child on a line of its own, one level deeper — a
  run of phrasing included (content model *lead*):

  ```html
  <li>
    zweiwertig
    <ul>
      <li>Eisensulfat</li>
    </ul>
  </li>
  ```

  A comment between blocks stands on a line of its own at the indentation of
  the blocks; a comment inside phrasing stays where it is.
- **C7** Whitespace in text is collapsed — runs of spaces, tabs and line breaks
  to one space — and removed at the start and end of a block, at the start
  and end of a run of phrasing that borders a block, and next to a `<br>`.
  These rules apply to the text as displayed, across the boundaries of inline
  elements: `<p><strong> a</strong></p>` becomes `<p><strong>a</strong></p>`,
  and of two spaces separated only by tags the first is kept. This is what a
  browser displays anyway.
- **C8** `pre` is verbatim: its content is not indented and not collapsed, and
  it begins immediately after the start tag; the start tag itself is indented
  like any other block. Because HTML parsers drop a line feed directly after
  `<pre>`, content that itself begins with a line feed is written with one
  additional line feed. Verbatim concerns whitespace only: C3 and C4 apply
  inside `pre` as everywhere. **A `pre` contains text only**; a language is a
  class on the `pre`.
- **C9** One blank line before every heading, except a heading that is the
  first child of `body` or of a container; no blank lines anywhere else. A
  blank line is empty: it carries no indentation.

*Tags and attributes*

- **C10** Lower case. Void elements without a slash (`<br>`). Attribute values
  always in double quotes, empty ones too (`alt=""` is a statement); boolean
  attributes are written with an empty value (`open=""`).
- **C11** Attribute order: `id`, then `class`, then all others alphabetically.
  Classes keep their order. Between them stands one space, at the ends none;
  a class that occurs again is dropped after its first occurrence, and an
  empty `class` attribute is dropped.
- **C12** `href` and `src` are percent-encoded. A value with a scheme — an
  RFC 3986 scheme of at least two characters followed by `:` (`https:`,
  `mailto:`; not `C:`) — is left untouched. Otherwise the value is split at
  the first `#` into reference and fragment, and the reference at the first
  `?` into path and query; `#` and `?` stay literal. In path and fragment
  everything except `A–Z a–z 0–9 - . _ ~ /` is encoded; in the query also
  the characters RFC 3986 allows there stay literal
  (`! $ & ' ( ) * + , ; = : @ / ?`). Hex digits are upper case. The value is
  split before it is decoded, and encoding is applied to the *decoded* value
  of each part, so `%20` never becomes `%2520`; a `%` not
  followed by two hex digits is a literal `%` and becomes `%25`.

*Properties*

- **C13** Empty elements are kept. Removing them is an editing decision, not
  a serialization rule.
- **C14** Canonicalisation is idempotent: `canon(canon(x)) == canon(x)`.

---

## 5 · Notation

### 5.1 Status, laws and the two writer rules

The notation is generated from the tree when a unit is opened and parsed back
when it is left. Two laws define it (see also §9):

```
L1   html(notation(h)) == h               for every canonical h, byte for byte
L2   html(notation(html(n))) == html(n)   for every valid notation n
```

Several notations may mean the same HTML. Exactly one of them is the display.
Two rules make the display unique and keep L1:

- **W1 — read back, or fall back.** The writer uses a short form (§5.3), a
  marker (§5.5.2) or the bracket form only when reading it back yields the
  same tree. Otherwise it uses the next more general form: marker → inline
  long form; short form → element line; element line → raw block (§5.11).
  Examples: `Eisen<strong>mangel</strong>` is written `Eisen.strong(mangel)`
  (no markers inside a word); `<sup>a b</sup>` is written `.sup(a b)`; an
  empty `<p></p>` is written `.p`, an empty `<h2></h2>` `.h2`.
- **W2 — escape by reading.** Scanning left to right, wherever the reader
  would begin a construct in what is literal text — markers, block openers,
  tolerated forms of §6, typed HTML — the writer puts a backslash before the
  construct's first ASCII punctuation character and reads on from there with
  the escape in place, until the text reads back as the same tree. So
  `<p>1. Mai</p>` is written `1\. Mai` (a backslash before `1` would be
  literal), and `<p>**a** und **b**</p>` is written `\*\*a** und \*\*b**`:
  after the first backslash the reader would still see `*a*`. Reader and
  writer SHOULD share one predicate for "the reader sees structure here", so
  that the two cannot drift apart.

### 5.2 The line model

Every line is blank, a **block opener**, or a **text line**.

**Block openers** are exactly these, in the notation of this section and the
tolerated forms of §6:

| Opener | Construct |
|---|---|
| `#` … (up to the profile's deepest level, then a space) | heading |
| `- `, `1. ` (any number) | list item (§5.6) |
| `>` at the start of a line | blockquote (§5.7), callout if followed by ` [!` (§7.3) |
| `---` alone | `hr` |
| three or more backticks or tildes | code fence (§5.10) or raw block (§5.11) |
| `\|` at the start of a line | table (§5.9) |
| `.name…` matching §5.4 | element line |
| `:::` and more colons | fenced container (§6) |
| `{` … `}` alone on a line | attribute line (§6) |
| `<!--` | comment (§5.11) |
| `<name` of a profile block element | typed HTML block (§6) |
| `#####` and deeper than the profile | heading, reduced (§6) |

Everything else is a text line. Whitespace at the start of a text line that
is not part of indented content (an element line's, a list item's) is not
content (C7): `  Text` at the top level is the paragraph `Text`.
**Paragraphs** follow three rules:

1. A text line after a text line belongs to the same paragraph; the line break
   between them is a `<br>`.
2. A blank line ends the paragraph.
3. A block opener ends the paragraph, whether or not a blank line came first.

**Every other construct says how it continues and ends:** a heading, an `hr`
and an attribute line are one line each; a list by §5.6; a blockquote or
callout as long as lines begin with `>`; a table as long as lines begin with
`|`; a fence, a fenced container, a comment and a typed HTML block until their
closing line, blank lines inside included; an element line by the indented
lines below it, blank lines between them included.

A fence or a fenced container that is never closed extends to the end of the
unit. A typed HTML block that is never closed is not well-formed and stays
text (§6); a comment that is never closed stays text and is reported
(`unclosed-comment`) — neither may swallow what follows.

So `## Dosis⏎Täglich 100 mg.` is a heading and a paragraph, and

```
Die Anämie ist häufig.
Sie betrifft vor allem Frauen.

Bei Erwachsenen 100 mg täglich.
```

```html
<p>Die Anämie ist häufig.<br>Sie betrifft vor allem Frauen.</p>
<p>Bei Erwachsenen 100 mg täglich.</p>
```

The writer puts exactly one blank line between two blocks of the same
context. The blank line is empty, inside indented content too: it carries no
indentation. The items of a list and the rows and cells of a table are not
blocks of a context; the writer puts no blank line between them, in the
short form and in the element-line form alike.

### 5.3 Short forms

A short form is the display of an element exactly when the element carries
**nothing beyond what the short form expresses**, its content fits, and W1
holds. Anything else is written as an element line.

| Short form | Element | Expresses besides the element |
|---|---|---|
| plain text | `p` | — |
| `#` … `####` | `h1` … `h4` | — |
| `- ` | `li` in `ul` | — |
| `1. ` | `li` in `ol` | `start` (§5.6) |
| `> ` | `blockquote` | — |
| `> [!type]` | callout (§7.3) | classes `callout TYPE`; `role`, `open` by type and folding |
| `---` | `hr` | — |
| code fence | `pre` | the classes of the `pre` |
| pipe grid / cell-per-line | `table` | `colspan`, `rowspan` by span markers |

### 5.4 The element line

#### 5.4.1 Grammar

```
element-line = "." name [ "#" token ] { "." token } [ "[" attribute { " " attribute } "]" ] [ "(" phrasing ")" ]
attribute    = attribute-name "=" '"' value '"'
token        = one or more Unicode letters, digits, "-" or "_"
```

- `name` MUST be a **body** element of the profile, in lower case. A line that
  does not match completely, or names anything else (`….`, `.NET`, `.meta`,
  `.P`), is text. A void element takes no content, or an empty content
  bracket: `.hr(x)` is text.
- Attributes the profile does not allow, and an `id` on an inline element,
  match the grammar: they are taken into the tree and reported (§3.3, §3.4).
- Inside a value, `\"` is a quote and `\\` a backslash. The writer puts the
  attributes in the bracket in the order of C11.
- An `id` or class that is not a `token` (`v1.2`, `w-1/2`) is written in the
  attribute bracket: `[id="v1.2"]`, `[class="a w-1/2"]` — the whole class
  attribute then, not part of it. The reader accepts both places; the display
  uses the selector whenever every token fits.
- The content of the element is indented one level (two spaces) below the
  line.

```
.aside#kinder.callout.tip
  Kinder nehmen den Saft lieber gekühlt.
```

```html
<aside id="kinder" class="callout tip">
  <p>Kinder nehmen den Saft lieber gekühlt.</p>
</aside>
```

(This callout carries an `id`, so its display is the element line, not the
short form.)

#### 5.4.2 Phrasing content

Elements with the content model *phrasing* take their indented content as
phrasing. **Inside phrasing there are no element lines:** an indented line
`.span.x(V)` under `.p.lead` is the inline long form, never a nested block.
Several indented lines are joined by `<br>`, as in a paragraph (§5.2).

```
.p.lead
  Eisenmangel ist die häufigste Mangelerscheinung.
```

The reader also accepts the content in brackets on the line itself
(`.p.lead(Eisenmangel …)`); the display indents it.

#### 5.4.3 Either and lead

- *either* — **content brackets** on the element line mean phrasing; indented
  content means blocks:

  ```
  .td(a)              <td>a</td>
  .td                 <td><p>a</p></td>
    a
  ```

- *lead* — the brackets carry the leading phrasing, indented lines the blocks
  after it: `.li(a)` with an indented `- b` is `<li>a<ul><li>b</li></ul></li>`.

#### 5.4.4 An inline element on a line of its own

No profile has inline elements at block level. A block-level line whose
element is inline — `.span.warning(Vorsicht)`, `.img[alt="Röntgen"
src="bild.png"]` — is therefore a **paragraph** containing that element.

The one exception is a structure whose inner grammar admits an inline
element directly, as the reference profile's `figure` admits `img`: there
the line is that element itself, and it counts as a block of its context.
A paragraph holding only that element is then written `.p` with the
element indented below it.

```
.figure
  .img[alt="Röntgen" src="bild.png"]

  .figcaption(Thorax im Stehen)
```

```html
<figure>
  <img alt="Röntgen" src="bild.png">
  <figcaption>Thorax im Stehen</figcaption>
</figure>
```

### 5.5 Inline

#### 5.5.1 The long form

`.element#id.class[attr="value"](content)` — `.span.fachbegriff(Ferritin)`,
`.kbd(Strg)`, `.time[datetime="2026-09-18"](18. September)`. **The long form
requires brackets:** a round bracket for content, a square one for
attributes, or both. Without a bracket the dot is text, so `saude.gov.br`
and `backup.img` stay text. Void elements take empty round brackets when they
have no attributes: `.br()`, and the writer always writes them that way.

The long form is also the way out: where a marker is in the way, write the
element out.

#### 5.5.2 Markers

| Marker | Element |
|---|---|
| `**bold**` | `strong` |
| `*italic*` | `em` |
| `==highlighted==` | `mark` |
| `~~struck~~` | `s` |
| `^super^` | `sup` |
| `~sub~` | `sub` |

**The reader** scans phrasing from left to right. At each position where a
marker character stands:

1. It tries the markers that begin there, **longest first** (`**` before `*`,
   `~~` before `~`).
2. A marker can **open** only if the next character is not whitespace, and —
   for `**`, `*`, `==`, `~~` — the previous character is not a letter or
   digit (**no markers inside a word**).
3. It **closes** at the first later occurrence of the same marker in the same
   scope that is preceded by a non-whitespace character and — for `**`, `*`,
   `==`, `~~` — not followed by a letter or digit. The content between MUST
   NOT be empty. For `^` and `~` the content MUST NOT contain whitespace.
4. If no marker opens there, the character is text and the scan moves on by
   one character.
5. The content is read recursively by the same rules; a marker inside closes
   before its parent can.

Examples: `2*3 und 4*5` is text. `10^9^/l` is a superscript;
`150 × 10^9/l und 4 × 10^9/l` stays text. `**a*` reads as `<em>*a</em>`.
`x^2+y^2` reads as `x<sup>2+y</sup>2` — write `x^2^+y^2^`.

**The writer** uses a marker only under W1 and escapes under W2.

#### 5.5.3 Code spans

A code span opens with a run of *n* backticks and closes at the next run of
exactly *n* backticks; a run without such a partner is literal text. Its
content is literal: no markers, no long forms, no
escapes. If the content both begins and ends with a space and is not all
spaces, one space is removed from each end. The writer chooses *n* one longer
than the longest run of backticks in the content, and pads with a space on
both sides when the content begins or ends with a backtick. (This is
CommonMark's rule.)

#### 5.5.4 Links

`[text](path)`. The text is phrasing. The path is literal — no markers, no
escapes except `\)` and `\\` — and ends at the `)` that balances the opening
one, so `[Eisen](https://de.wikipedia.org/wiki/Eisen_(Element))` works. The
path is shown as stored (C12). Wiki-style link input (`[[Title]]`) is an
editor aid (§8.5); it MUST resolve to a link with a path before the unit is
left, because a title stops being unique as soon as two documents share it.

### 5.6 Lists

`- ` for `ul`, `1. ` for `ol`. Nesting is by indentation, **two spaces per
level**, up to the profile's list depth.

- On input, indentation is rounded down to whole levels, so the three spaces
  one types to align under `1. ` still read as one level; a tab counts as one
  level. The writer always writes two spaces.
- `- a` is `<li>a</li>`. An indented text line under an item continues its
  phrasing (a `<br>`); an indented list under it is a nested list after the
  phrasing (content model *lead*). A blank line followed by indented content
  starts a new block inside the item.
- A blank line between two items does **not** end the list. There is no
  distinction between tight and loose lists.
- The writer writes the real numbers of an `ol`. The number of the first item
  is the `start` attribute, omitted when it is 1.
- **Element-line form.** A list one of whose items begins with a block, or is
  empty, is written as an element line `.ul` / `.ol` with `.li` lines below it.
  So is a list that directly follows another list of the same kind — written
  with the short form, the two would read as one.

### 5.7 Blockquote

`> ` on every line; there is no lazy continuation. A line `>` alone is an
empty line inside the quote. `>` directly followed by text is accepted as
`> `; the writer writes the space. Inside, §5.2 applies one level down.

### 5.8 Callouts

See §7.3.

### 5.9 Tables

Two forms are valid input. The writer chooses by a property of the HTML, not
by the history of the file: the **pipe grid** when every cell is phrasing, no
cell carries an attribute besides the spans the grid expresses, no cell holds
a code span containing `|`, the table has at most the profile's maximum
column count (reference profile: six), and the header rows contain only `th`;
otherwise the **cell-per-line form**.

```
Eisenpräparate
| Präparat    | Dosis  |
|-------------|--------|
| Eisensulfat | 100 mg |
```

- **Caption:** a paragraph of exactly one text line directly above the grid,
  without a blank line, is the `caption`. A paragraph of more than one line
  directly above the grid is a paragraph, and the table has no caption. A
  caption of more than one line or with blocks takes the element line form
  of the table.
- **Header:** all rows above the separator line are header rows (`thead`,
  cells `th`). Without a separator line the table has no `thead`. A header
  cell among the body rows begins with `= `.
- **Separator line:** a cell of one or more `-`, optionally with `:` at
  either end. Colons are accepted and dropped with the diagnostic
  `alignment-dropped`.
- **Rows that do not add up** — of unequal width once spans are counted —
  are taken as typed; no cell is added. Such a table is displayed in element
  lines (see below).
- **Spans:** `^` as the sole content of a cell continues the cell above
  (rowspan), `<` the cell to the left (colspan). A cell reading `< 5` is text;
  a cell whose content really is `<` or `^` takes a backslash. A `|` inside a
  cell's text is `\|`.
- Column widths and alignment are classes, never the separator line. The
  writer pads each column to its widest cell, measured in graphemes of the
  cell as displayed — a `= ` and backslashes included. The separator line
  has as many `-` per column as the column is wide plus two.
- A grid of more columns than the profile's maximum is valid input; it is
  displayed in the cell-per-line form.

The **cell-per-line form** opens with `|===` and the column count, and closes
with `|===`:

```
|=== 3
| Gruppe
| Links
| Rechts
|---
.3+| oral
2+| zweiwertig
| Sulfat
| Gluconat
2+| Fumarat
|===
```

Without the count, two cells of one row could not be told from two rows of
one column. A cell begins at the start of a line with an optional prefix
before the `|`: `2+` colspan, `.3+` rowspan, `2.3+` both, `[name="value" …]`
attributes, `a` for block content indented on the following lines — in that
order. A cell with `a` has nothing after the `|` on its line; its blocks
follow, indented two spaces. `|---` ends the header rows; `= ` marks a header
cell among body rows.
A caption stands on the line above `|===`, as in the grid.

What neither form can say — a `tfoot`, a `td` in a header row, a grid that
does not add up — is written as element lines.

### 5.10 Code

A run of three or more backticks opens a `pre`; a run of the same character,
at least as long, alone on a line, closes it. Everything between is literal,
dedented by the current level. The content of the `pre` is the lines between
the fences joined by line feeds: an empty `pre` is an opening fence directly
followed by its closing fence, and content that ends with a line feed shows
an empty last line. The writer chooses a fence longer than any run of
backticks at the start of a line in the content. The opening fence carries
the classes of the `pre`, separated by spaces (```` ```rust beispiel ````),
the way a Markdown fence carries a language:

````
```rust
fn main() {}
```
````

Tilde fences (`~~~`) are tolerated input (§6).

### 5.11 Raw blocks and comments

A block the notation cannot express is displayed between code fences whose
info string is `{=html}`:

````
```{=html}
<pre id="beispiel">fn main() {}</pre>
```
````

- For a block **outside the profile** (§3.3, §3.4) and for an **island**
  (§3.6), the content is the block's bytes exactly as stored.
- For a block **inside the profile that no other form can say** — a `pre`
  with an `id` — the content is its canonical HTML. (A `pre` holding
  elements, as a Markdown converter writes `pre > code`, is outside the
  profile, because a `pre` contains text only.)
- Either way it comes back unchanged.

*(The `{=format}` info string is MacFarlane's proposal and Pandoc's syntax;
Appendix B.)*

A **comment** needs no fence: between blocks it stands on its own lines and
is separated like a block, with one blank line before and after it; inside
phrasing it stays where it is. It is passed through and reported
(`comment`), and it is never indexed as text.

### 5.12 Escaping

One rule: **a backslash before an ASCII punctuation character takes the
meaning from that character** — `\*`, `\[`, `\#`, `\-`, `\<`, `\|`, `\\`. It
holds at the start of a line as much as in the middle; a paragraph beginning
"- 5 kg abgenommen" and a continuation line that would read as a block opener
are the same case. **Before any other character a backslash is literal**, so
`C:\Users\Erben` is typed as it is. (This is CommonMark's rule.)

A backslash cannot say "this line is not an element line", because there the
dot has to keep its meaning; `\.` makes the dot literal and the line text.

What the writer escapes is decided by W2.

### 5.13 The case that does not fit

`<p>a<br><br>b</p>` would be written as a line, a blank line and a line — and
would read back as two paragraphs. Two adjacent breaks, or a break at the very
start or end of a paragraph, therefore switch the whole paragraph to **one
line** with every break written inline: `a.br().br()b`.

### 5.14 What an editor needs back (informative)

A writer SHOULD return, besides the text, spans that tile it — in order, no
gaps, no overlaps: *markup* (characters that carry structure: `**`, `## `, an
element line except its content), *content*, *break* (a line break that is a
`<br>`), *separator* (a line break that ends a block) and *invisible*
(non-breaking space, soft hyphen, zero-width and private-use characters). An
editor paints every character exactly once; dimming markup is what makes the
notation read like prose.

---

## 6 · Tolerated input

A reader MUST accept the following forms in addition to the notation of §5.
They are never displayed: on leaving the unit, the display is the canonical
form. The writer escapes literal text that would read as one of them (W2).

| Input | Tree | Display |
|---|---|---|
| `__bold__`, `_italic_` (rules of §5.5.2, `_` like `*`) | `strong`, `em` | `**bold**`, `*italic*` |
| `#####`, `######` (deeper than the profile) | deepest heading | `####`; diagnostic `heading-level-reduced` |
| `## Titel {#anker .klasse}` | heading with `id` and class | `.h2#anker.klasse` + indented content |
| `{#id .klasse name="wert"}` alone on the line directly above a block | that block with these attributes | element line |
| `:::name.klasse` … `:::` | the element, content unindented between the fences | element line |
| `:::` … `:::` without a name | `div` | element line |
| `> [!TYPE] Titel`, `> [!type]- Titel`, `> [!type]+ Titel` | callout (§7), type in any case | `> [!type] Titel`, `> [!type]- Titel`, `> [!type]+ Titel` — type in lower case |
| `~~~` and `~~~lang` fences | `pre` | backtick fence |
| ```` ```{=html} ```` with `=html` written without braces | raw block | ```` ```{=html} ```` |
| typed HTML of profile elements, inline or as whole lines | that fragment | its notation |

Details:

- **Fenced containers.** A fence is three or more colons. For nesting, the
  outer fence MUST be longer than the inner one; a closing fence closes the
  innermost open fence of the same length. `::: name` with a space is
  accepted. A name that is not a block element of the profile is read as
  Pandoc reads it, as a class of a `div`: `::: warnung` is
  `<div class="warnung">`.
- **Attribute braces.** Inside `{…}`: `#id`, `.class`, `name="value"`. A bare
  word is not accepted, because Pandoc reads it as a class and the element
  line as an element; a line `{wichtig}` is text. Attributes from braces and
  from an element line on the same block are merged, classes united in the
  order braces, then element line. A conflict — two different values for the
  same attribute — is the diagnostic `attribute-conflict`, and the element
  line wins, because it is what the display shows.
- **Typed HTML.** A typed fragment is parsed as HTML as a whole: inside it,
  character references are decoded and notation does not apply (no markers,
  no backslash escapes). It is taken only if it is well-formed and consists of
  profile elements and attributes; otherwise it stays literal text, with the
  diagnostic `html-not-taken`. A block fragment begins with a start tag at
  the start of a line and extends to its matching end tag, blank lines inside
  included. The writer escapes every `<` in literal text that begins
  something shaped like a tag — `<` followed by a letter, `/` or `!` —
  whether the reader would take it or not, so that reading the display back
  never reports `html-not-taken`: `<p>Ein &lt;blink> Tag</p>` is written
  `Ein \<blink> Tag`. *Outside* typed HTML, `&` is literal text: `&nbsp;` typed in
  notation is six characters (invisible characters are entered with an editor
  command and shown by the *invisible* span).
- **Unknown callout types** are taken as an `aside` with the classes
  `callout <type>` and the diagnostic `unregistered-callout` (§7.4).
- **Markdown pasted** from elsewhere SHOULD be converted by the same rules;
  constructs without an equivalent are reported, not dropped.

The law for this section:

```
L3   for every tolerated input a:  html(display(html(a))) == html(a)
```

---

## 7 · Typed containers: callouts

A callout (admonition) is a container whose class names its type. HTML has no
admonition element; lipupona defines how to build one so that people,
browsers, screen readers and language models read the same thing.

### 7.1 The registry

A profile declares a registry of callout types. Each entry has:

- a **name** — lower-case ASCII, `[a-z][a-z0-9-]*`;
- a **weight** — *core* (part of the main text; it must not be skipped) or
  *aside* (supplementary; it may be skipped).

Icon and colour are **not** part of the registry or the document. They belong
to the stylesheet, selected by the type class. A type has one look; a `note`
coloured like a warning would contradict itself for every reader.

Every profile MUST contain the five core types:

| Type | Weight | Meaning |
|---|---|---|
| `note` | core | information readers should take in even when skimming |
| `tip` | aside | advice that helps doing something better |
| `important` | core | information needed to reach the goal |
| `warning` | core | needs attention to avoid a problem |
| `caution` | core | warns of risks or negative consequences |

*(The five names and their meanings follow GitHub's alerts; Appendix B.)*

A profile MAY add types, for example `dosage` (core) or `example` (aside).

### 7.2 HTML

| Callout | HTML |
|---|---|
| weight *aside* | `<aside class="callout TYPE">…</aside>` |
| weight *core* | `<div class="callout TYPE" role="note">…</div>` |
| folding, any weight | `<details class="callout TYPE">` with `<summary>` as its first child |
| folding, open by default | as above, with `open=""` |

- The marker class `callout` comes first, the type second. Every callout of a
  document is found with one selector, and the type class cannot collide with
  other uses of a word like `note`.
- An optional **title** of a non-folding callout is its first child, a
  `<p class="title">`, holding phrasing. A heading would enter the document
  outline; a `strong` would misuse emphasis as structure. A callout without a
  title has no `p.title`.
- The title of a folding callout is its `summary`. When none is given, the
  type name with an upper-case initial is stored (`Warning`), because a
  `details` without a `summary` is shown by browsers with a generic label.
- Folding a callout of core weight hides content that must not be skipped.
  An implementation SHOULD say so when the author does it.
- **Every callout is a container**, the folding `details.callout` included: it
  counts towards the container depth and MUST NOT stand inside a structure
  (a list item, a table cell). A `details` without the class `callout`
  remains a structure.

### 7.3 Notation

The short form is the display of every callout whose element matches its
weight, whose classes are exactly `callout TYPE` with a registered type, and
which carries no attribute besides those the short form expresses (`role` for
core weight, `open` for folding):

```
> [!warning] Nicht mit Alkohol
> Die Wirkung wird verstärkt.
>
> - Schwindel
> - Benommenheit
```

```html
<div class="callout warning" role="note">
  <p class="title">Nicht mit Alkohol</p>
  <p>Die Wirkung wird verstärkt.</p>
  <ul>
    <li>Schwindel</li>
    <li>Benommenheit</li>
  </ul>
</div>
```

- The first line is `> [!type]`, then optionally `-` (folding, closed) or `+`
  (folding, open), then optionally a space and the title as phrasing — the
  rest of the line, markers included.
- The following `>` lines are the content, read like a blockquote one level
  down.
- Without a title the first line is `> [!tip]`. A folding callout always has a
  `summary`, so its display always shows the title: `> [!caution]+ Caution`.
- A blockquote whose first paragraph really begins with `[!` is written with a
  backslash: `> \[!…`.

Everything that does not meet the conditions — an `id`, a further class, a
weight that does not match the element, an unregistered type — is written as
an element line.

### 7.4 Unregistered types and changed registries

- Input with an unregistered type becomes `<aside class="callout TYPE">` with
  the diagnostic `unregistered-callout`. Its display is the element line until
  the type is registered.
- The writer depends on the registry (short form only for registered types),
  the reader too (element by weight). The stored HTML does not: it describes
  itself, and an export needs no registry.
- A callout whose element does not match its type's weight is reported
  (`callout-weight-mismatch`) whenever it is read, and displayed as an element
  line. This happens in particular when a type's weight changes: existing
  documents no longer match, the implementation reports each and offers the
  conversion; it does not rewrite on reading (§3.3).
- Input aliases for type names MAY be configured (§8.6). They change the
  input, never the stored type.

---

## 8 · Editing behaviour (informative)

These are recommendations for editors. They are informative because editors
differ; they are written down because the notation was designed for them.

### 8.1 The unit

The unit of editing is the smallest block container around the caret — a
paragraph, a list, a table, a callout. Only the open unit is shown as notation
or source; the rest of the document stays rendered. A selection over several
blocks opens the run of blocks at the level they share.

### 8.2 Encapsulated mode

The unit is shown as notation, markup dimmed (§5.14). Tolerated input (§6) is
accepted. The text is parsed **when the unit is left**, not on every
keystroke: while typing it is necessarily invalid for moments (a half-typed
`.fig`), and judging it then would interrupt the writer. Nothing typed may be
lost: what the reader cannot take stays open with a diagnostic until it
parses, and limits or constraints broken by it are handled by §3.4.

### 8.3 Transparent mode

The unit is shown as its canonical HTML source, with the tags dimmed. Edits
keep tags paired:

- **T1** Deleting a start or end tag deletes its partner and keeps the content
  (*unwrap*).
- **T2** Changing a tag name changes the partner at the same time.
- **T3** Typing a complete start tag inserts the end tag, caret between them.
- **T4** Deleting a selection removes the text in it and every element whose
  start and end tag both lie inside it. A tag whose partner lies outside the
  selection stays. The tree remains well-formed.
- **T5** Quotes in attribute values stay balanced.
- **T6** The unit is parsed when it is left, as in §8.2.

Both modes are projections of the same tree. Switching between them never
translates the document, so it is lossless by construction.

### 8.4 Structural keys

- **Enter** in a list item starts the next item; on an empty item it leaves
  the list (or outdents one level).
- **Enter** on an empty last paragraph of a container leaves the container.
  **Arrow down** at the end of the last block of a container moves the caret
  behind it, creating an empty paragraph if there is none.
- A key to move between table cells — Tab where the editor can spare it.

### 8.5 Input aids

Scaffolds such as `!table:3:4` (a three-by-four grid), completion of element
names valid at the caret, and wiki-style link input (`[[`) are editor
features. They produce notation; they are not part of its grammar.

### 8.6 Aliases

A user MAY define input aliases that map to canonical constructs — a German
`> [!hinweis]` for `> [!note]`, a private marker for a frequent class. An editor:

- MUST check an alias against the notation and other aliases before it takes
  effect, and refuse or warn when it shadows a construct;
- MUST NOT write an alias form anywhere outside the open unit — file,
  clipboard, export (P7).

### 8.7 Clipboard

Copying SHOULD put the canonical HTML fragment (`text/html`) and its display
(`text/plain`) on the clipboard. Pasting notation parses it; pasting Markdown
converts it (§6); what pasted foreign HTML becomes is the implementation's
policy — taking it as plain text is the safe default.

---

## 9 · Conformance

### 9.1 Classes

- **Document** — canonical (§4) and within its profile (§3).
- **Notation reader** — implements §5 and §6.
- **Notation writer** — produces the display (§5), including W1 and W2.
- **Editor** — §8 is informative; an editor conforms when its reader and
  writer do and it keeps L5 and L6.

### 9.2 Laws

```
L1   html(notation(h)) == h                      canonical h, byte for byte
L2   html(notation(html(n))) == html(n)          valid notation n
L3   html(display(html(a))) == html(a)           tolerated input a
L4   canon(canon(x)) == canon(x)                 any x
L5   reading a document does not change its bytes
L6   nothing outside the profile is dropped; it passes through and is reported
```

### 9.3 Diagnostics

Implementations report by stable codes. This version defines:

| Code | Meaning |
|---|---|
| `out-of-profile` | element or attribute the profile does not know, passed through |
| `limit-exceeded` | list depth, container depth or heading level beyond the profile |
| `constraint-violated` | profile elements arranged against a constraint of §3.2 or §7.2 (a callout in a list item) |
| `island-rejected` | an island breaks the whitelist of its profile; passed through |
| `alignment-dropped` | colons in a separator line were accepted and dropped |
| `heading-level-reduced` | a heading deeper than the profile was reduced |
| `html-not-taken` | typed HTML not well-formed or not in the profile; kept as text |
| `unregistered-callout` | callout type not in the registry |
| `callout-weight-mismatch` | callout element does not match its type's weight |
| `attribute-conflict` | braces and element line disagree on an attribute |
| `comment` | a comment is present (not indexed, not exported as text) |
| `unclosed-comment` | a comment is never closed |

Implementations MAY add codes with a prefix of their own.

### 9.4 The test suite

The test suite is the normative companion of this text; where they disagree,
one of them has a bug, to be reported. Its format is described in
`tests/README.md`.

---

## 10 · Versioning

The specification uses semantic versioning. A change to the canonical
serialization or to a profile that makes existing documents non-canonical is
a major change. A change to the notation alone never requires migrating a
document, because notation is never stored — only implementations and tests
change.

---

## 11 · Extensions

- Each implementation reserves its own attribute prefix `data-<name>-` and
  MUST NOT use attributes outside it for its own purposes.
- Attributes `data-<name>-*` of another implementation are kept, ordered by
  C11, and not reported: they mean nothing to this implementation and change
  nothing in the document. They are valid on every body element of every
  profile.
- An implementation MAY store content as a recipe — a query, a template —
  that is assembled when the document is viewed. Such a document is not
  complete without the implementation (an exception to P5) and MUST be marked
  as containing recipes. An export SHOULD produce a complete copy.

---

## Appendix A · Reference profile

See [`profiles/reference.md`](profiles/reference.md).

## Appendix B · Rationale and prior art

*Informative.*

**Why HTML, not Markdown, as the document.** John MacFarlane, author of
Pandoc and co-author of the CommonMark specification, lists six features of
Markdown that caused most of its complexity: emphasis, reference links,
indented code blocks, raw HTML, lists and blank lines, and the lack of
attributes [1]. CommonMark needs seventeen rules for emphasis alone and still
leaves cases undecided. Eric Holscher argues against Markdown for
documentation because of its flavours and missing extensibility [2]. From the
other side, Thariq Shihipar (Claude Code team, Anthropic) argues that HTML is
the better output format for language-model agents — tables, SVG, styling,
interaction, sharing [3] — and Simon Willison, commenting, reconsiders his
Markdown default [4]. Shihipar's reason for leaving Markdown's human
convenience behind is that people edit such files less and less themselves.
lipupona takes the opposite premise — people keep writing — and keeps the
convenience in a view instead of in the file.

**How each of MacFarlane's points is answered.**

| Markdown problem [1] | lipupona |
|---|---|
| Emphasis: doubled delimiters, intraword emphasis, 17 rules | one left-to-right algorithm, no markers inside words, long form for the rest (§5.5.2) |
| Reference links depend on definitions elsewhere | none; a link carries its path (§5.5.4) |
| Indented code complicates lists | none; fences only (§5.10) |
| Raw HTML with seven start/end conditions | raw blocks are explicit, ```` ```{=html} ```` as he proposes (§5.11); typed HTML is taken only as a well-formed fragment of profile elements (§6) |
| Lists and blank lines | *deliberately different:* a list opener ends a paragraph without a blank line (§5.2 rule 3). In a live editor the effect is visible at once and fixed with one backslash; a mandatory blank line would cost on every list. |
| No attributes | the element line (§5.4); Pandoc-style braces are tolerated (§6) |

**Why the element line rather than braces.** Pandoc and Djot [5] attach
attributes in braces and use `:::` fences for generic blocks. That is three
syntaxes — braces, fences, bracketed spans — where the element line is one,
for blocks and inline alike, always element first, then class. Braces and
fences are accepted for those who know them (§6).

**Callouts.** The five type names follow GitHub's alerts [6]; the
`> [!type]` input follows Obsidian's callouts [7], including folding with `-`
and `+`. The weights follow GitHub's descriptions: `NOTE` is information
users should know even when skimming, so `note` has core weight; of the five
only `tip` — advice for doing something better — is supplementary.
Obsidian treats `important` as an alias of `tip` and `caution` as an alias of
`warning`; lipupona keeps all five distinct, so an Obsidian document imported
by name may change the look of those two. That warnings are `div role="note"`
and not `aside` follows from the HTML definition of `aside` as content only
tangentially related to its surroundings [8].

**Sub- and superscript without whitespace** follow Pandoc's extensions [9].
In medical text, `10^9/l` occurs twice in one sentence often enough to make
the difference. The cost is `x^2+y^2`, which needs `x^2^+y^2^`.

**Code spans, backslashes** follow CommonMark [11].

**Selector notation** for elements is the convention of Emmet [10].

[1] J. MacFarlane, *Beyond Markdown*, 2018. https://johnmacfarlane.net/beyond-markdown.html
[2] E. Holscher, *Why You Shouldn't Use "Markdown" for Documentation*, 2016. https://ericholscher.com/blog/2016/mar/15/dont-use-markdown-for-technical-docs/
[3] T. Shihipar, *Using Claude Code: The Unreasonable Effectiveness of HTML*, May 2026; examples at https://github.com/ThariqS/html-effectiveness
[4] S. Willison, link post on [3], 8 May 2026, https://simonwillison.net
[5] Djot. https://djot.net
[6] GitHub Docs, *Basic writing and formatting syntax — Alerts*. https://docs.github.com
[7] Obsidian Help, *Callouts*. https://help.obsidian.md
[8] WHATWG, *HTML Living Standard — the aside element*. https://html.spec.whatwg.org
[9] Pandoc User's Guide, *Superscripts and subscripts*; *Raw attribute*. https://pandoc.org/MANUAL.html
[10] Emmet. https://emmet.io
[11] CommonMark Spec — code spans, backslash escapes. https://spec.commonmark.org

## Appendix C · A worked example

*Informative.* Notation **as typed** — the first line uses a tolerated form:

```
## Eisenmangel {#eisenmangel}

Eisenmangel ist die **häufigste** Mangelerscheinung weltweit.
Frauen sind öfter betroffen.

> [!warning] Nicht mit Tee einnehmen
> Gerbstoffe hemmen die Aufnahme.

Eisenpräparate
| Präparat    | Dosis  |
|-------------|--------|
| Eisensulfat | 100 mg |

> [!note]
> Ferritin unter 30 µg/l spricht für leere Speicher.
```

Stored:

```html
<h2 id="eisenmangel">Eisenmangel</h2>
<p>Eisenmangel ist die <strong>häufigste</strong> Mangelerscheinung weltweit.<br>Frauen sind öfter betroffen.</p>
<div class="callout warning" role="note">
  <p class="title">Nicht mit Tee einnehmen</p>
  <p>Gerbstoffe hemmen die Aufnahme.</p>
</div>
<table>
  <caption>Eisenpräparate</caption>
  <thead>
    <tr>
      <th>Präparat</th>
      <th>Dosis</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td>Eisensulfat</td>
      <td>100 mg</td>
    </tr>
  </tbody>
</table>
<div class="callout note" role="note">
  <p>Ferritin unter 30 µg/l spricht für leere Speicher.</p>
</div>
```

Displayed when opened again: everything as typed, except the first line,
which becomes

```
.h2#eisenmangel
  Eisenmangel
```
