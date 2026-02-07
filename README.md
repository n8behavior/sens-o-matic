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
cargo test
```

### API Tests (Hurl)

The project includes comprehensive API tests using [Hurl](https://hurl.dev/).

Run all tests:

```bash
hurl --test --variables-file tests/hurl/config/local.env tests/hurl/**/*.hurl
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

## Publishing

To publish a new version to crates.io:

1. Update the version in `Cargo.toml`
2. Ensure all checks pass:
   ```bash
   cargo clippy -- -D warnings
   cargo test
   cargo publish --dry-run
   ```
3. Commit, tag, and push:
   ```bash
   git commit -am "Release vX.Y.Z"
   git tag vX.Y.Z
   git push origin main --tags
   ```
4. Publish:
   ```bash
   CARGO_REGISTRY_TOKEN=$(passage show cargo/registry-token) cargo publish
   ```

The API token is stored encrypted in [passage](https://github.com/FiloSottile/passage) and only decrypted in memory at publish time. No plaintext tokens are written to disk.

## License

This project is licensed under the [MIT License](LICENSE).

## Status

Backend API implementation complete with in-memory storage. No persistence layer yet.
