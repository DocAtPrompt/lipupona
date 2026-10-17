# Converters — work plan

**Status: steps 1 and 2 of §5 are built** (`converters/md2lipupona`,
flavours `commonmark`, `gfm` and `obsidian`); html2lipupona is planned. **Informative, not part of the
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
- **Raw HTML**, until html2lipupona exists, follows the rule for typed HTML
  (SPEC §6): an inline start tag of a profile element with profile
  attributes opens that element, its end tag closes it, and Markdown between
  them is read; an HTML block is taken if it is a fragment of profile
  elements. Anything else stays literal text: `html-not-taken`. Comments
  are kept as comments.
- **Footnotes** are numbered in the order of their first reference;
  definitions without a reference follow, numbered on. There are no back
  links.
- **Autolinks.** `<https://…>` and `<a@b.org>` (CommonMark) become links;
  GFM's bare `www.` autolinks are not recognised by the parser and stay
  text.
- **Alignment** of table columns is reported once per table.
- **The report** is a list of entries, each a stable code and a short
  message. Cases list the codes, one per line, like `diagnostics.txt`.

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
| `aside` | **not** a callout; in *article* mode stripped as a sidebar, in *export* mode unwrapped | `stripped-region` / `dropped-element` |
| layout table (one cell, nested tables, `role="presentation"`, no header and mostly blocks) | unwrapped | `layout-table-unwrapped` |
| data table | `table` with `colspan`/`rowspan`; first row of `th` cells becomes `thead` | — |
| `dl` | a two-column table | `definition-list-as-table` |
| `figure`, `figcaption`, `img` | kept | `image-not-local` |
| `iframe`, `video`, `audio`, `embed`, `object` | a link to the source | `embed-as-link` |
| `form` and its controls | dropped | `dropped-element` |
| runs of `br` | paragraphs | — |
| entities | decoded; written per C4 | — |

Meaning that lives only in colour or position — a warning set in red — is
**not guessed**. It is reported (`dropped-style`), and the author decides.

### 3.3 Article mode, step 1: structural removal

Reliable, because it rests on declared meaning:

- the page's own `nav`, `header`, `footer`, `aside`;
- elements with the roles `navigation`, `banner`, `contentinfo`,
  `complementary`, `search`;
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
| `definition-list-as-table` | a `dl` became a two-column table |
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

- `u` (underline): dropped with its text kept, or a class? It is not in the
  reference profile.
- `dl` as a two-column table, or a reason to add `dl` to a profile?
- Whether `aside` from foreign HTML should ever become a callout (for
  example `aside class="note"` from a documentation site).
