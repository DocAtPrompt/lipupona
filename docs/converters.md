# Converters — work plan

**Status: all four steps of §5 are built** (`converters/md2lipupona`,
flavours `commonmark`, `gfm` and `obsidian`; `converters/html2lipupona`,
modes *export* and *article*). The command `lipupona` (`cli/`) runs both:
`from-md` and `from-html` (`--article`), with `--document` for a whole
document whose head (SPEC §3.7) it writes from the returned values — a
Markdown front matter's keys become the head's names, a list one entry per
item —, on a file or on a
folder (with a resolver for wikilinks, §2). **Informative, not part of the
format.** The converters use the core library (reader, writer,
canonicaliser). Nothing here changes `SPEC.md`.

Two tools:

- **md2lipupona** — Markdown to a lipupona document.
- **html2lipupona** — foreign HTML to a lipupona document, in two modes:
  *export* and *article*.

Both are libraries first and command-line tools second, so that an
application (such as konsona) can call them and show their report.

---

## 1 · Shared rules

1. **Convert, never adopt.** A converter builds a new tree through the
   library and writes it canonically. Foreign markup never reaches the output
   as it was.
2. **Every loss is reported.** Anything dropped, changed or guessed goes into
   a report, as stable codes plus a human summary (§4). A conversion without a
   report is not finished.
3. **Nothing is thrown away silently.** Stripped regions stay in the report
   *with their content*, so an application can offer to restore them before
   saving.
4. **Reading never converts.** A reader that meets foreign markup passes it
   through and reports it (SPEC §3.3, L5, L6). Converting is always an
   explicit act.
5. **The head belongs to the caller.** Converters return what belongs in a
   head — title, language, creation date, source URL, retrieval time — as
   values, and the caller writes it. They produce `body` content only.
6. **Paths are the caller's.** Links between documents and image sources
   go through a resolver the caller supplies (a wikilink `[[Eisen]]` needs to
   know where `Eisen` lives). Without one, links stay as written and are
   reported as unresolved. Downloading external images is the caller's
   decision.
7. **Tested like the rest.** Cases go to `tests/converters/<tool>/<case>/`
   with `input.md` or `input.html`, `expected.html` and `report.txt`, under
   the conventions of `tests/README.md`.

---

## 2 · md2lipupona

**Parser:** `pulldown-cmark` (CommonMark plus the GFM extensions).

**Flavours**, selected by the caller:

| Flavour | Adds |
|---|---|
| `commonmark` (default) | — |
| `gfm` | tables, strikethrough, task lists, footnotes, autolinks |
| `obsidian` | `gfm` plus wikilinks and embeds, `==highlight==`, callouts with title and folding, `#tags`, YAML frontmatter |

**Mapping:**

| Markdown | lipupona | Report |
|---|---|---|
| `#`–`######` | `h1`–`h4` | `heading-level-reduced` for 5 and 6 |
| emphasis, strong, strikethrough, highlight | `em`, `strong`, `s`, `mark` | — |
| code span | `code` | — |
| fenced or indented code | `pre` with the language as class, **no `code` inside** (C8) | — |
| lists, tight or loose | `ul`/`ol` with `li`; a loose item's first paragraph unwrapped (content model *lead*) | — |
| ordered list starting at *n* | `ol start="n"` | — |
| blockquote | `blockquote` | — |
| table | `table`; header row in `thead` | column alignment: `alignment-dropped` |
| image | `img` with `alt` | `image-not-local` for absolute URLs |
| link | `a`; wikilinks through the resolver | `unresolved-link` |
| task list item | `li class="task"`, done `li class="task done"` | — |
| footnote | `sup > a href="#fn-N"` in the text, `ol class="footnotes"` with `li id="fn-N"` at the end | — |
| Obsidian callout `> [!type]± title` | callout per SPEC §7 | `unregistered-callout` |
| `#tag`, frontmatter `tags` | returned to the caller as values | — |
| frontmatter | returned to the caller as key/value pairs | unknown keys listed |
| raw HTML inside Markdown | through html2lipupona, mode *export* | as html2lipupona reports |
| definition list (`Term` / `: definition`, only when asked for) | `dl` with `dt` and `dd` | — |
| hard line break | `br` | — |
| horizontal rule | `hr` | — |

