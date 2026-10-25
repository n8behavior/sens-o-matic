# The deploy image of the server: a static binary and nothing else.
#
# There is no build stage. `just image` runs `just build-release` and puts
# the binary it made in the build context, so the build logic stays in the
# justfile. The binary is static (musl) and the server reads no file, so
# the image needs no libc, no shell and no CA certificates.
FROM scratch

COPY --chmod=0555 sens-o-matic /sens-o-matic

# The server binds 0.0.0.0:3000 (src/main.rs).
EXPOSE 3000

# A numeric user: scratch has no /etc/passwd to resolve a name. 65534 is
# "nobody" by convention.
USER 65534:65534

ENTRYPOINT ["/sens-o-matic"]
