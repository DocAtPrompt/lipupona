# lipupona test suite

The test suite is the normative companion of `SPEC.md`. Where the two
disagree, one of them has a bug — please open an issue.

*Status: format defined, cases to be written.*

## Layout

One directory per case:

```
tests/<area>/<case>/
  input.txt          notation as typed                      (kind tolerance)
  input.html         HTML as found in a file                (kinds canonical, passthrough)
  expected.html      canonical body fragment                (required)
  display.txt        canonical notation of expected.html    (required unless kind = passthrough)
  diagnostics.txt    expected diagnostic codes, one per line (optional)
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
| `passthrough` | `input.html` comes back byte for byte, with the listed diagnostics (L6) |
| `diagnostic` | the listed diagnostics are reported, nothing else |

## Conventions

- All files are UTF-8 without BOM, LF line ends.
- Every file ends with exactly one LF, which is **not** part of the content
  under test.
- `expected.html` contains `body` content only, no `body` tag.
- `case.toml` keys: `kind`, `profile` (default `reference`), `section`
  (e.g. `"5.5.2"`), `note` (one sentence: what the case is about).
- A case tests one thing. Its name says which:
  `inline/no-marker-inside-word`, `tolerance/heading-braces`.

## License

MIT, see `LICENSE` in this directory. The specification text is CC BY 4.0.