**Decisions for `commonmark` and `gfm`** (2026-10-17), where the table
above leaves a choice:

- **Line breaks.** A soft line break is a space, as CommonMark renders it;
  a hard one is `br`.
- **Code blocks.** The info string's words become the classes of `pre`, as
  the notation reads a fence (SPEC §5.10); the content loses its final line
  end.
- **Titles.** `[text](url "title")` and image titles are dropped
  (`title` is not in the profile): `dropped-attribute`.
- **Images outside the document.** An image whose source is absolute — a
  scheme or a leading `/`, which the profile does not allow in `src` —
  becomes a link to that source with the alt text as its text:
  `image-not-local`. Inside a link, only the alt text is kept, since links
  do not nest. Downloading the image stays the caller's decision.
- **Lists deeper than the profile.** Items below the profile's list depth
  move up into its deepest level, in document order, after the item they
  stood in: `list-depth-reduced`, once per top-level list.
- **Raw HTML.** An HTML block that is a fragment of profile elements and
  attributes is taken as it is — the rule for typed HTML (SPEC §6) — so what
  the author typed stays, classes included. Any other HTML block goes
  through html2lipupona, mode *export*, with its report (since 2026-10-18;
  before, it stayed literal text). A block of comments alone stays
  comments. Inline HTML, which CommonMark hands over one tag at a time,
  follows the typed-HTML rule: an inline start tag of a profile element
  with profile attributes opens that element, its end tag closes it, and
  Markdown between them is read; anything else stays literal text:
  `html-not-taken`.
- **Footnotes** are numbered in the order of their first reference;
  definitions without a reference follow, numbered on. There are no back
  links.
- **Autolinks.** `<https://…>` and `<a@b.org>` (CommonMark) become links;
  GFM's bare `www.` autolinks are not recognised by the parser and stay
  text.
- **Alignment** of table columns is reported once per table.
- **The report** is a list of entries, each a stable code and a short
  message. Cases list the codes, one per line, like `diagnostics.txt`.

**Definition lists** (2026-10-18), in any flavour, only when the caller
asks for them (`Settings::definition_lists`; the CLI's
`--definition-lists`): none of the flavours' own tools shows `: ` as a
definition list, so by default it stays text. When asked for, they are read
as pulldown-cmark reads them (the syntax of PHP Markdown Extra): a term is
a paragraph's lines directly above a `: ` line, joined like any paragraph
(a soft line break is a space, in `obsidian` a `br`), and it is a `dt`;
each `: ` line opens a `dd`, read like a list item, so that a loose
definition's first paragraph leads (content model *lead*, SPEC §5.6.1) and
an empty one stays empty. A line directly below a definition continues it,
even if a `: ` line follows (Markdown Extra needs a blank line before the
next term).

**Decisions for `obsidian`** (2026-10-17), on top of `gfm`:

- **Line breaks.** A soft line break is a `br`, as Obsidian shows it (with
  "strict line breaks" off, its default) and as the notation reads a line
  end.
- **Wikilinks** `[[Ziel]]`, `[[Ziel|Text]]`, `[[Ziel#Abschnitt]]`: the
  resolver gets the target as written (everything before `|`) and returns
  the `href`; the link text is the alias, else the target. Without a
  resolver, or when it finds nothing, the wikilink stays literal text as
  written: `unresolved-link`. Markdown links `[Text](Ziel.md)` stay as
  written; only wikilinks and embeds go through the resolver.
