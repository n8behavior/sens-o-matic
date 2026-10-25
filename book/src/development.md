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
