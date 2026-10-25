# Default recipe: list the recipes
default:
    @just --list

# ANCHOR: check
# Run all checks (format + clippy + docs + tests + book) — matches CI
check: fmt-check lint doc test book-test
# ANCHOR_END: check

# Lint with clippy (warnings are errors)
lint:
    cargo clippy --all-targets -- -D warnings

# Build the API docs (warnings, such as a broken intra-doc link, are errors)
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

# Run tests
test:
    cargo test

# Build debug binary
build:
    cargo build

# Run the server
run:
    cargo run

# Run all hurl API tests (requires running server)
hurl-test:
    hurl --test --variables-file tests/hurl/config/local.env tests/hurl/**/*.hurl

# Run a specific hurl test suite (e.g., just hurl entities)
hurl suite:
    hurl --test --variables-file tests/hurl/config/local.env tests/hurl/{{ suite }}/*.hurl

# Run hurl API tests with automatic server lifecycle
test-api: build
    #!/usr/bin/env bash
    set -euo pipefail
    cargo run &
    SERVER_PID=$!
    trap "kill $SERVER_PID 2>/dev/null; wait $SERVER_PID 2>/dev/null || true" EXIT
    for i in $(seq 1 30); do
      if curl -sf http://localhost:3000/health > /dev/null 2>&1; then
        break
      fi
      if ! kill -0 $SERVER_PID 2>/dev/null; then
        echo "::error::Server exited unexpectedly"
        exit 1
      fi
      sleep 0.1
    done
    just hurl-test

# Build the book into book/book
book:
    mdbook build book

# Test the book and guard it against a silent hole (see book/src/how-this-book-includes-code.md)
book-test:
    #!/usr/bin/env bash
    set -euo pipefail
    mdbook test book
    # mdbook exits with success when an anchor or a file is missing, so the
    # guard builds the book in a scratch folder and looks for the symptoms.
    out=$(mktemp -d)
    trap 'rm -rf "$out"' EXIT
    log=$(mdbook build book --dest-dir "$out" 2>&1)
    if grep -qE '^\s*(ERROR|WARN)\b' <<< "$log"; then
      echo "$log" >&2
      echo "::error::mdbook reported an error or a warning while it built the book. A common cause is an include of a file that is missing. See book/src/how-this-book-includes-code.md" >&2
      exit 1
    fi
    if empty=$(grep -rPzl --include='*.html' '<code[^>]*>\s*</code>' "$out"); then
      echo "$empty" >&2
      echo "::error::The pages above have an empty code block. An include names an anchor that its file does not have, and mdbook does not fail on that. See book/src/how-this-book-includes-code.md" >&2
      exit 1
    fi

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Format code
fmt:
    cargo fmt --all

# Auto-fix formatting and clippy warnings
fix:
    cargo fmt --all
    cargo clippy --fix --all-targets --allow-dirty --allow-staged

# Install pre-push git hook that runs checks before pushing
setup-hooks:
    #!/usr/bin/env bash
    set -euo pipefail
    hook=".git/hooks/pre-push"
    cat > "$hook" << 'HOOK'
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Running pre-push checks..."
    just check
    HOOK
    chmod +x "$hook"
    echo "Installed pre-push hook: $hook"

# Watch the latest CI run for the current branch
ci:
    gh run watch --exit-status "$(gh run list --branch "$(git branch --show-current)" --limit 1 --json databaseId --jq '.[0].databaseId')"

# Cut a release: run checks, tag, push, and create GitHub release
release: check publish-dry-run
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    TAG="v${VERSION}"

    # Ensure tag doesn't already exist (version must be bumped)
    if git rev-parse "$TAG" >/dev/null 2>&1; then
      echo "Error: Tag $TAG already exists. Bump the version in Cargo.toml first."
      exit 1
    fi

    # Ensure working tree is clean
    if ! git diff --quiet || ! git diff --cached --quiet; then
      echo "Error: Working tree is dirty. Commit or stash changes first."
      exit 1
    fi

    git tag -s "$TAG" -m "Release $TAG"
    git push origin HEAD --tags
    gh release create "$TAG" --verify-tag --generate-notes
    echo "Released $TAG — GitHub Actions will handle the rest."

# Build release binary (static musl)
build-release:
    cargo build --release --target x86_64-unknown-linux-musl

# Build the deploy image of the server (local only: no registry, no push)
image: build-release
    #!/usr/bin/env bash
    set -euo pipefail
    # The context holds the binary and nothing else: the target folder is
    # too big to send to the docker daemon.
    context=$(mktemp -d)
    trap 'rm -rf "$context"' EXIT
    cp target/x86_64-unknown-linux-musl/release/sens-o-matic "$context/"
    docker build --file deploy/server/Dockerfile --tag sens-o-matic "$context"

# Package release artifact as tarball
package version:
    mkdir -p staging
    cp target/x86_64-unknown-linux-musl/release/sens-o-matic staging/
    cp LICENSE README.md staging/
    tar -czf "sens-o-matic-{{version}}-x86_64-unknown-linux-musl.tar.gz" -C staging .
    rm -rf staging

# Validate that Cargo.toml version matches the given tag
validate-version tag:
    #!/usr/bin/env bash
    set -euo pipefail
    TAG_VERSION="{{ tag }}"
    TAG_VERSION="${TAG_VERSION#v}"
    CARGO_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    if [ "$TAG_VERSION" != "$CARGO_VERSION" ]; then
      echo "::error::Tag version ($TAG_VERSION) ≠ Cargo.toml version ($CARGO_VERSION)"
      exit 1
    fi
    echo "Version match: $CARGO_VERSION"

# Publish to crates.io (token from env: CARGO_REGISTRY_TOKEN)
publish:
    cargo publish

# Dry-run publish
publish-dry-run:
    cargo publish --dry-run