- **Embeds** `![[…]]`: an image (by extension: `png jpg jpeg gif svg webp
  avif bmp`) becomes `img` with the resolved source and, as alt text, the
  alias, else the file name; an alias that is a size (`300`, `300x200`) is
  dropped: `dropped-attribute`. A resolved source that is absolute is
  treated as for Markdown images. Any other embed (a note, a PDF) becomes a
  link to it: `embed-as-link`. Unresolved: literal text, `unresolved-link`.
- **Highlight** `==text==` is `mark`, within one run of phrasing: the
  opening `==` is followed by no whitespace, the closing one follows none,
  and they pair from left to right at the same level of nesting.
- **Callouts** `> [!type]`, `> [!type]- Titel`, `> [!type]+ Titel` follow
  SPEC §7 (the type in lower case); a type outside the registry is kept as
  SPEC §7.4 says: `unregistered-callout`. A callout where the profile allows
  none (in a structure: a list item, a table cell, a blockquote — SPEC §7.2,
  Appendix A) stays a blockquote with its first line as text:
  `callout-as-blockquote`.
- **Tags** `#tag` — after the start or whitespace, of letters, digits, `_`,
  `-` and `/`, not digits alone, outside code and links — stay in the text
  as written and are also returned as values, without `#`, together with
  the frontmatter's `tags`, in the order of first occurrence.
- **Frontmatter** (YAML between `---` lines at the very start) is returned
  as values: `title`, `lang`, `created`, `source`, `aliases`, `tags`, and
  every other key with its value as written, which the report lists:
  `unknown-frontmatter-key`. Flat keys with a scalar or a list are read;
  anything else is returned as written.
- **Comments** `%%…%%` within a paragraph become HTML comments, which
  lipupona keeps hidden as Obsidian does. Comments across blocks stay text.
- **Not read** (they stay text): math `$…$`, inline footnotes `^[…]`, block
  references `^id`, task markers other than `x` and space.

**The CLI's resolver** (2026-10-18). Converting a folder, the command
`lipupona` finds a wikilink's or embed's target among the folder's files as
Obsidian does: by name — a note without `.md` — in any case and in NFC, or
by the end of its path (`Labor/Ferritin`); where several match, the
shortest path wins. The `href` is relative to the page being converted,
with `.md` turned into `.html`; a section after `#` is dropped, since
headings carry no ids. The folder is mirrored: other files are copied,
hidden ones (`.obsidian`) left out. A single file has no resolver.

**Template:** the author's earlier Markdown importer (konsona-2, not public)
already covers most of the Obsidian flavour and has its own loss report. It
is a starting point to be adapted, not copied.

---

## 3 · html2lipupona

### 3.1 Modes

| Mode | For | Strips |
|---|---|---|
| **export** | HTML written by an editor: Word, Google Docs, Notion, other wikis, static sites | only what is never content: `script`, `style`, `meta`, `link`, `noscript`, `template`, comments |
| **article** | web pages to keep | everything of *export*, then §3.3 and §3.4 |

### 3.2 Mapping (both modes)

| Foreign HTML | lipupona | Report |
|---|---|---|
| `b`, `i` | `strong`, `em` | — |
| `strike`, `del` | `s` | — |
| `ins`, `u`, `small`, `cite`, `q`, `abbr` | their text | `dropped-element` |
| `h5`, `h6` | `h4` | `heading-level-reduced` |
| `section`, `article`, `main`, `header`/`footer` inside an article | unwrapped, or `div` where they group blocks that need grouping | — |
| `div` and `span` without meaning | unwrapped | — |
| `font`, `center` | unwrapped | `dropped-element` |
| `span` whose style is bold, italic or line-through (common in Google Docs export) | `strong`, `em`, `s` | — |
| any other `style` | dropped | `dropped-style` |
| classes and ids from the foreign document | dropped (they refer to a stylesheet that is not there) | `dropped-attribute`, summarised |
| `aside` | **not** a callout; in *article* mode stripped where it lies outside the main content, in *export* mode unwrapped | `stripped-region` / `dropped-element` |
| layout table (one cell, nested tables, `role="presentation"`, no header and mostly blocks) | unwrapped | `layout-table-unwrapped` |
| data table | `table` with `colspan`/`rowspan`; first row of `th` cells becomes `thead` | — |
| `dl` | `dl` (in the profile since 2026-10-18, SPEC §5.6.1) | — |
| `figure`, `figcaption`, `img` | kept | `image-not-local` |
| `iframe`, `video`, `audio`, `embed`, `object` | a link to the source | `embed-as-link` |
| `form` and its controls | dropped | `dropped-element` |
| runs of `br` | paragraphs | — |
| entities | decoded; written per C4 | — |

