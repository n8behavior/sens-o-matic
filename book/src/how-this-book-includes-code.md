# How this book includes code

The book never holds a copy of code or of config. A copy goes stale without a
failure; an include does not. The book shows a snippet of the real file with
an `include`:

```text
\{{#include ../../justfile:check}}
```

The path is relative to the page. The word after the colon is an ANCHOR: the
source file marks the snippet with a pair of comments, and the book shows the
lines between them.

```makefile
{{#include ../../justfile:check}}
```

The snippet above is the recipe `check` of the `justfile`. Its markers are
the comments `# ANCHOR: check` and `# ANCHOR_END: check`, and they are in the
`justfile`. In a Rust file the markers are `// ANCHOR: name` and
`// ANCHOR_END: name`.

## A snippet of the crate

The crate already compiles the code in `src/`, so the book does not compile it
again. `mdbook test` builds each Rust block of the book as a standalone
program, and a snippet of `src/` is not one: it uses the types of the crate.
Mark such a block `rust,ignore`:

````text
```rust,ignore
\{{#include ../../src/main.rs:main}}
```
````

## The guard against a silent hole

`mdbook` does not fail on a missing anchor: the snippet is empty, and the
build passes. For a missing file it logs an error and still exits with
success. So `just book-test` does more than `mdbook test`: it builds the book
and fails on a message of `mdbook` at the level `ERROR` or `WARN`, and on a
code block that is empty. `just check` runs `just book-test`, so the CI job
`Check` runs it too.

When you rename or remove an anchor in a source file, the guard names the page
that lost its snippet.
