# CLAUDE.md

Rules for a session that works in this repository. Look up what you need, when you need it; do not read all the docs first. `just` lists the commands, `cargo doc --open` shows the module docs and the function docs, and the book (in `book/`; `just book` builds it) holds the concepts and the usage.

## Work

- Run `just check` before each push. It runs `fmt-check`, `lint` (clippy with `-D warnings`) and `cargo test`, the same as the CI job `Check`. It must pass with no warning.
- The CI job `Check` runs `just test-api` in the step `Hurl API suite`, after `just check`, with the `hurl` version that the step `Install hurl` of `.github/workflows/ci.yml` pins. `just check` does not need `hurl`. The session image, built from `deploy/session/`, has Rust 1.93 and `hurl`, so `just test-api` can run in a session.
- The crate declares `rust-version = "1.93"` in `Cargo.toml` and the CI builds with Rust 1.93. Do not use a feature newer than 1.93.
- A commit message has a short subject in the imperative, as in `git log`.

## Code

- Write idiomatic Rust, so that an expert such as Jon Gjengset reads it and says "yes, this is how I would write it".
- Make invalid states unrepresentable: newtypes for domain values, typestate for lifecycles, exhaustive `match` (no `_` arm on a type of ours), and a trait for a concept, not for a bag of methods.
- Enforce an invariant in the code, not in a comment. The failure message says what broke and why, and names the doc that has the detail.
- Build and test with the standard Rust toolchain: `cargo`, `rustdoc`, `cargo test`. Cargo plugins are fine. Do not build a test approach of our own.

## Documentation

One fact has one home. Detail grows in this order: README, book, module docs, function docs. Before you write a fact, search for it; link to it or include it, never copy it.

- Function and type docs (`///`): the technical detail, with examples that `cargo test` runs.
- Module docs (`//!`): the technical concepts of the module.
- A concept that crosses modules: a test that shows it, and the docs that explain it.
- The book (mdBook): the high-level documentation and the usage. Include code and config with `{{#include path:ANCHOR}}`; never paste them.
- `README.md`: a few commands to get started, then a pointer to the book and to `just`. No command list and no file tree: they change.

## Deploy

A deploy image holds a static binary and nothing else. Prefer `FROM scratch`.

## Discovery

`just` with no argument prints every recipe. Keep the default recipe so.

## Issues

- Each "Done when:" line of an issue names a test or a command that you can run. When a line is missing or cannot be run, stop and say why.
- An issue says where to look (a module, a doc, a test), not what the docs already say.

## Verify

- A verifier reads the diff and runs the test or command of each "Done when:" criterion. It does not run `just check` again: the CI job `Check` did.

## Pull request

- The pull request links its issue (`riff pr open` writes the line; keep it).
- Merges are squash merges, after `Check` and `riff/verify` pass.
- A release (`just release`, which publishes to crates.io) is a step of a person: a session never does it.
