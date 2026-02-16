# Sens-O-Matic

A backend API for coordinating spontaneous meetups with friends. Turn "Who's up
for drinks?" into an actual hangout with minimal friction.

## Overview

Sens-O-Matic streamlines group coordination by collecting structured
availability, finding time overlaps algorithmically, and reducing
back-and-forth messaging. See [docs/SPEC.md](docs/SPEC.md) for the full
application specification.

## Installation

Install from [crates.io](https://crates.io/crates/sens-o-matic):

```bash
cargo install sens-o-matic
```

## Building

Requires Rust 1.85+ (edition 2024).

```bash
cargo build
```

## Running

Start the server on port 3000:

```bash
cargo run
```

The API will be available at `http://localhost:3000`.

## API Documentation

Interactive API documentation is available via Swagger UI:

```
http://localhost:3000/swagger-ui/
```

OpenAPI spec is served at:

```
http://localhost:3000/api-docs/openapi.json
```

## Testing

### Unit Tests

```bash
just test
```

### API Tests (Hurl)

The project includes comprehensive API tests using [Hurl](https://hurl.dev/).

Run all API tests (builds, starts the server, runs tests, stops the server):

```bash
just test-api
```

If you already have a server running, you can run the tests directly:

```bash
just hurl-test
```

Run a specific test suite (requires running server):

```bash
just hurl entities
just hurl flows
```

Test categories:

| Directory                   | Description                                                   |
| --------------------------- | ------------------------------------------------------------- |
| `tests/hurl/entities/`      | CRUD operations for users, groups, pings, responses, hangouts |
| `tests/hurl/state_machine/` | Ping lifecycle state transitions                              |
| `tests/hurl/flows/`         | End-to-end user flows                                         |
| `tests/hurl/errors/`        | Validation and error handling                                 |
| `tests/hurl/edge_cases/`    | Edge cases (no responses, cancellations, no overlap)          |

## Project Structure

```
src/
├── main.rs              # Server entry point
├── lib.rs               # Library exports
├── router.rs            # Route definitions with OpenAPI
├── state.rs             # In-memory state management
├── state_machine.rs     # Ping lifecycle transitions
├── matching.rs          # Time overlap algorithm
├── models/              # Domain types
│   ├── user.rs
│   ├── group.rs
│   ├── ping.rs
│   ├── response.rs
│   ├── hangout.rs
│   └── error.rs
└── handlers/            # API endpoints
    ├── users.rs         # User management
    ├── groups.rs        # Group management
    ├── pings.rs         # Ping lifecycle
    ├── responses.rs     # Ping responses
    └── hangouts.rs      # Hangout management
```

## Documentation

- [docs/SPEC.md](docs/SPEC.md) - Full application specification
- [docs/api.yaml](docs/api.yaml) - OpenAPI specification (design reference)

## Development

This project uses [just](https://github.com/casey/just) as a command runner. The
`justfile` is the single source of truth for build logic — the same recipes run
locally and in CI.

```bash
just check           # run fmt-check + clippy + tests (same as CI)
just fix             # auto-fix formatting and clippy warnings
just setup-hooks     # install pre-push git hook
just ci              # watch the latest CI run for the current branch
just lint            # clippy only (warnings are errors)
just test            # tests only
just build           # build debug binary
just run             # start the server on port 3000
just test-api        # build, start server, run hurl tests, stop server
just hurl-test       # run all hurl API tests (requires running server)
just hurl entities   # run a specific hurl test suite
just fmt             # format code
just fmt-check       # check formatting without changing files
just publish-dry-run # validate crate packaging
```

## Releasing

Releases are automated via GitHub Actions. When a GitHub Release is published,
the workflow validates the tag, runs checks, builds a static musl binary,
publishes to crates.io, and uploads the binary tarball to the release.

### Release checklist

1. Update the version in `Cargo.toml`
2. Run checks and dry-run publish:
   ```bash
   just check
   just publish-dry-run
   ```
3. Commit and tag:
   ```bash
   git add Cargo.toml Cargo.lock && git commit -m "Bump version to X.Y.Z"
   git tag -s vX.Y.Z -m "Release vX.Y.Z"
   ```
4. Push:
   ```bash
   git push origin main --tags
   ```
5. Create the GitHub Release:
   ```bash
   gh release create vX.Y.Z --verify-tag --generate-notes
   ```
6. GitHub Actions handles the rest (build artifacts, publish to crates.io)

### Manual publish

If you need to publish outside the automated workflow:

```bash
CARGO_REGISTRY_TOKEN=$(passage show cargo/registry-token) just publish
```

### Token management

| Location | Purpose |
|----------|---------|
| `passage` (local) | Encrypted token for manual publishing |
| GitHub secret `CARGO_REGISTRY_TOKEN` | Token used by CI release workflow |

To rotate the crates.io API token:

1. Generate a new token at [crates.io](https://crates.io/settings/tokens)
2. `passage edit cargo/registry-token`
3. `gh secret set CARGO_REGISTRY_TOKEN --body "$(passage show cargo/registry-token)"`

## License

This project is licensed under the [MIT License](LICENSE).

## Status

Backend API implementation complete with in-memory storage. No persistence layer yet.
