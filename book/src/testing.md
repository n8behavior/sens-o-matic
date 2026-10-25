# Testing

## Unit tests

```bash
just test
```

`just test` runs `cargo test`: the unit tests in `src/` and the examples in the
docs of the code. `just check` runs it.

## The Hurl API suite

The API tests are [Hurl](https://hurl.dev/) files in `tests/hurl/`. They need
`hurl` on your machine; `just check` does not. The CI installs `hurl` in the
step `Install hurl` of `.github/workflows/ci.yml`, which pins the version, and
runs the suite in the step `Hurl API suite`, after `just check`.

Run the whole suite. This builds the server, starts it, runs the tests, and
stops the server:

```bash
just test-api
```

When a server already runs, run the tests against it:

```bash
just hurl-test
```

The tests read the address of the server from `tests/hurl/config/local.env`.

### Categories

Each folder of `tests/hurl/` is a category of tests, and its name says what its
files test. Run one category against a running server:

```bash
just hurl entities
just hurl flows
```

`just hurl-test` runs every category. To see the categories, list the folders:
`ls tests/hurl`.
