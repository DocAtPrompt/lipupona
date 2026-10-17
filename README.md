<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo/logo-dark.svg">
    <img src="assets/logo/logo.svg" alt="lipupona" height="64">
  </picture>
</p>

# lipupona

**Type like Markdown. Keep HTML.**

Write HTML almost as easily as Markdown. lipupona is a proposal for a
plain-text notation that maps to HTML and back without loss. You type
something that feels like Markdown; what is stored is clean, canonical HTML
that any browser opens as it is.

```
> [!warning] Nicht mit Tee einnehmen
> Gerbstoffe hemmen die **Aufnahme**.
```

```html
<div class="callout warning" role="note">
  <p class="title">Nicht mit Tee einnehmen</p>
  <p>Gerbstoffe hemmen die <strong>Aufnahme</strong>.</p>
</div>
```

## Why

Markdown is a wonderful way to write: quick to type, pleasant to read as it
is, supported almost everywhere. It stays small on purpose, and for a lot of
what I write it is too small. I want a caption on a table, a warning that is
marked as a warning, a cell that spans three rows, a class that says what a
paragraph *is* — a dosage, a definition, a source. Markdown leaves these out
by design, and that is a fair choice for Markdown. It just does not cover what
my documents need.

HTML has all of it. It can say what a document means — figure, caption,
callout, column — and every browser, static site generator and language model
reads it. And it lasts. An HTML file is a plain text file in an open standard
that browsers have kept readable for more than thirty years. It needs no
particular app, no database and no export step to be read decades from now:
as durable as a `.txt` file, with the structure a `.txt` file cannot hold.

What HTML costs is on the writing side: typing tags, and reading past them.
lipupona is an attempt to remove that cost with as few compromises as
possible.

## How

- **The document is HTML.** A declared subset (a *profile*), written in
  exactly one canonical way, so that the same document always has the same
  bytes — friendly to diffs, version control and sync.
- **The notation is a view.** A plain-text projection of the document tree,
  generated for the part being edited and never stored. It maps to the HTML
  and back without loss.
- **Familiar input is welcome.** Markdown emphasis, Obsidian callouts, Pandoc
  attributes and plain HTML tags are accepted as typed and shown afterwards
  in one canonical form.
- **One mechanism reaches every element.** Anything the short forms do not
  cover is written as an *element line*, `.element#id.class[attr="value"]`,
  so nothing in the profile is out of reach of the keyboard.

| You type | You get |
|---|---|
| `**strong**` `*em*` `==mark==` `~~s~~` `m^2^` `H~2~O` | `strong` `em` `mark` `s` `sup` `sub` |
| `## Heading`, `- item`, `1. item`, `> quote`, a code fence | `h2`, `ul`, `ol`, `blockquote`, `pre` |
| `.p.lead` with the text indented below | `<p class="lead">` |
| `.span.term(Ferritin)` inside a sentence | `<span class="term">Ferritin</span>` |
| `> [!tip] Title`, `> [!caution]- Title` | a callout, a folding callout |
| a pipe grid with a caption line and `^` / `<` span markers | `table` with `caption`, `thead`, `rowspan`, `colspan` |

## Where it comes from

lipupona is a side project. It grew out of konsona, a desktop wiki I am
building for my own notes, with Obsidian as its model: pages as plain files,
written in a light notation, linked to one another. Obsidian and Markdown are
where the good ideas here come from — callouts with `> [!type]`, emphasis
with asterisks, links in brackets. lipupona keeps them for typing and stores
HTML underneath, because that is what my notes needed.

## Status

Version 0.1.0, a draft for review.

- [`SPEC.md`](SPEC.md) — the specification
- [`profiles/reference.md`](profiles/reference.md) — the reference profile
- [`tests/`](tests/) — the test suite, 331 cases in a language-neutral format
- [`docs/converters.md`](docs/converters.md) — work plan for md2lipupona and
  html2lipupona
- [`docs/ports.md`](docs/ports.md) — plan for bindings and ports, and the
  constraints on the core that keep them possible
- `src/` — the reference implementation in Rust: `canon`, `html` (reading
  notation) and `display` (writing it). It passes the whole suite; property
  tests check L1, L2 and L4 on generated documents. `cargo test` runs
  everything; `cargo run --example display -- page.html` shows a page's
  notation.

The first application is konsona, which stores its pages as HTML files.

Reviews, objections and counter-examples are welcome as issues. The most
useful contribution is a case the specification gets wrong — ideally in the
format of [`tests/README.md`](tests/README.md).

## Name

*lipu* — document, page, sheet; *pona* — good, simple, repaired. Both are
words of toki pona.

## License

Specification text, documentation and logo: CC BY 4.0. Test suite: MIT.
Code: MIT or Apache-2.0, at your option. See [`LICENSE`](LICENSE).
