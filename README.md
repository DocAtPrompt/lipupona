# lipupona

**Plain-text notation for profiled HTML.**

Markdown is easy to type and too small. HTML is expressive and tiring to
type. lipupona keeps HTML as the document and makes it as easy to write as
Markdown:

- **Store HTML** — a declared subset (a *profile*), written in exactly one
  canonical way. Browsers, static site generators and language models read it
  as it is.
- **Type notation** — a plain-text view of the document tree, generated for
  the part being edited and never stored. It maps to the HTML and back without
  loss.
- **Accept what people already type** — Markdown emphasis, Obsidian callouts,
  Pandoc attributes, plain HTML tags — and display it in one canonical form.

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

## Contents

- [`SPEC.md`](SPEC.md) — the specification
- [`profiles/reference.md`](profiles/reference.md) — the reference profile
- [`tests/`](tests/) — the test suite (format defined, cases in progress)
- [`docs/converters.md`](docs/converters.md) — work plan for md2lipupona and html2lipupona

## Status

Version 0.1.0, a draft for review. Reviews, objections and counter-examples
are welcome as issues. The most useful contribution is a case the
specification gets wrong.

The first implementation is konsona, a desktop wiki that stores pages as
HTML files.

*lipu* — document, page; *pona* — good, simple, repaired (toki pona).

## License

Specification text: CC BY 4.0. Test suite: MIT. See [`LICENSE`](LICENSE).
