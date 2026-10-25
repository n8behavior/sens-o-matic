# Default recipe: list the recipes
default:
    @just --list

# ANCHOR: site
# The folder of the site: the book at the root, the rustdoc in rustdoc/.
site_dir := "target/site"
# The ports of `just book`. The book and the rustdoc are one site, so they
# share a port. The API port is the one that src/main.rs binds.
site_port := "3001"
api_port := "3000"
# ANCHOR_END: site

# The symlink, in the sources of the book, through which mdbook copies the
# rustdoc into the site (see rustdoc-stage).
rustdoc_link := "book/src/rustdoc"

# ANCHOR: check
# Run all checks (format + clippy + docs + tests + book) — matches CI
check: fmt-check lint doc-test test book-test
# ANCHOR_END: check

# Lint with clippy (warnings are errors)
lint:
    cargo clippy --all-targets -- -D warnings

# Build the rustdoc of the crate; warnings, such as a broken intra-doc link, are errors
doc-test:
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

# Build the book alone into book/book (no rustdoc, no server)
book-build:
    mdbook build book

# Build the rustdoc and link it into the sources of the book. mdbook clears its
# output folder on each rebuild, also in `mdbook serve`, so a rustdoc folder
# put into the output is lost at the next change of a chapter. Files in the
# sources are copied again on each build, so the rustdoc survives there.
[private]
rustdoc-stage:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo doc --no-deps
    doc=$(cd "${CARGO_TARGET_DIR:-target}/doc" && pwd)
    ln -sfn "$doc" "{{ rustdoc_link }}"

# Write the site folder (site_dir): the book at the root, the rustdoc next to it
site: rustdoc-stage
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'rm -f "{{ rustdoc_link }}"' EXIT
    mdbook build book --dest-dir "{{ site_dir }}"
    # The book links to the rustdoc by relative links. Each one must lead to a
    # page of the site, so that the same folder works on a server and on
    # GitHub Pages.
    links=$(grep -rhoE --include='*.html' --exclude-dir=rustdoc 'href="rustdoc/[^"#]+' "{{ site_dir }}" | sed 's/^href="//' | sort -u || true)
    if [ -z "$links" ]; then
      echo "::error::No page of the book links to the rustdoc. The pages link to it with a relative link that starts with rustdoc/. See book/src/development.md" >&2
      exit 1
    fi
    while read -r link; do
      if [ ! -f "{{ site_dir }}/$link" ]; then
        echo "::error::The book links to $link, and the site has no such page. See book/src/development.md" >&2
        exit 1
      fi
    done <<< "$links"

# Serve the book, the rustdoc and Swagger UI, and open the browser (`just book false`: no browser)
book open="true": rustdoc-stage
    #!/usr/bin/env bash
    set -euo pipefail
    api=http://localhost:{{ api_port }}
    book=http://localhost:{{ site_port }}
    server_pid=
    book_pid=
    stop() {
      trap '' INT TERM HUP
      for pid in $book_pid $server_pid; do
        kill "$pid" 2>/dev/null || true
      done
      wait 2>/dev/null || true
      rm -f "{{ rustdoc_link }}"
    }
    trap stop EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM HUP
    # A server that already answers owns the port, and ours would exit.
    if curl -sf "$api/health" > /dev/null 2>&1; then
      echo "::error::Something already answers on $api, the port that src/main.rs binds, so the server of this recipe could not start. Stop that server, such as a \`just run\`. See book/src/development.md" >&2
      exit 1
    fi
    cargo build
    RUST_LOG="${RUST_LOG:-sens_o_matic=warn,tower_http=warn}" cargo run --quiet &
    server_pid=$!
    for _ in $(seq 1 100); do
      if curl -sf "$api/health" > /dev/null 2>&1; then
        break
      fi
      if ! kill -0 "$server_pid" 2>/dev/null; then
        echo "::error::The server exited. See its message above." >&2
        exit 1
      fi
      sleep 0.1
    done
    if ! curl -sf "$api/health" > /dev/null 2>&1; then
      echo "::error::The server does not answer on $api. src/main.rs binds the port; api_port in the justfile must name it. See book/src/development.md" >&2
      exit 1
    fi
    echo "Book:       $book/"
    echo "Swagger UI: $api/swagger-ui/"
    echo "Ctrl-C stops all of it."
    open_flag=
    if [ "{{ open }}" = true ]; then
      open_flag=--open
    fi
    mdbook serve book --dest-dir "{{ site_dir }}" --port "{{ site_port }}" $open_flag &
    book_pid=$!
    wait "$book_pid"

# Test the book and guard it against a silent hole (see book/src/how-this-book-includes-code.md)
book-test:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -e "{{ rustdoc_link }}" ] || [ -L "{{ rustdoc_link }}" ]; then
      echo "::error::{{ rustdoc_link }} is left from a \`just book\` or a \`just site\` that was killed. It would put the rustdoc into the book. Remove it. See book/src/development.md" >&2
      exit 1
    fi
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
    # A link cannot take its port from a variable, so the book repeats api_port
    # in the links to the API. This check keeps the book and the justfile together.
    for path in swagger-ui/ api-docs/openapi.json; do
      if ! grep -qF "href=\"http://localhost:{{ api_port }}/$path\"" "$out/api-documentation.html"; then
        echo "::error::api-documentation.html has no link to http://localhost:{{ api_port }}/$path. The links of the book to the API use api_port of the justfile, the port that src/main.rs binds. See book/src/development.md" >&2
        exit 1
      fi
    done

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

# ANCHOR: release
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
# ANCHOR_END: release

# Build release binary (static musl)
build-release:
    cargo build --release --target x86_64-unknown-linux-musl

# ANCHOR: image
# Build the deploy image of the server (local only: no push; the arguments go to docker build)
image *build_args: build-release
    #!/usr/bin/env bash
    set -euo pipefail
    # The context holds the binary and nothing else: the target folder is
    # too big to send to the docker daemon.
    context=$(mktemp -d)
    trap 'rm -rf "$context"' EXIT
    cp target/x86_64-unknown-linux-musl/release/sens-o-matic "$context/"
    docker build --file deploy/server/Dockerfile --tag sens-o-matic {{ build_args }} "$context"
# ANCHOR_END: image

# ANCHOR: package
# Package release artifact as tarball
package version:
    mkdir -p staging
    cp target/x86_64-unknown-linux-musl/release/sens-o-matic staging/
    cp LICENSE README.md staging/
    tar -czf "sens-o-matic-{{version}}-x86_64-unknown-linux-musl.tar.gz" -C staging .
    rm -rf staging
# ANCHOR_END: package

# ANCHOR: validate-version
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
# ANCHOR_END: validate-version

# ANCHOR: publish
# Publish to crates.io (token from env: CARGO_REGISTRY_TOKEN)
publish:
    cargo publish
# ANCHOR_END: publish

# Dry-run publish
publish-dry-run:
    cargo publish --dry-run
