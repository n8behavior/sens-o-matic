# CLAUDE.md

Rules for a session that works in this repository.

## Work

- Run `just check` before each push. It runs `fmt-check`, `lint` (clippy with `-D warnings`) and `cargo test`, the same as the CI job `Check`. It must pass with no warning.
- Do not run `just test-api` in a session: it needs `hurl`, which the session image does not have. The CI job `Check` runs it in the step `Hurl API suite`, after `just check`, with the `hurl` version that the step `Install hurl` of `.github/workflows/ci.yml` pins. `just check` does not need `hurl`.
- The crate declares `rust-version = "1.93"` in `Cargo.toml` and the CI builds with Rust 1.93. Do not use a feature newer than 1.93.
- A commit message has a short subject in the imperative, as in `git log`.

## Issues

- Each "Done when:" line of an issue names a test or a command that you can run. When a line is missing or cannot be run, stop and say why.

## Verify

- A verifier reads the diff and runs the test or command of each "Done when:" criterion. It does not run `just check` again: the CI job `Check` did.

## Pull request

- The pull request links its issue (`riff pr open` writes the line; keep it).
- Merges are squash merges, after `Check` and `riff/verify` pass.
- A release (`just release`, which publishes to crates.io) is a step of a person: a session never does it.