Meaning that lives only in colour or position — a warning set in red — is
**not guessed**. It is reported (`dropped-style`), and the author decides.

**Decisions for *export* mode** (2026-10-18), where the table leaves a
choice:

- **`u`** keeps its text, `dropped-element` (no meaning is guessed).
- **`aside`** is never a callout, also with a class like `note`: unwrapped,
  `dropped-element`. A class of a foreign site is not a callout type.
- **ids** stay on a block that a link of the same document points to
  (`href="#…"`); all other ids, and ids on inline elements, are dropped.
- **Sectioning** (`section`, `article`, `main`, `header`, `footer`, `nav`,
  `hgroup`, `address`) and `div` and `span` without meaning are always
  unwrapped, never turned into a `div`: what needs grouping is the author's
  call. Not reported.
- **Meaning in style**, as Google Docs exports it: `font-weight` bold or
  600 and above is `strong`, `font-style: italic` is `em`, a
  `line-through` decoration is `s`, `vertical-align: super`/`sub` is
  `sup`/`sub`. A `b` or `strong` whose style says `font-weight: normal`
  (Google Docs wraps whole documents in one) is unwrapped.
- **Code** in `pre` loses its `code` (C8); a class `language-x` or `lang-x`
  on it becomes the class `x` of the `pre`.
- **Images.** An absolute source is treated as in md2lipupona: a link to
  it with the alt text, `image-not-local`. A `data:` source cannot be
  linked usefully: the alt text stays, and the report keeps the source.
- **Links** without `href`, or with a `javascript:` one, are unwrapped:
  `dropped-attribute`. An inline element that holds blocks (a link around a
  card) is unwrapped around them: `dropped-element`.
- **Forms**: `form`, `fieldset` and `label` are unwrapped; the controls
  (`input`, `button`, `select`, `textarea`, `option`) are dropped:
  `dropped-element`.
- **Text** directly in `body` or in an unwrapped container becomes a
  paragraph; runs of two or more `br` split paragraphs.
- **Paragraphs in items and cells.** The first paragraph of a list item or
  definition leads (as in md2lipupona), and a cell's only paragraph is its
  phrasing: Word puts a `p` into every cell, Google Docs into every item.
  For telling a layout table from a data table, such a cell holds no
  blocks.
- **Anything else** outside the profile is unwrapped, its text kept:
  `dropped-element`.
- **Empty blocks.** A paragraph or heading without text or image — empty
  in the source, or emptied by the conversion (an `h3` that held only a
  button) — is left out without a report; what emptied it is reported
  already.
