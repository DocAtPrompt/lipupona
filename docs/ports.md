# Ports and bindings — plan

**Status: planned. Nothing here is built before the specification has been
reviewed (towards 1.0).** What applies now is §1.

## 1 · Constraints on the core, from the start

So that the core can later be compiled to WebAssembly and bound to C and
Python without being rebuilt:

- **No IO in the core.** No file system, no network, no clock, no
  environment. Text goes in; tree, text and diagnostics come out. Reading
  files and walking folders belongs to callers (the test runner, the CLI,
  applications).
- **No threads, no global state** in the core.
- **No panics.** The core returns a result for every input, however broken;
  a panic must never cross a binding. Nesting is bounded, so that no walk
  over the tree can exhaust the stack.
- **Plain data at the boundary.** The public API takes and returns strings
  and simple owned data (tree, spans, diagnostics with stable codes), so that
  it maps cleanly onto C structs, Python objects and JavaScript values.
- **Few, small dependencies**, none that pull in IO or a runtime.
- **The test suite stays data**, never Rust code. The runner is a small
  program over the case files; every binding and every port runs the same
  cases.

## 2 · Order, after review

1. **JavaScript via WebAssembly** (`wasm-bindgen`): browser, Node, editor
   plugins. Reaches the most people.
2. **Python via PyO3** (`maturin`, prebuilt wheels): scripts, data
   processing, language-model pipelines.
3. **C ABI** (`cbindgen`), with a header: the base for any other language.
4. **Go** over the C ABI (cgo) or over the WebAssembly build.

Each binding is finished when it runs the full test suite green.

## 3 · Independent ports

Rewrites in other languages are welcome and are left to whoever needs them.
The test suite is the contract: a port conforms when it passes it. Where a
port and the reference implementation disagree on a case, the specification
decides, and an unclear specification is a bug to report.
