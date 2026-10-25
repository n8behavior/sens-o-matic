# Releasing

GitHub Actions automates a release. When a GitHub Release is published, the
workflow `.github/workflows/release.yml`:

1. validates the tag (`just validate-version`) and runs `just check`;
2. builds a static musl binary (`just build-release`) and packages it
   (`just package`);
3. publishes the crate to crates.io (`just publish`);
4. uploads the binary tarball to the release;
5. publishes the deploy image to GitHub Container Registry (GHCR).

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

## The deploy image

The job `publish-image` pushes the image of `deploy/server/` to
`ghcr.io/n8behavior/sens-o-matic`. It runs only in this workflow, so only when
a person publishes a GitHub Release: a pull request and a push to `main`
push no image, and the job `Image` of `ci.yml` stays the test of the build. The
image has two tags: the version of the release (`0.1.2` for `v0.1.2`) and
`latest`. A prerelease gets the version tag only. The image is for
`linux/amd64`, as the static binary of `just build-release`, and it carries the
labels `org.opencontainers.image.source` and `org.opencontainers.image.revision`.
The job prints the digest of the pushed image to the summary of the run.

The job builds the image with the recipe `image`, the same recipe that builds
it on a machine, and then tags and pushes it:

```makefile
{{#include ../../justfile:image}}
```

```yaml
{{#include ../../.github/workflows/release.yml:publish-image}}
```

To run the image of a release:

```bash
docker run --publish 3000:3000 ghcr.io/n8behavior/sens-o-matic:latest
```

The first release creates the package. If GHCR makes it private, set its
visibility to public once, in the package settings.

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