- **Definition lists out of shape.** A `dl` keeps the profile's form,
  groups of one or more `dt` followed by one or more `dd`. A `dt` that no
  `dd` follows becomes a paragraph with its text in `strong`, as a browser
  shows it (MediaWiki's `;Term`); `dd` that no `dt` precedes give their
  content as blocks in their place (MediaWiki's `:` indentation). Each such
  repair is `dropped-element`; well-formed groups stay a `dl`.
- **Parts without their structure** — `tr`, `td`, `th`, `thead`, `tfoot`,
  `caption` without a table, `li` without a list, `dt`/`dd` without `dl`,
  `summary` without `details`, `figcaption` without `figure` — are
  unwrapped: `dropped-element`. CommonMark cuts an HTML block at a blank
  line, so a table typed in Markdown with blank lines arrives in pieces. A
  table without any cell is dropped.
- **Never content** (`script`, `style`, `meta`, `link`, `noscript`,
  `template`, comments, the head) is removed without a report.
- **The head**: `title` and the `lang` of `html` are returned as values.
- **Summaries.** `dropped-element`, `dropped-attribute` and `dropped-style`
  are reported once per element, attribute or style property name, with a
  count; every other code once per occurrence. Attributes and styles are
  reported only on elements that are kept: an unwrapped element's go with
  it.
- HTML that is already lipupona's own — a document of the profile — is not
  for this converter: `canon` reads it as it is.

### 3.3 Article mode, step 1: structural removal

Reliable, because it rests on declared meaning:

- the page's own `nav`, `header`, `footer`;
- elements with the roles `navigation`, `banner`, `contentinfo`, `search`
  (an `aside`, `role="complementary"`, goes with step 2: an info box in the
  article stays);
- hidden elements (`hidden`, `aria-hidden="true"`);
- `form` and its controls.

### 3.4 Article mode, step 2: extracting the main content

Heuristic, after the Readability approach used by Firefox's reader view:
score blocks by text density, link density and class or id names (`ad`,
`sidebar`, `related`, `comments`, `share`, `newsletter` …), keep the
best-scoring content block and its relevant neighbours.

- Ad containers can additionally be matched with the cosmetic selector lists
  of ad blockers (EasyList). **Check the licence before bundling a list** —
  this repository is MIT and CC BY 4.0, and filter lists carry their own
  licences.
- Rust ports of Readability exist; which one is fit for purpose is to be
  evaluated, not assumed.
- **The heuristic will be wrong on specialist pages**: a dosage table full of
  links looks like navigation, an info box like an advert. That is why every
  stripped region stays in the report with its content (§1 rule 3), and why
  *export* mode never uses this step.

**Decisions for *article* mode** (2026-10-18). The user decided the
frame — a report entry carries the stripped content, the heuristic is the
converter's own (no Readability port, no ad-block lists, so no licence to
check) — and, after a trial on twelve real web pages, the details below.

- **The report.** Every stripped region is one `stripped-region` entry:
  the message names the element (`nav`, `div#comments`, `div.share`,
  `div[role=navigation]`) and the start of its text; the entry's content is
  the region converted by the rules of *export* mode, as canonical HTML, so
  that an application can put it back. What the conversion of a stripped
  region reports itself is not reported. A region without text or image is
  removed without an entry (a hidden icon).
- **Step 1** (§3.3) strips, in this order of looking:
  - `nav` anywhere; `header` and `footer` except inside `article` (an
    article's own header holds its title and byline, and is unwrapped as
    in *export*);
  - elements with `role` `navigation`, `banner`, `contentinfo` or
    `search`;
  - elements with `hidden` or `aria-hidden="true"`;
  - `form`, except a form that holds `main`, `article` or `h1` (some
    frameworks wrap the whole page in one): that one is unwrapped as in
    *export*.
- **Step 2** (§3.4) chooses the main content, on a page that holds at
  least 500 characters of text after step 1, whitespace not counted (on a
  shorter one there is too little to tell, and step 2 and the cleaning do
  nothing):
  - Declared meaning first: the only `main`, else the only `article`.
  - Otherwise by score, after Readability. Each paragraph-like block —
    `p`, `pre`, and a `td`, `blockquote` or `div` that holds no block —
    with at least 25 characters of text scores 1, plus 1 per comma, plus 1
    per 100 characters up to 3. The score goes fully to its nearest
    candidate ancestor and half to the next; candidates are `div`,
    `section`, `article`, `main`, `td`, `blockquote` and the body. A
    candidate starts from its kind (`div` +5; `td`, `blockquote` +3) and
    its class and id: +25 for a positive name (`article`, `body`,
    `content`, `entry`, `main`, `page`, `post`, `text`, `blog`, `story`),
    −25 for a negative one (`ad`, `banner`, `comment`, `comments`,
    `footer`, `menu`, `nav`, `newsletter`, `promo`, `related`, `share`,
    `sidebar`, `social`, `sponsor`, `widget`); a name counts when it is a
    whole word of the class or id, split at `-` and `_`. The final score is
    multiplied by one minus the link density (the share of the text that is
    in links).
  - The best candidate is the main content — unless three others score at
    least three quarters of it: then the nearest ancestor that holds three
    of them (content split into sections). While the main content is the
    only element in a candidate, that candidate is.
  - No block scores (a page of lists or tables only): nothing is stripped
    in step 2.
  - What is *content-like* stays beside it, on its own level and on the
    level of each of its ancestors: an element that is no container
    (`div`, `section`, `article`, `main`, `aside`, `nav`, `header`,
    `footer`, `form`, `table`) — a heading, a paragraph, a list, a figure —
    unless its class or id is negative or at least half of its text is
    links; and a container that holds no block but paragraphs and is prose
    (more than 80 characters, less than a quarter of them in links, its
    class and id not negative) — a lead, a byline. On the main content's
    own level a container also stays when it scores at least a fifth of
    the main content (and at least 10).
  - Everything else is stripped, as the largest regions that hold text:
    the siblings of the main content and of each of its ancestors. A body
    as main content strips nothing. An `aside` is stripped here when it
    lies outside the main content, and stays inside it.
- **Cleaning the main content.** Inside it, a `div`, `section`, `ul`,
  `ol` or `form` is stripped when its class or id is negative and not
  positive. A `div`, `section` or `form` is stripped as well when it holds
  at least three links that make up more than half of its text, has fewer
  than ten commas and holds no paragraph of prose (more than 80
  characters, less than half of them in links). A list is never stripped
  for its links — symptoms linked to their articles are content —, and
  tables and paragraphs are never stripped there — a dosage table full of
  links stays.
- **The title.** The page's only `h1` always stays, as the first block of
  the content, also when it stood in a region that is stripped (a page
  header); the region is reported without it. The title of the page is
  also returned as a value in both modes.

### 3.5 Limits

html2lipupona works on the HTML it is given. Content that a page builds with
script after loading is not in that HTML; getting it needs a browser (a web
clipper) and is out of scope here.

---

## 4 · Report codes

Shared with the diagnostics of SPEC §9.3, plus:

| Code | Meaning |
|---|---|
| `dropped-element` | an element outside the profile; its text was kept |
| `dropped-attribute` | attributes removed (summarised per kind) |
| `dropped-style` | a style removed; meaning may have been in it |
| `stripped-region` | a region removed as navigation, advert, sidebar, comments; content kept in the report |
| `layout-table-unwrapped` | a table used for layout was unwrapped |
| `embed-as-link` | an embedded object became a link |
| `alignment-dropped` | table column alignment from Markdown not kept |
| `list-depth-reduced` | list items below the profile's list depth moved up into its deepest level |
| `callout-as-blockquote` | a callout where the profile allows none kept as a blockquote |
| `unknown-frontmatter-key` | a frontmatter key that has no place in a head; returned with its value |
| `image-not-local` | an image points outside the document's folder |
| `unresolved-link` | a link the resolver could not resolve |

---

## 5 · Order

1. md2lipupona, flavours `commonmark` and `gfm`.
2. md2lipupona, flavour `obsidian`.
3. html2lipupona, mode *export*.
4. html2lipupona, mode *article*.

Each step is finished when its test cases pass and a run over a real corpus
produces documents that satisfy L1–L4 of the specification.

## 6 · Open for review

- (The questions on `u`, `dl` and `aside` and the other rules of *export*
  mode were decided on 2026-10-18; see §3.2.)
