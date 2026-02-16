# Default recipe: run checks
default: check

# Run all checks (format + clippy + tests) — matches CI
check: fmt-check lint test

# Lint with clippy (warnings are errors)
lint:
    cargo clippy --all-targets -- -D warnings

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

# Build release binary (static musl)
build-release:
    cargo build --release --target x86_64-unknown-linux-musl

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
