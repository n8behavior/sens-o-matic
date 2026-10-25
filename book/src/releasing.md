# Releasing

GitHub Actions automates a release. When a GitHub Release is published, the
workflow `.github/workflows/release.yml`:

1. validates the tag (`just validate-version`) and runs `just check`;
2. builds a static musl binary (`just build-release`) and packages it
   (`just package`);
3. publishes the crate to crates.io (`just publish`);
4. uploads the binary tarball to the release.

These are the recipes of the `justfile` that the workflow runs:

```makefile
{{#include ../../justfile:validate-version}}
```

```makefile
{{#include ../../justfile:package}}
```

```makefile
{{#include ../../justfile:publish}}
```

## Release checklist

1. Bump the version in `Cargo.toml` and commit:

   ```bash
   # edit Cargo.toml
   git add Cargo.toml Cargo.lock && git commit -m "Bump version to X.Y.Z"
   ```

2. Cut the release:

   ```bash
   just release
   ```

   The recipe runs `just check`, validates the packaging with
   `just publish-dry-run`, tags the current commit with a signed `vX.Y.Z` tag,
   pushes, and creates the GitHub Release. GitHub Actions does the rest.

```makefile
{{#include ../../justfile:release}}
```

## Manual publish

To publish outside the workflow, give `just publish` the token in the
environment:

```bash
CARGO_REGISTRY_TOKEN=$(passage show cargo/registry-token) just publish
```

## Token management

| Location                             | Purpose                               |
| ------------------------------------ | ------------------------------------- |
| `passage` (local)                    | Encrypted token for manual publishing |
| GitHub secret `CARGO_REGISTRY_TOKEN` | Token used by CI release workflow     |

To rotate the crates.io API token:

1. Generate a new token at [crates.io](https://crates.io/settings/tokens)
2. `passage edit cargo/registry-token`
3. `gh secret set CARGO_REGISTRY_TOKEN --body "$(passage show cargo/registry-token)"`
