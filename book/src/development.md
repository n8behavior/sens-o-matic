# Development

The `justfile` is the single source of truth for the build logic: the same
recipes run on your machine and in the CI. `just` with no argument lists them.

Before each push, run `just check`:

```makefile
{{#include ../../justfile:check}}
```

The CI job `Check` runs `just check`, then the Hurl API suite (see
[Testing](testing.md)). `just fix` repairs the formatting and the clippy
warnings that it can repair. `just setup-hooks` installs a pre-push git hook
that runs `just check` before every push.

`just ci` watches the latest CI run of the current branch.

## Read the docs

`just book` is the one command to read the docs. It builds the Rust docs of
the crate (rustdoc), starts the server, serves the book and the Rust docs, and
opens the browser at the book. The book links to the Rust docs and to Swagger
UI. Ctrl-C stops all of it: no server and no `mdbook` stays behind. `just book
false` does not open the browser, and where there is no browser the recipe
does not fail.

The book and the Rust docs are one site: the book is at the root and the Rust
docs are in `rustdoc/`, linked by relative paths. They share one port. Swagger
UI is a page of the server, so it has the port of the server. The ports are
variables of the `justfile`:

```makefile
{{#include ../../justfile:site}}
```

The other recipes that build or check the docs have a name that says so:

| Recipe       | What it does                                                    |
| ------------ | --------------------------------------------------------------- |
| `book-build` | Builds the book alone into `book/book`. No serve.               |
| `site`       | Writes the site folder (`site_dir`): the book and the Rust docs. |
| `doc-test`   | Builds the Rust docs with warnings denied. `just check` runs it. |
| `book-test`  | Tests the book and guards it (see below). `just check` runs it.  |

`site` is the one recipe that writes the layout of the site, and `just book`
serves that layout. The links to the Rust docs work in the site, not in the
book alone that `book-build` writes. The links to Swagger UI and to the
OpenAPI document work while the server runs. `book-test` fails when those
links name another port than `api_port`.

## The hosted site

The workflow `.github/workflows/pages.yml` hosts the same site on GitHub Pages:
the book at the root and the Rust docs of `main` in `rustdoc/`. Its URL is in
the `README.md`. The workflow calls `just site`, so the layout has one home,
and it installs `mdbook` with the composite action `setup-mdbook`, as the other
workflows do.

```yaml
{{#include ../../.github/workflows/pages.yml:build}}
```

A pull request builds and packs the site, so a broken book or a broken link to
the Rust docs fails before the merge. Only a push to `main` runs the job
`deploy`, the one job with the permissions to write the site:

```yaml
{{#include ../../.github/workflows/pages.yml:deploy}}
```

The hosted site has no server, so its links to Swagger UI and to the OpenAPI
document lead to the machine of the reader (see
[API documentation](api-documentation.md)). The hosted site is the only place
with the Rust docs of the crate: docs.rs cannot build it, because the build
script of `utoipa-swagger-ui` downloads Swagger UI and the sandbox of docs.rs
has no network. The site holds the docs of `main`, which are ahead of the
last release. `Cargo.toml` names the site as `documentation`, so crates.io
shows it in place of the docs.rs link.

Pages needs the source "GitHub Actions" (`build_type=workflow`). That is a
setting of the repository, not of the workflow, and a person with admin rights
sets it once before the first deploy:

```bash
gh api -X POST repos/n8behavior/sens-o-matic/pages -f build_type=workflow
```
