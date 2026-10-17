# lipupona test suite

The test suite is the normative companion of `SPEC.md`. Where the two
disagree, one of them has a bug — please open an issue.

*Status: 343 cases; the reference implementation passes all of them.*

## Layout

One directory per case:

```
tests/<area>/<case>/
  input.txt          notation as typed                      (kinds tolerance, diagnostic)
  input.html         HTML as found in a file                (kinds canonical, passthrough, diagnostic)
  input.md           Markdown as found in a file            (kind conversion)
  expected.html      canonical body fragment                (kinds roundtrip, tolerance, canonical)
  display.txt        canonical notation of expected.html    (kinds roundtrip, tolerance)
  diagnostics.txt    expected diagnostic codes, one per line (optional)
  report.txt         expected report codes, one per line    (kind conversion, optional)
  case.toml          kind, profile, spec section, note      (required)
```

`<area>` follows the sections of the specification: `serialization`,
`lines`, `short-forms`, `element-line`, `inline`, `lists`, `quote`,
`tables`, `code`, `raw`, `escaping`, `tolerance`, `callouts`, `profile`.

## Kinds

| Kind | What is checked |
|---|---|
| `roundtrip` | `display(expected.html) == display.txt` and `html(display.txt) == expected.html` (L1) |
| `tolerance` | `html(input.txt) == expected.html` and `display(expected.html) == display.txt` (L3) |
| `canonical` | `canon(input.html) == expected.html` and `canon(expected.html) == expected.html` (C1–C14, L4) |
| `passthrough` | `canon(input.html) == input.html`, byte for byte, with the listed diagnostics (L5, L6) |
| `diagnostic` | reading the one input file (`input.txt` or `input.html`) reports the listed diagnostics |
| `conversion` | the converter turns `input.md` into `expected.html` with the codes of `report.txt`; `expected.html` is canonical (`canon` returns it unchanged and reports nothing outside the profile) and satisfies L1 |

In every kind the diagnostics reported must be exactly those listed in
`diagnostics.txt`, in any order, each as often as listed. A case without
`diagnostics.txt` expects none. The same holds for a converter's report and
`report.txt`.

Converter cases live in `tests/converters/<tool>/<case>/` (for example
`tests/converters/md2lipupona/soft-line-break/`) and are run by the
converter's own runner; `docs/converters.md` is their specification.

## Conventions

- All files are UTF-8 without BOM, LF line ends.
- Every file ends with exactly one LF, which is **not** part of the content
  under test.
- A case with `raw = true` in `case.toml` takes its input file byte for
  byte, exempt from the two rules above: BOM, CR and final line ends
  included. This is how C1 and C2 are tested. Its other files follow the
  rules. Git does not
  convert line ends under `tests/` (`.gitattributes`).
- `expected.html` contains `body` content only, no `body` tag.
- `case.toml` keys: `kind`, `profile` (default `reference`), `section`
  (e.g. `"5.5.2"`; for a converter case the section of `docs/converters.md`),
  `note` (one sentence: what the case is about), `raw` (default `false`),
  `flavour` (converter cases of md2lipupona: `commonmark`, the default, or
  `gfm`).
- Implementation-specific names use `test`: the island marker is
  `data-test-rendered`, reserved attributes are `data-test-*`. A runner
  substitutes its own name.
- A case tests one thing. Its name says which:
  `inline/no-marker-inside-word`, `tolerance/heading-braces`.

## License

MIT, see `LICENSE` in this directory. The specification text is CC BY 4.0.
