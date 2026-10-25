# The session image of this repository. It is the session image of riff
# with the toolchain of the CI (Rust 1.93, clippy and rustfmt) and with
# hurl, the tool of `just test-api`. It holds no riff and no secret.
#
# The workflow "Session image" builds it on each change of this folder,
# and riff.toml pins its digest. FROM pins the image of riff by digest,
# not by tag: a build of this folder gives the same toolchain each time.
FROM ghcr.io/como-technologies/riff-session@sha256:57413e21fc507e6ace6af4f695b3a873218b29f8a94ff66868ae27b178a7ff20

# The channel is the toolchain of `ci.yml` and the rust-version of
# Cargo.toml. The hurl version is pinned with the checksum of its .deb.
ARG RUST_CHANNEL=1.93
ARG HURL_VERSION=8.0.1
ARG HURL_SHA256=e76e6c0957f83f9b761416452871554d0f068384b6cd004bff82dd8795c62225

# The toolchain is in the image, with no write for the session: rustup
# cannot install it later. The base image has another toolchain; this
# one is the default.
RUN rustup toolchain install "$RUST_CHANNEL" --profile minimal --component clippy,rustfmt \
 && rustup default "$RUST_CHANNEL" \
 && chmod -R a+rX "$RUSTUP_HOME" "$CARGO_HOME"

# hurl, from its release. apt installs the libraries that the .deb needs.
RUN curl -fsSLo /tmp/hurl.deb \
      "https://github.com/Orange-OpenSource/hurl/releases/download/${HURL_VERSION}/hurl_${HURL_VERSION}_amd64.deb" \
 && echo "${HURL_SHA256}  /tmp/hurl.deb" | sha256sum -c - \
 && apt-get update \
 && apt-get install -y --no-install-recommends /tmp/hurl.deb \
 && rm -rf /tmp/hurl.deb /var/lib/apt/lists/*
