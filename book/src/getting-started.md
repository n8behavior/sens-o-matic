# Getting started

You need Rust 1.93 or newer (edition 2024) and
[just](https://github.com/casey/just).

Build the server:

```bash
just build
```

Start it:

```bash
just run
```

`just run` runs the `main` function of the binary. It sets up the tracing,
creates the application state and the router, and listens on port 3000:

```rust,ignore
{{#include ../../src/main.rs:main}}
```

The API is then at `http://localhost:3000`. The server documents the API
itself: see [API documentation](api-documentation.md).

Run `just` with no argument to list every recipe.
