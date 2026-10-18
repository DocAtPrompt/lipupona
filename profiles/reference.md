# Reference profile

**lipupona 0.4.0 — Appendix A**

The reference profile describes a *text document*: prose, lists, tables,
figures, callouts, folding sections. It is not a website. It is the profile of
the first implementation, konsona, and the one the test suite assumes unless a
case names another.

## The head

The head is the same in every profile (SPEC §3.7): `lang` on `html`,
`<meta charset="utf-8">`, at most one `title`, and entries
`<meta name="…" content="…">`.

## Elements

### Blocks

| Element | Content model | Kind |
|---|---|---|
| `h1` `h2` `h3` `h4` | phrasing | — |
| `p` | phrasing | — |
| `summary` | phrasing | first child of `details`, required there |
| `li` | lead | inside `ul` / `ol` |
| `dt` | phrasing | inside `dl` |
| `dd` | lead | inside `dl` |
| `td` `th` | either | inside `tr` |
| `caption` | either | first child of `table` |
| `figcaption` | either | first or last child of `figure` |
| `pre` | text | structure |
| `hr` | void | — |
| `ul` `ol` | `li` only | structure |
| `dl` | groups of one or more `dt`, then one or more `dd` | structure (SPEC §5.6.1) |
| `table` | `caption`?, `thead`?, `tbody`, `tfoot`? | structure |
| `thead` `tbody` `tfoot` | `tr` only | inside `table` |
| `tr` | `th`, `td` | inside `thead` / `tbody` / `tfoot` |
| `blockquote` | blocks | structure |
| `figure` | blocks and `img`, with at most one `figcaption` | structure; `img` stands directly in it (SPEC §5.4.4) |
| `details` | `summary`, then blocks | structure; with class `callout` a container (SPEC §7.2) |
| `div` | blocks | container |
| `aside` | blocks | container |

### Source

`template` — holds the source of something rendered elsewhere in the
document (a diagram source, a template). Browsers do not render it; it is not
indexed as text, carries no `id`, and is canonicalised like all other
content. Its content model is *blocks* or *text*, as the implementation
declares per use.

### Inline

`strong em mark code s sup sub kbd a span time` — phrasing.
`br img` — void.

## Limits

| Limit | Value |
|---|---|
| Container depth | 2 (for example columns: one parent, one child per column) |
| List depth | 3 (`ul`, `ol` and `dl` count) |
| Deepest heading | `h4` |
| Pipe grid columns | 6 |
| Nesting depth | 256 elements, as in every profile (SPEC §3.4) |

## Attributes

Allowed on body elements:

`id class lang href src alt width height colspan rowspan datetime start open role`

with these restrictions:

| Attribute | Restriction |
|---|---|
| `id` | only on blocks — a link target inside a paragraph is not a stable anchor |
| `role` | only on `div`, only the value `note` (callouts of core weight) |
| `open` | only on `details`, boolean, written `open=""` |
| `start` | only on `ol`; omitted when 1 — canonicalisation removes `start="1"` |
| `href` `src` | percent-encoded (C12); `src` never absolute: no scheme, no leading `/` |

Never allowed: `style`, event attributes (`on…`), `contenteditable`, `tabindex`.

Reserved per implementation: `data-<name>-*` (§11). konsona uses
`data-konsona-*`; the test suite uses `data-test-*`. Such attributes of
another implementation are kept and not reported.

## Callout registry

The five core types of §7.1, nothing else:

| Type | Weight | Element |
|---|---|---|
| `note` | core | `div role="note"` |
| `tip` | aside | `aside` |
| `important` | core | `div role="note"` |
| `warning` | core | `div role="note"` |
| `caution` | core | `div role="note"` |

## Islands

The reference profile declares one kind: an element carrying
`data-<name>-rendered` (`<name>` the implementation's, `test` in the test
suite), which names the renderer that produced it — for
example the SVG beside a diagram source. Its content runs through a whitelist
of its own: no `script`, no event attributes, no `foreignObject`, no reference
to another host, no `javascript:`, no `@import`. An island that breaks it is
reported as `island-rejected`.

## Not included — with the reason

Every one of these can be added later without breaking a document; that is
what profiles are for.

| Element | Reason |
|---|---|
| `section` | a `div` with a class does the same, and containers are limited anyway |
| `colgroup` `col` | column widths go through classes; `col` would need a line kind of its own in the table notation |
| `dl` `dt` `dd` | a third list grammar; a two-column table does it until it hurts |
| `abbr` | its title shows only on mouse hover — invisible on a phone |
| `del` `ins` `small` `u` `q` `cite` | `s` covers the one case that really occurs; the rest are classes |
| `audio` `video` | not needed for text documents yet |
| `h5` `h6` | four levels are enough; deeper notation input is reduced with a diagnostic (SPEC §6), deeper HTML is passed through and reported (SPEC §3.3) |
| `script`, `iframe`, `form`, `input` | not a document |

**Anything that does not fit the profile — formulas, musical notation, circuit
diagrams — comes as an island with source and rendering, not as an extension
of the element list.**
